// The actual install / uninstall work. Runs on a worker thread and reports progress through the statics below.
use crate::{payload, reg::*, VERSION};
use std::{
    ffi::c_void,
    fs,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicI32, Ordering::Relaxed},
        Mutex,
    },
    thread::sleep,
    time::Duration,
};
use windows::{
    core::{w, Interface, PCWSTR},
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        System::{
            Com::{CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED},
            Registry::REG_SZ,
        },
        UI::{
            Shell::{IShellLinkW, ShellExecuteW, ShellLink},
            WindowsAndMessaging::{FindWindowW, PostMessageW, SW_SHOWNORMAL, WM_APP, WM_CLOSE},
        },
    },
};

pub const WM_WORK: u32 = WM_APP + 1;
const ROOT: &str = r"Software\{7C1E4B52-9A3D-4F68-B0E2-5D81A6C3F974}";
const UNINST: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{7C1E4B52-9A3D-4F68-B0E2-5D81A6C3F974}";
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_NAME: &str = "SysVolGuard"; // the value name Bark itself uses
const SETUP_NAME: &str = "BarkSetup.exe";
const NO_WINDOW: u32 = 0x0800_0000;

pub static PROGRESS: AtomicI32 = AtomicI32::new(0);
/// 0 = running, 1 = finished, 2 = failed
pub static STATE: AtomicI32 = AtomicI32::new(0);
/// A folder that could not be deleted while setup was running; main() removes it after exit.
pub static LEFTOVER: Mutex<Option<PathBuf>> = Mutex::new(None);
static STATUS: Mutex<String> = Mutex::new(String::new());
static ERROR: Mutex<String> = Mutex::new(String::new());

pub fn status() -> String {
    STATUS.lock().map(|s| s.clone()).unwrap_or_default()
}
pub fn error() -> String {
    ERROR.lock().map(|s| s.clone()).unwrap_or_default()
}

#[derive(Clone, Copy)]
pub struct Settings {
    pub ding: bool,
    pub updates: bool,
    pub autostart: bool,
}

pub fn default_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| r"C:\Users\Public\AppData\Local".to_string());
    PathBuf::from(base).join("Programs").join("Bark")
}

fn lnk_path() -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_default();
    PathBuf::from(base).join(r"Microsoft\Windows\Start Menu\Programs\Bark.lnk")
}

/// (version, folder) of the installed Bark, if any.
pub fn existing() -> Option<(String, PathBuf)> {
    let k = Key::open(UNINST, false)?;
    let ver = k.raw_str("DisplayVersion")?.1;
    let dir = PathBuf::from(k.raw_str("InstallLocation")?.1);
    if dir.join("bark.exe").exists() {
        Some((ver, dir))
    } else {
        None
    }
}

/// The user's current settings (used to pre-fill the Custom page when Bark is already installed).
pub fn read_settings() -> Settings {
    let k = Key::open(ROOT, false);
    let get = |n: &str, d: bool| k.as_ref().and_then(|k| k.dword(n)).map_or(d, |v| v != 0);
    let auto = Key::open(RUN, false).and_then(|k| k.raw_str(RUN_NAME)).is_some();
    Settings { ding: get("DingMute", true), updates: get("UpdateCheck", true), autostart: auto }
}

pub fn log_path() -> PathBuf {
    std::env::temp_dir().join("BarkSetup.log")
}

/// Appends a line to %TEMP%\BarkSetup.log (best effort), so a failed install can be diagnosed.
pub fn log(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = writeln!(f, "{msg}");
    }
}

/// Starts the installed Bark (called from the final page when "Start Bark after Installation" is ticked).
pub fn launch_bark(dir: &Path) {
    let exe = dir.join("bark.exe");
    let (f, d) = (w(&exe.to_string_lossy()), w(&dir.to_string_lossy()));
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let _ = ShellExecuteW(None, w!("open"), PCWSTR(f.as_ptr()), w!("--first-run"), PCWSTR(d.as_ptr()), SW_SHOWNORMAL);
    }
}

