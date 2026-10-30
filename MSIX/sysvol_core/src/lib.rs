macro_rules! lg {
    ($($a:tt)*) => { $crate::log::write(&format!($($a)*)) };
}

mod audio;
mod cfg;
mod focus;
mod log;
mod reg;

use cfg::*;
use std::sync::{
    atomic::{AtomicI32, Ordering},
    Mutex, OnceLock,
};
use std::thread::JoinHandle;
use std::time::Duration;
use windows::{
    core::HSTRING,
    ApplicationModel::{StartupTask, StartupTaskState},
    Win32::Storage::Packaging::Appx::GetCurrentPackageFamilyName,
};
use windows::Win32::{
    Foundation::{ERROR_INSUFFICIENT_BUFFER, LPARAM, WPARAM},
    System::Registry::REG_SZ,
    UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT},
};

static TH: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
static EXE: OnceLock<String> = OnceLock::new();

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_NAME: &str = "SysVolGuard";

// MSIX build: the Run key is redirected to a private hive, so autostart uses the manifest StartupTask.
const TASK_ID: &str = "BarkStartup";
static ST_ON: AtomicI32 = AtomicI32::new(0);

fn packaged() -> bool {
    static P: OnceLock<bool> = OnceLock::new();
    *P.get_or_init(|| {
        let mut n = 0u32;
        unsafe { GetCurrentPackageFamilyName(&mut n, None) == ERROR_INSUFFICIENT_BUFFER }
    })
}

fn task() -> Option<StartupTask> {
    StartupTask::GetAsync(&HSTRING::from(TASK_ID)).ok()?.get().ok()
}

fn task_refresh() {
    let s = task().and_then(|t| t.State().ok());
    let on = s.is_some_and(|s| s == StartupTaskState::Enabled || s == StartupTaskState::EnabledByPolicy);
    lg!("startup task state: {s:?}");
    ST_ON.store(on as i32, Ordering::Relaxed);
}

fn task_apply(on: bool) {
    match task() {
        Some(t) if on => lg!("startup task enable: {:?}", t.RequestEnableAsync().and_then(|op| op.get())),
        Some(t) => lg!("startup task disable: {:?}", t.Disable()),
        None => lg!("startup task '{TASK_ID}' not found"),
    }
    task_refresh();
}

fn task_spawn(f: impl FnOnce() + Send + 'static) {
    let _ = std::thread::Builder::new().stack_size(512 * 1024).spawn(f);
}

fn autostart_get() -> i32 {
    if packaged() {
        return ST_ON.load(Ordering::Relaxed);
    }
    reg::Key::open(RUN, false)
        .and_then(|k| k.raw_str(RUN_NAME))
        .is_some() as i32
}

fn autostart_set(on: bool) {
    if packaged() {
        ST_ON.store(on as i32, Ordering::Relaxed);
        task_spawn(move || task_apply(on));
        return;
    }
    if on {
        if let (Some(k), Some(p)) = (reg::Key::create(RUN), EXE.get()) {
            k.set_str(RUN_NAME, REG_SZ.0, &format!("\"{p}\""));
        }
    } else if let Some(k) = reg::Key::open(RUN, true) {
        k.del_value(RUN_NAME);
    }
}

