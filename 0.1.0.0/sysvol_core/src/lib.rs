macro_rules! lg {
    ($($a:tt)*) => { $crate::log::write(&format!($($a)*)) };
}

mod audio;
mod cfg;
mod focus;
mod log;
mod quiet;
mod reg;
mod update;

use cfg::*;
use std::sync::{atomic::Ordering, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;
use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    System::Registry::REG_SZ,
    UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT},
};

static TH: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
static EXE: OnceLock<String> = OnceLock::new();

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_NAME: &str = "SysVolGuard";

fn autostart_get() -> i32 {
    reg::Key::open(RUN, false)
        .and_then(|k| k.raw_str(RUN_NAME))
        .is_some() as i32
}

fn autostart_set(on: bool) {
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

// Autostart is on by default on first run; afterwards the user's choice is kept.
fn autostart_init() {
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
    if let Some(d) = std::path::Path::new(&path).parent() {
        log::init(d);
    }
    let _ = EXE.set(path);
    cfg::load();
    autostart_init();
    lg!("start");
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
        0..=8 | K_DING | K_UPDATE_AVAIL => cfg::get(k),
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
            audio::post(audio::WM_EVAL, 0, 0);
        }
        K_SCHED_START | K_SCHED_END => {
            cfg::put(k, v.clamp(0, 1439));
            cfg::save(k);
            audio::post(audio::WM_EVAL, 0, 0);
        }
        K_STEP => {
            cfg::put(k, v.clamp(1, 25));
            cfg::save(k);
        }
        K_AUTOSTART => autostart_set(v != 0),
        K_DING => {
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