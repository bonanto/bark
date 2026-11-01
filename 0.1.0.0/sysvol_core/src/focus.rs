use std::sync::{
    atomic::{AtomicU64, Ordering::Relaxed},
    Mutex, OnceLock,
};
use windows::{
    core::{BOOL, PWSTR},
    Win32::{
        Foundation::{CloseHandle, HWND, LPARAM},
        System::{
            SystemInformation::{GetSystemWindowsDirectoryW, GetTickCount64},
            Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION},
        },
        UI::{
            Input::KeyboardAndMouse::*,
            WindowsAndMessaging::*,
        },
    },
};

static LAST_GESTURE: AtomicU64 = AtomicU64::new(0);
// (hwnd, tick, is_settings, exe name)
static CACHE: Mutex<(isize, u64, bool, String)> = Mutex::new((0, 0, false, String::new()));

fn windir() -> &'static str {
    static W: OnceLock<String> = OnceLock::new();
    W.get_or_init(|| {
        let mut b = [0u16; 260];
        let n = unsafe { GetSystemWindowsDirectoryW(Some(&mut b)) } as usize;
        String::from_utf16_lossy(&b[..n.min(260)]).to_lowercase()
    })
}

fn under_windir(p: &str) -> bool {
    p.strip_prefix(windir()).map_or(false, |r| r.starts_with('\\'))
}

fn pid_of(h: HWND) -> u32 {
    let mut p = 0u32;
    unsafe {
        GetWindowThreadProcessId(h, Some(&mut p));
    }
    p
}

fn exe_of(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        r.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]).to_lowercase())
    }
}

fn is_settings_exe(p: &str) -> bool {
    under_windir(p) && (p.ends_with(r"\systemsettings.exe") || p.ends_with(r"\sndvol.exe"))
}

unsafe extern "system" fn child_cb(h: HWND, lp: LPARAM) -> BOOL {
    let found = &mut *(lp.0 as *mut bool);
    if let Some(p) = exe_of(pid_of(h)) {
        if is_settings_exe(&p) {
            *found = true;
            return BOOL(0);
        }
    }
    BOOL(1)
}

/// (foreground window is Settings/Volume mixer, foreground exe name)
pub fn fg_settings() -> (bool, String) {
    unsafe {
        let h = GetForegroundWindow();
        if h.is_invalid() {
            return (false, String::new());
        }
        let now = GetTickCount64();
        if let Ok(c) = CACHE.lock() {
            if c.0 == h.0 as isize && now.saturating_sub(c.1) < 1500 {
                return (c.2, c.3.clone());
            }
        }
        let p = exe_of(pid_of(h)).unwrap_or_default();
        let mut ok = is_settings_exe(&p);
        if !ok && under_windir(&p) && p.ends_with(r"\applicationframehost.exe") {
            let mut f = false;
            let _ = EnumChildWindows(Some(h), Some(child_cb), LPARAM(&mut f as *mut bool as isize));
            ok = f;
        }
        let name = p.rsplit('\\').next().unwrap_or("").to_string();
        if let Ok(mut c) = CACHE.lock() {
            *c = (h.0 as isize, now, ok, name.clone());
        }
        (ok, name)
    }
}

pub fn mouse_down() -> bool {
    unsafe {
        let vk = if GetSystemMetrics(SM_SWAPBUTTON) != 0 { VK_RBUTTON } else { VK_LBUTTON };
        GetAsyncKeyState(vk.0 as i32) < 0
    }
}

/// True while the user is interacting with Settings (mouse/keys down, or within 300 ms of it).
pub fn gesture() -> bool {
    unsafe {
        if !fg_settings().0 {
            return false;
        }
        let now = GetTickCount64();
        let mouse = if GetSystemMetrics(SM_SWAPBUTTON) != 0 { VK_RBUTTON } else { VK_LBUTTON };
        let keys = [mouse, VK_LEFT, VK_RIGHT, VK_UP, VK_DOWN, VK_PRIOR, VK_NEXT, VK_HOME, VK_END];
        if keys.iter().any(|k| GetAsyncKeyState(k.0 as i32) < 0) {
            LAST_GESTURE.store(now, Relaxed);
            return true;
        }
        now.saturating_sub(LAST_GESTURE.load(Relaxed)) < 300
    }
}