fn step(hwnd: isize, pct: i32, msg: &str) {
    log(&format!("{pct:>3}%  {msg}"));
    PROGRESS.store(pct, Relaxed);
    if let Ok(mut s) = STATUS.lock() {
        *s = msg.to_string();
    }
    notify(hwnd);
}

fn notify(hwnd: isize) {
    unsafe {
        let _ = PostMessageW(Some(HWND(hwnd as *mut c_void)), WM_WORK, WPARAM(0), LPARAM(0));
    }
}

fn finish(hwnd: isize, r: Result<(), String>) {
    match r {
        Ok(()) => {
            PROGRESS.store(100, Relaxed);
            STATE.store(1, Relaxed);
        }
        Err(e) => {
            log(&format!("FAILED: {e}"));
            if let Ok(mut s) = ERROR.lock() {
                *s = e;
            }
            STATE.store(2, Relaxed);
        }
    }
    notify(hwnd);
}

/// Asks a running Bark to exit. Only if its window is still there afterwards is it force-closed.
/// (When Bark is not running - e.g. a first install - no other program is started at all.)
pub fn close_bark() {
    log("close_bark: looking for Bark's window");
    let mut running = false;
    let mut gone = false;
    unsafe {
        for _ in 0..30 {
            match FindWindowW(w!("BarkWnd"), PCWSTR::null()) {
                Ok(h) => {
                    running = true;
                    log("close_bark: asking Bark to close");
                    let _ = PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0));
                    sleep(Duration::from_millis(150));
                }
                Err(_) => {
                    gone = true;
                    break;
                }
            }
        }
    }
    if !running {
        log("close_bark: Bark is not running");
        return;
    }
    if !gone {
        log("close_bark: Bark did not close, forcing it");
        // Null std handles: this is a GUI-subsystem program, so it has no valid ones to hand to the child.
        let _ = Command::new("taskkill")
            .args(["/F", "/IM", "bark.exe"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(NO_WINDOW)
            .status();
    }
    sleep(Duration::from_millis(300));
    log("close_bark: done");
}

fn write_retry(path: &Path, data: &[u8]) -> Result<(), String> {
    let mut last = String::new();
    for _ in 0..8 {
        match fs::write(path, data) {
            Ok(()) => return Ok(()),
            Err(e) => last = e.to_string(),
        }
        sleep(Duration::from_millis(300));
    }
    Err(format!("Could not write {}: {last}", path.display()))
}

fn make_shortcut(lnk: &Path, target: &Path, dir: &Path) -> windows::core::Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let sl: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        let t = w(&target.to_string_lossy());
        let d = w(&dir.to_string_lossy());
        let desc = w("Bark - System Volume Fix");
        sl.SetPath(PCWSTR(t.as_ptr()))?;
        sl.SetWorkingDirectory(PCWSTR(d.as_ptr()))?;
        sl.SetDescription(PCWSTR(desc.as_ptr()))?;
        sl.SetIconLocation(PCWSTR(t.as_ptr()), 0)?;
        let pf: IPersistFile = sl.cast()?;
        let l = w(&lnk.to_string_lossy());
        pf.Save(PCWSTR(l.as_ptr()), true)?;
    }
    Ok(())
}

// ---------------------------------------------------------------- install

fn guarded(f: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .unwrap_or_else(|_| Err(format!("Setup hit an unexpected error. Details were saved to {}", log_path().display())))
}

/// Reports a failure that happened before the worker could even start.
pub fn fail(hwnd: isize, msg: &str) {
    finish(hwnd, Err(msg.to_string()));
}

pub fn install(hwnd: isize, s: Settings, dir: PathBuf) {
    log(&format!("--- install {VERSION} to {}", dir.display()));
    let r = guarded(|| install_inner(hwnd, &s, &dir));
    finish(hwnd, r);
}

