#![windows_subsystem = "windows"]

mod gfx;
mod ui;

use std::{
    ffi::c_void,
    mem::{size_of, transmute},
    sync::{
        atomic::{AtomicIsize, AtomicU32, AtomicU64, Ordering::Relaxed},
        OnceLock,
    },
};
use windows::{
    core::{s, w, PCWSTR},
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        System::{LibraryLoader::*, SystemInformation::*, Threading::*},
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
    },
};

pub const APP_VERSION: &str = "1.0";

// ---- core DLL API (loaded at runtime from the exe folder) ----
struct Api {
    init: unsafe extern "C" fn(*const u16) -> i32,
    shutdown: extern "C" fn(),
    get: extern "C" fn(u32) -> i32,
    set: extern "C" fn(u32, i32),
    nudge: extern "C" fn(i32) -> i32,
}
static API: OnceLock<Api> = OnceLock::new();
fn api() -> &'static Api {
    API.get().unwrap()
}

const K_VOLUME: u32 = 0;
const K_LOCK: u32 = 1;
const K_QUIET_MANUAL: u32 = 2;
const K_SCHED_ON: u32 = 3;
const K_SCHED_START: u32 = 4;
const K_SCHED_END: u32 = 5;
const K_STEP: u32 = 6;
const K_RESETS: u32 = 7;
const K_AUTOSTART: u32 = 9;
const K_DING: u32 = 10;
const K_UPDATE_AVAIL: u32 = 11;
const K_SYS_DARK: u32 = 12;

const WM_TRAY: u32 = WM_APP + 1;
const WM_MOUSELEAVE: u32 = 0x02A3;

static LAST_HIDE: AtomicU64 = AtomicU64::new(0);
static TB_CREATED: AtomicU32 = AtomicU32::new(0);
static LOGDIR: OnceLock<Vec<u16>> = OnceLock::new();
static ICON_INST: AtomicIsize = AtomicIsize::new(0);

fn xy(lp: LPARAM) -> (i32, i32) {
    ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32)
}

unsafe fn trim() {
    let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
}

unsafe fn hide_fly(h: HWND) {
    ui::on_hide(h);
    let _ = ShowWindow(h, SW_HIDE);
    LAST_HIDE.store(GetTickCount64(), Relaxed);
    trim();
}

unsafe fn toggle(h: HWND) {
    if IsWindowVisible(h).as_bool() {
        hide_fly(h);
    } else if GetTickCount64().saturating_sub(LAST_HIDE.load(Relaxed)) > 300 {
        ui::on_show(h);
    }
}