fn sys_dark() -> i32 {
    reg::Key::open(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", false)
        .and_then(|k| k.dword("AppsUseLightTheme"))
        .map_or(0, |v| (v == 0) as i32)
}

fn clock24() -> i32 {
    let v = cfg::get(K_CLOCK24);
    if v >= 0 {
        return v;
    }
    reg::Key::open(r"Control Panel\International", false)
        .and_then(|k| k.raw_str("sTimeFormat"))
        .map_or(1, |(_, f)| f.contains('H') as i32)
}

// Windows keeps the accent as an 8-colour palette (RGBA each): 0 Light3, 1 Light2, 2 Light1, 3 base, 4 Dark1 ...
// Returns 0x00RRGGBB, or -1 when the palette is not available.
fn accent(k: u32) -> i32 {
    let i = match k {
        K_ACC_L3 => 0,
        K_ACC_L2 => 1,
        K_ACC_L1 => 2,
        _ => 4,
    };
    reg::Key::open(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent", false)
        .and_then(|key| key.bytes("AccentPalette"))
        .and_then(|b| b.get(i * 4..i * 4 + 3).map(|c| ((c[0] as i32) << 16) | ((c[1] as i32) << 8) | c[2] as i32))
        .unwrap_or(-1)
}

fn taskbar_dark() -> i32 {
    reg::Key::open(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", false)
        .and_then(|k| k.dword("SystemUsesLightTheme"))
        .map_or(1, |v| (v == 0) as i32)
}

// Autostart is on by default on first run; afterwards the user's choice is kept.
fn autostart_init() {
    if packaged() {
        task_spawn(task_refresh);
        return;
    }
    let Some(exe) = EXE.get() else { return };
    let want = format!("\"{exe}\"");
    if let Some(k) = reg::Key::create(cfg::ROOT) {
        if k.dword("AutostartInit").is_none() {
            autostart_set(true);
            k.set_dword("AutostartInit", 1);
            return;
        }
    }
    if let Some(k) = reg::Key::open(RUN, true) {
        if let Some((_, cur)) = k.raw_str(RUN_NAME) {
            if cur != want {
                k.set_str(RUN_NAME, REG_SZ.0, &want);
            }
        }
    }
}

/// exe = full path of host exe (UTF-16, NUL-terminated). 0 = ok.
#[no_mangle]
pub unsafe extern "C" fn sv_init(exe: *const u16) -> i32 {
    if exe.is_null() {
        return -1;
    }
    let mut n = 0usize;
    while *exe.add(n) != 0 {
        n += 1;
        if n > 32_768 {
            return -1;
        }
    }
    let path = String::from_utf16_lossy(std::slice::from_raw_parts(exe, n));
    if let Ok(base) = std::env::var("LOCALAPPDATA") {
        log::init(&std::path::Path::new(&base).join("bark"));
    }
    let _ = EXE.set(path);
    cfg::load();
    autostart_init();
    lg!(
        "start packaged={} lock={} ding={} autostart={}",
        packaged(),
        cfg::get(K_LOCK),
        cfg::get(K_DING),
        autostart_get()
    );
    match std::thread::Builder::new()
        .name("sysvol-engine".into())
        .stack_size(256 * 1024)
        .spawn(audio::run)
    {
        Ok(h) => *TH.lock().unwrap() = Some(h),
        Err(_) => return -2,
    }
    for _ in 0..400 {
        if audio::TID.load(Ordering::Acquire) != 0 {
            return 0;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    -3
}

#[no_mangle]
pub extern "C" fn sv_shutdown() {
    let t = audio::TID.load(Ordering::Acquire);
    if t != 0 {
        unsafe {
            let _ = PostThreadMessageW(t, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
    let h = TH.lock().ok().and_then(|mut g| g.take());
    if let Some(h) = h {
        let _ = h.join();
    }
}

#[no_mangle]
pub extern "C" fn sv_get(k: u32) -> i32 {
    match k {
        K_AUTOSTART => autostart_get(),
        K_SYS_DARK => sys_dark(),
        K_TASKBAR_DARK => taskbar_dark(),
        K_CLOCK24 => clock24(),
        K_QUIET_LEFT => audio::sched_left(),
        K_ACC_L3 | K_ACC_L2 | K_ACC_L1 | K_ACC_D1 => accent(k),
        0..=8 | K_DING | K_HIDE_TRAY | K_SYS_ICON | K_WELCOME => cfg::get(k),
        _ => 0,
    }
}

#[no_mangle]
pub extern "C" fn sv_set(k: u32, v: i32) {
    match k {
        K_VOLUME => {
            cfg::put(k, v.clamp(0, 100));
            audio::post(audio::WM_APPLY, 0, 0);
        }
        K_LOCK => {
            cfg::put(k, (v != 0) as i32);
            cfg::save(k);
            if v != 0 {
                audio::post(audio::WM_APPLY, 0, 0);
            }
        }
        K_QUIET_MANUAL | K_SCHED_ON => {
            cfg::put(k, (v != 0) as i32);
            cfg::save(k);
            if k == K_SCHED_ON {
                audio::SKIP.store(false, Ordering::Relaxed);
            }
            let msg = if k == K_QUIET_MANUAL && v == 0 { audio::WM_QOFF } else { audio::WM_EVAL };
            audio::post(msg, 0, 0);
        }
        K_SCHED_START | K_SCHED_END => {
            cfg::put(k, v.clamp(0, 1439));
            cfg::save(k);
            audio::SKIP.store(false, Ordering::Relaxed);
            audio::post(audio::WM_EVAL, 0, 0);
        }
        K_STEP => {
            cfg::put(k, v.clamp(1, 25));
            cfg::save(k);
        }
        K_AUTOSTART => autostart_set(v != 0),
        K_DING | K_HIDE_TRAY | K_CLOCK24 | K_SYS_ICON | K_WELCOME => {
            cfg::put(k, (v != 0) as i32);
            cfg::save(k);
        }
        _ => {}
    }
}

/// Adds delta to the locked volume; returns the new value.
#[no_mangle]
pub extern "C" fn sv_nudge(delta: i32) -> i32 {
    let cur = cfg::get(K_VOLUME);
    let v = (if cur < 0 { 0 } else { cur } + delta).clamp(0, 100);
    sv_set(K_VOLUME, v);
    v
}

/// Windows 11 keeps a per-icon "IsPromoted" flag (visible in the tray vs. hidden-icons overflow). Sets it for this
/// exe's icon. Returns 1 if the icon's entry was found (it appears shortly after the icon is added), else 0.
#[no_mangle]
pub extern "C" fn sv_tray_pin(visible: i32) -> i32 {
    let Some(exe) = EXE.get() else { return 0 };
    let base = r"Control Panel\NotifyIconSettings";
    let Some(root) = reg::Key::open(base, false) else { return 0 };
    let mut found = 0;
    let low = exe.to_ascii_lowercase();
    let tail = low.find(r"\windowsapps\").map_or(String::new(), |i| low[i..].to_string());
    for name in root.subkeys() {
        if let Some(k) = reg::Key::open(&format!(r"{base}\{name}"), true) {
            if let Some((_, p)) = k.raw_str("ExecutablePath") {
                if p.eq_ignore_ascii_case(exe) || (!tail.is_empty() && p.to_ascii_lowercase().ends_with(&tail)) {
                    k.set_dword("IsPromoted", (visible != 0) as u32);
                    found = 1;
                }
            }
        }
    }
    lg!("tray pin visible={visible}: entry found={found}");
    found
}