fn install_inner(hwnd: isize, s: &Settings, dir: &Path) -> Result<(), String> {
    step(hwnd, 4, "Closing Bark...");
    close_bark();

    if existing().is_some() {
        // Modify / Repair = clean the old installation, then install again as new.
        step(hwnd, 7, "Cleaning the previous installation...");
        wipe_registration(dir);
        if let Ok(rd) = fs::read_dir(dir) {
            for e in rd.flatten() {
                let keep = e.file_name().to_string_lossy().eq_ignore_ascii_case(SETUP_NAME); // this may be the running setup
                if !keep && e.path().is_file() {
                    let _ = fs::remove_file(e.path());
                }
            }
        }
    }

    step(hwnd, 10, "Preparing files...");
    let files = payload::files_for_this_pc()?;
    fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;

    let n = files.len().max(1);
    let mut bytes = 0usize;
    for (i, f) in files.iter().enumerate() {
        step(hwnd, 15 + (60 * i / n) as i32, &format!("Copying {}...", f.name));
        write_retry(&dir.join(&f.name), &f.data)?;
        bytes += f.data.len();
    }

    step(hwnd, 78, "Adding the uninstaller...");
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let dst = dir.join(SETUP_NAME);
    let same = fs::canonicalize(&me).ok().is_some() && fs::canonicalize(&me).ok() == fs::canonicalize(&dst).ok();
    if !same {
        fs::copy(&me, &dst).map_err(|e| format!("Could not copy the setup program: {e}"))?;
    }
    bytes += fs::metadata(&dst).map(|m| m.len() as usize).unwrap_or(0);

    step(hwnd, 84, "Saving settings...");
    let exe = dir.join("bark.exe");
    if let Some(k) = Key::create(ROOT) {
        k.set_dword("DingMute", s.ding as u32);
        k.set_dword("UpdateCheck", s.updates as u32);
        k.set_dword("AutostartInit", 1); // Bark must not switch autostart on by itself
    }
    if s.autostart {
        if let Some(k) = Key::create(RUN) {
            k.set_str(RUN_NAME, REG_SZ.0, &format!("\"{}\"", exe.display()));
        }
    } else if let Some(k) = Key::open(RUN, true) {
        k.del_value(RUN_NAME);
    }
    if let Some(k) = Key::create(UNINST) {
        let setup = dir.join(SETUP_NAME);
        k.set_str("DisplayName", REG_SZ.0, "Bark");
        k.set_str("DisplayVersion", REG_SZ.0, VERSION);
        k.set_str("Publisher", REG_SZ.0, "Bonanto");
        k.set_str("InstallLocation", REG_SZ.0, &dir.to_string_lossy());
        k.set_str("DisplayIcon", REG_SZ.0, &exe.to_string_lossy());
        k.set_str("UninstallString", REG_SZ.0, &format!("\"{}\" /uninstall", setup.display()));
        k.set_str("ModifyPath", REG_SZ.0, &format!("\"{}\" /modify", setup.display()));
        k.set_str("URLInfoAbout", REG_SZ.0, "https://github.com/bonanto/bark");
        k.set_dword("NoModify", 0);
        k.set_dword("NoRepair", 1);
        k.set_dword("EstimatedSize", (bytes / 1024) as u32);
    }

    step(hwnd, 92, "Creating the Start menu shortcut...");
    let lnk = lnk_path();
    if let Some(p) = lnk.parent() {
        let _ = fs::create_dir_all(p);
    }
    let _ = make_shortcut(&lnk, &exe, dir); // not fatal

    // Bark is started from the final page (only if the user leaves the checkbox ticked), not from here.
    step(hwnd, 98, "Finishing...");
    Ok(())
}

// -------------------------------------------------------------- uninstall