unsafe fn open_log() {
    if let Some(p) = LOGDIR.get() {
        let _ = ShellExecuteW(None, w!("open"), PCWSTR(p.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

unsafe fn tray_icon() -> HICON {
    let mut b = [0u16; 260];
    let n = GetSystemDirectoryW(Some(&mut b)) as usize;
    let mut p = b[..n].to_vec();
    p.extend("\\SndVol.exe".encode_utf16());
    p.push(0);
    let hi = HINSTANCE(ICON_INST.load(Relaxed) as *mut c_void);
    let i = ExtractIconW(Some(hi), PCWSTR(p.as_ptr()), 0);
    if i.is_invalid() {
        LoadIconW(None, IDI_APPLICATION).unwrap_or_default()
    } else {
        i
    }
}

unsafe fn tray(h: HWND, add: bool) {
    let mut nid = NOTIFYICONDATAW::default();
    nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = h;
    nid.uID = 1;
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    if add {
        nid.hIcon = tray_icon();
    }
    for (i, c) in "Bark - System Volume Fix".encode_utf16().enumerate() {
        nid.szTip[i] = c;
    }
    let _ = Shell_NotifyIconW(if add { NIM_ADD } else { NIM_DELETE }, &nid);
}

unsafe fn menu(h: HWND) {
    let Ok(mn) = CreatePopupMenu() else { return };
    let _ = AppendMenuW(mn, MF_STRING, 203, w!("Open Bark"));
    let _ = AppendMenuW(mn, MF_STRING, 202, w!("Open log folder"));
    let _ = AppendMenuW(mn, MF_STRING, 201, w!("Exit"));
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let _ = SetForegroundWindow(h);
    let cmd = TrackPopupMenu(mn, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, pt.x, pt.y, Some(0), h, None).0;
    let _ = PostMessageW(Some(h), WM_NULL, WPARAM(0), LPARAM(0));
    let _ = DestroyMenu(mn);
    match cmd {
        201 => {
            let _ = DestroyWindow(h);
        }
        202 => open_log(),
        203 => ui::on_show(h),
        _ => {}
    }
}

unsafe extern "system" fn wndproc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_TRAY => {
            match (lp.0 as u32) & 0xFFFF {
                WM_LBUTTONUP => toggle(h),
                WM_RBUTTONUP => menu(h),
                _ => {}
            }
            LRESULT(0)
        }
        WM_HOTKEY => {
            let a = api();
            match wp.0 {
                1 => {
                    let _ = (a.nudge)((a.get)(K_STEP));
                }
                2 => {
                    let _ = (a.nudge)(-(a.get)(K_STEP));
                }
                3 => (a.set)(K_QUIET_MANUAL, 1 - (a.get)(K_QUIET_MANUAL)),
                _ => {}
            }
            if IsWindowVisible(h).as_bool() {
                let _ = InvalidateRect(Some(h), None, false);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(h, &mut ps);
            ui::paint(h, dc);
            let _ = EndPaint(h, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEMOVE => {
            let (x, y) = xy(lp);
            ui::mouse_move(h, x, y);
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            ui::mouse_leave(h);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = xy(lp);
            ui::mouse_down(h, x, y);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = xy(lp);
            ui::mouse_up(h, x, y);
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let (x, y) = xy(lp);
            ui::right_up(h, x, y);
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            ui::capture_lost();
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
            let (sx, sy) = xy(lp);
            let mut pt = POINT { x: sx, y: sy };
            let _ = ScreenToClient(h, &mut pt);
            ui::wheel(h, pt.x, pt.y, delta);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wp.0 == 0x1B {
                hide_fly(h);
            } else {
                ui::key(h, wp.0 as u32);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            match wp.0 {
                ui::T_TIP => ui::tip_timer(h),
                ui::T_REFRESH => {
                    if IsWindowVisible(h).as_bool() {
                        let _ = InvalidateRect(Some(h), None, false);
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            if (wp.0 & 0xFFFF) == 0 && IsWindowVisible(h).as_bool() {
                hide_fly(h);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            tray(h, false);
            (api().shutdown)();
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => {
            let tb = TB_CREATED.load(Relaxed);
            if tb != 0 && m == tb {
                tray(h, true);
            }
            DefWindowProcW(h, m, wp, lp)
        }
    }
}

unsafe fn load_api(dll: &[u16]) -> Option<Api> {
    let h = LoadLibraryExW(PCWSTR(dll.as_ptr()), None, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32).ok()?;
    macro_rules! sym {
        ($n:literal) => {
            transmute(GetProcAddress(h, s!($n))?)
        };
    }
    Some(Api {
        init: sym!("sv_init"),
        shutdown: sym!("sv_shutdown"),
        get: sym!("sv_get"),
        set: sym!("sv_set"),
        nudge: sym!("sv_nudge"),
    })
}

fn main() {
    unsafe { run() }
}

unsafe fn run() {
    let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

    let Ok(_mx) = CreateMutexW(None, false, w!("Local\\Bark-7C1E4B52-9A3D-4F68-B0E2-5D81A6C3F974")) else {
        return;
    };
    if GetLastError() == ERROR_ALREADY_EXISTS {
        return;
    }

    let mut buf = [0u16; 1024];
    let n = GetModuleFileNameW(None, &mut buf) as usize;
    let mut exe: Vec<u16> = buf[..n].to_vec();
    let cut = exe.iter().rposition(|&c| c == b'\\' as u16).unwrap_or(0);
    let mut dll: Vec<u16> = exe[..cut].to_vec();
    let mut logd = dll.clone();
    logd.extend("\\log".encode_utf16());
    logd.push(0);
    let _ = LOGDIR.set(logd);
    dll.extend("\\sysvol.dll".encode_utf16());
    dll.push(0);
    exe.push(0);

    let Some(a) = load_api(&dll) else {
        MessageBoxW(None, w!("sysvol.dll is missing or invalid (it must sit next to bark.exe)."), w!("Bark"), MB_ICONERROR | MB_OK);
        return;
    };
    let _ = API.set(a);
    if (api().init)(exe.as_ptr()) != 0 {
        MessageBoxW(None, w!("The core engine failed to start."), w!("Bark"), MB_ICONERROR | MB_OK);
        return;
    }

    let Ok(hm) = GetModuleHandleW(None) else { return };
    let hi = HINSTANCE(hm.0);
    ICON_INST.store(hi.0 as isize, Relaxed);
    TB_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Relaxed);

    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_DROPSHADOW,
        lpfnWndProc: Some(wndproc),
        hInstance: hi,
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        lpszClassName: w!("BarkWnd"),
        ..Default::default()
    };
    if RegisterClassExW(&wc) == 0 {
        return;
    }
    let tc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_DROPSHADOW,
        lpfnWndProc: Some(ui::tip_proc),
        hInstance: hi,
        lpszClassName: w!("BarkTip"),
        ..Default::default()
    };
    RegisterClassExW(&tc);

    let Ok(hwnd) = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        w!("BarkWnd"),
        w!("Bark"),
        WS_POPUP,
        0,
        0,
        10,
        10,
        None,
        None,
        Some(hi),
        None,
    ) else {
        return;
    };
    let tw = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
        w!("BarkTip"),
        w!(""),
        WS_POPUP,
        0,
        0,
        10,
        10,
        None,
        None,
        Some(hi),
        None,
    )
    .unwrap_or_default();
    ui::set_tip(tw);

    let round: u32 = 2; // DWMWCP_ROUND
    let _ = DwmSetWindowAttribute(hwnd, DWMWINDOWATTRIBUTE(33), &round as *const u32 as *const c_void, 4);

    tray(hwnd, true);
    let mods = MOD_CONTROL | MOD_ALT;
    let _ = RegisterHotKey(Some(hwnd), 1, mods, 0x21); // Ctrl+Alt+PgUp
    let _ = RegisterHotKey(Some(hwnd), 2, mods, 0x22); // Ctrl+Alt+PgDn
    let _ = RegisterHotKey(Some(hwnd), 3, mods, 0x51); // Ctrl+Alt+Q
    trim();

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}