/// Puts back the Windows sounds if Quiet mode was on. Same logic as Bark's own quiet::leave().
fn restore_sounds() {
    const SCHEMES: &str = r"AppEvents\Schemes";
    const APPS: &str = r"AppEvents\Schemes\Apps";
    let bk_path = format!(r"{ROOT}\SoundBackup");
    let Some(bk) = Key::open(&bk_path, true) else { return };
    if let Some(apps) = Key::open(APPS, false) {
        for a in apps.subkeys() {
            let ak = format!(r"{APPS}\{a}");
            let Some(k) = Key::open(&ak, false) else { continue };
            for e in k.subkeys() {
                let name = format!("{a}|{e}");
                if let Some((ty, val)) = bk.raw_str(&name) {
                    if let Some(ek) = Key::open(&format!(r"{ak}\{e}\.Current"), true) {
                        ek.set_str("", ty, &val);
                    }
                    bk.del_value(&name);
                }
            }
        }
    }
    if let Some((_, scheme)) = bk.raw_str("__scheme") {
        if let Some(k) = Key::open(SCHEMES, true) {
            k.set_str("", REG_SZ.0, &scheme);
        }
        bk.del_value("__scheme");
    }
}

/// Removes everything Bark registered (the same things reset.ps1 clears), after giving the Windows sounds back.
fn wipe_registration(dir: &Path) {
    restore_sounds();
    delete_tree(ROOT);
    if let Some(k) = Key::open(RUN, true) {
        k.del_value(RUN_NAME);
    }
    // Windows' own record of where the tray icon is pinned
    let exe = dir.join("bark.exe").to_string_lossy().to_lowercase();
    if let Some(nis) = Key::open(r"Control Panel\NotifyIconSettings", false) {
        for id in nis.subkeys() {
            let p = format!(r"Control Panel\NotifyIconSettings\{id}");
            let hit = Key::open(&p, false).and_then(|k| k.raw_str("ExecutablePath")).map_or(false, |(_, v)| v.to_lowercase() == exe);
            if hit {
                delete_tree(&p);
            }
        }
    }
    delete_tree(UNINST);
    let _ = fs::remove_file(lnk_path());
}

/// Deletes the files in `dir` (and, with `recurse`, everything below it). Never stops at a file that cannot be deleted.
fn purge(dir: &Path, recurse: bool) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() && !ft.is_symlink() {
            if recurse {
                purge(&p, true);
                let _ = fs::remove_dir(&p);
            }
        } else {
            if let Ok(m) = fs::metadata(&p) {
                let mut perm = m.permissions();
                if perm.readonly() {
                    perm.set_readonly(false);
                    let _ = fs::set_permissions(&p, perm);
                }
            }
            if fs::remove_file(&p).is_err() {
                let _ = fs::remove_dir(&p); // a directory symlink / junction
            }
        }
    }
}

pub fn uninstall(hwnd: isize, clear_logs: bool, dir: PathBuf) {
    log("--- uninstall");
    let r = guarded(|| uninstall_inner(hwnd, clear_logs, &dir));
    finish(hwnd, r);
}

fn uninstall_inner(hwnd: isize, clear_logs: bool, dir: &Path) -> Result<(), String> {
    step(hwnd, 8, "Closing Bark...");
    close_bark();

    step(hwnd, 25, "Restoring your Windows sounds...");
    restore_sounds();

    step(hwnd, 45, "Removing settings...");
    wipe_registration(dir);

    step(hwnd, 70, "Deleting files...");
    // Setup itself runs from a copy in %TEMP% (see main), so even BarkSetup.exe in the folder can go.
    // Files that are still locked (antivirus scan, Explorer) are retried for a few seconds.
    for _ in 0..12 {
        purge(dir, clear_logs);
        if clear_logs {
            let _ = fs::remove_dir(dir);
            if !dir.exists() {
                break;
            }
        } else if !fs::read_dir(dir).map_or(false, |rd| rd.flatten().any(|e| e.path().is_file())) {
            break;
        }
        sleep(Duration::from_millis(400));
    }
    if clear_logs && dir.exists() {
        // Something still holds the folder itself (its contents are already gone): main() removes it right after
        // this program has exited.
        log("folder still in use, will be removed after setup exits");
        if let Ok(mut l) = LEFTOVER.lock() {
            *l = Some(dir.to_path_buf());
        }
    }
    if !clear_logs {
        let _ = fs::remove_dir(dir); // only succeeds when no log folder is left
    }
    Ok(())
}
