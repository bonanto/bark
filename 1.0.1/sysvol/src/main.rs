#![windows_subsystem = "windows"]

use std::{
    ffi::c_void,
    mem::{size_of, transmute},
    sync::{
        atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, AtomicU64, Ordering::Relaxed},
        OnceLock,
    },
};
use windows::{
    core::{s, w, BOOL, GUID, PCWSTR},
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        System::{LibraryLoader::*, SystemInformation::*, Threading::*},
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
    },
};

// ---- core DLL API (loaded at runtime from the exe folder) ----
struct Api {
    init: unsafe extern "C" fn(*const u16) -> i32,
    shutdown: extern "C" fn(),
    get: extern "C" fn(u32) -> i32,
    set: extern "C" fn(u32, i32),
    nudge: extern "C" fn(i32) -> i32,
    tray_pin: extern "C" fn(i32) -> i32,
}
static API: OnceLock<Api> = OnceLock::new();
fn api() -> &'static Api {
    API.get().unwrap()
}

// ---- UI DLL (bui.dll, loaded at startup from the exe folder) ----
struct Ui {
    init: extern "C" fn(extern "C" fn(u32) -> i32, extern "C" fn(u32, i32)),
    on_show: extern "C" fn(isize),
    begin_hide: extern "C" fn(isize) -> i32,
    paint: extern "C" fn(isize, isize),
    timer: extern "C" fn(isize, usize),
    mouse_move: extern "C" fn(isize, i32, i32),
    mouse_leave: extern "C" fn(isize),
    mouse_down: extern "C" fn(isize, i32, i32),
    mouse_up: extern "C" fn(isize, i32, i32),
    capture_lost: extern "C" fn(),
    wheel: extern "C" fn(isize, i32, i32, i32),
    key: extern "C" fn(isize, u32),
    set_tip: extern "C" fn(isize),
    tip_wndproc: extern "C" fn() -> usize,
    icon_font: extern "C" fn(i32) -> isize,
    tray_icon: extern "C" fn(i32) -> isize,
}
static UI: OnceLock<Ui> = OnceLock::new();
fn ui() -> &'static Ui {
    UI.get().unwrap()
}

fn hv(h: HWND) -> isize {
    h.0 as isize
}

const K_QUIET_MANUAL: u32 = 2;
const K_STEP: u32 = 6;
const K_HIDE_TRAY: u32 = 14;
const K_TASKBAR_DARK: u32 = 16;
const K_SYS_ICON: u32 = 17;
const K_WELCOME: u32 = 22;

// DLL names per architecture (the exe and its two DLLs must have the same bitness).
#[cfg(target_arch = "x86_64")]
const CORE_DLL: &str = "\\sysvol64.dll";
#[cfg(target_arch = "x86_64")]
const UI_DLL: &str = "\\bui64.dll";
#[cfg(target_arch = "x86")]
const CORE_DLL: &str = "\\sysvol.dll";
#[cfg(target_arch = "x86")]
const UI_DLL: &str = "\\bui86.dll";
#[cfg(target_arch = "aarch64")]
const CORE_DLL: &str = "\\sysvola64.dll";
#[cfg(target_arch = "aarch64")]
const UI_DLL: &str = "\\buia64.dll";

const WM_TRAY: u32 = WM_APP + 1;
const WM_SHOWME: u32 = WM_APP + 2;
const WM_CLOSED: u32 = WM_APP + 3;
const WM_BRAND: u32 = WM_APP + 4;
const T_PIN: usize = 100;
const T_BALLOON: usize = 101;
const NIN_BALLOONUSERCLICK: u32 = 0x0405;
const WM_MOUSELEAVE: u32 = 0x02A3;
const TRAY_GUID: GUID = GUID::from_u128(0x7c1e4b52_9a3d_4f68_b0e2_5d81a6c3f975);
const TRAY_GLYPH: u16 = 0xF4C3; //-- OR F0ED USE. 

static LAST_HIDE: AtomicU64 = AtomicU64::new(0);
static TB_CREATED: AtomicU32 = AtomicU32::new(0);
static LOGDIR: OnceLock<Vec<u16>> = OnceLock::new();
static TRAY_ON: AtomicBool = AtomicBool::new(false);
static TRAY_GUID_OK: AtomicBool = AtomicBool::new(true);
static LAST_TB_DARK: AtomicI32 = AtomicI32::new(-1);
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static PIN_TRIES: AtomicU32 = AtomicU32::new(0);

// The UI calls this instead of the core's setter, so a change of the icon style updates the tray at once.
extern "C" fn set_wrap(k: u32, v: i32) {
    (api().set)(k, v);
    if k == K_SYS_ICON {
        let h = MAIN_HWND.load(Relaxed);
        if h != 0 {
            unsafe {
                let _ = PostMessageW(Some(HWND(h as *mut c_void)), WM_BRAND, WPARAM(0), LPARAM(0));
            }
        }
    }
}

fn xy(lp: LPARAM) -> (i32, i32) {
    ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32)
}

unsafe fn trim() {
    let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
}

// ---- tray icon (flat Segoe glyph, follows the taskbar theme) ----

unsafe fn flat_icon() -> Option<HICON> {
    let sz = GetSystemMetrics(SM_CXSMICON).max(16);
    let fg: u32 = if (api().get)(K_TASKBAR_DARK) != 0 { 0x00FF_FFFF } else { 0 };
    let hdc = GetDC(None);
    let mdc = CreateCompatibleDC(Some(hdc));
    let mut bi = BITMAPINFO::default();
    bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = sz;
    bi.bmiHeader.biHeight = -sz;
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    let mut bits: *mut c_void = std::ptr::null_mut();
    let mut icon = None;
    if let Ok(bmp) = CreateDIBSection(Some(hdc), &bi, DIB_RGB_COLORS, &mut bits, None, 0) {
        if !bits.is_null() {
            let old = SelectObject(mdc, bmp.into());
            let f = (ui().icon_font)(sz);
            let prev = SelectObject(mdc, HGDIOBJ(f as *mut c_void));
            SetBkMode(mdc, TRANSPARENT);
            SetTextColor(mdc, COLORREF(0x00FF_FFFF));
            let mut g = [TRAY_GLYPH];
            let mut rc = RECT { left: 0, top: 0, right: sz, bottom: sz };
            DrawTextW(mdc, &mut g, &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
            let _ = GdiFlush();
            SelectObject(mdc, prev);
            let _ = DeleteObject(HGDIOBJ(f as *mut c_void));
            for px in std::slice::from_raw_parts_mut(bits as *mut u32, (sz * sz) as usize) {
                *px = (((*px >> 8) & 0xFF) << 24) | fg;
            }
            let stride = (((sz + 15) / 16) * 2) as usize;
            let zero = vec![0u8; stride * sz as usize];
            let mask = CreateBitmap(sz, sz, 1, 1, Some(zero.as_ptr() as *const c_void));
            let ii = ICONINFO { fIcon: BOOL(1), xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: bmp };
            icon = CreateIconIndirect(&ii).ok();
            let _ = DeleteObject(mask.into());
            SelectObject(mdc, old);
        }
        let _ = DeleteObject(bmp.into());
    }
    let _ = DeleteDC(mdc);
    ReleaseDC(None, hdc);
    icon
}

// Returns the icon and whether we own it (and must destroy it).
unsafe fn icon_pair() -> (HICON, bool) {
    if (api().get)(K_SYS_ICON) == 0 {
        let sz = GetSystemMetrics(SM_CXSMICON).max(16);
        let h = (ui().tray_icon)(sz);
        if h != 0 {
            return (HICON(h as *mut c_void), true);
        }
    }
    match flat_icon() {
        Some(i) => (i, true),
        None => (LoadIconW(None, IDI_APPLICATION).unwrap_or_default(), false),
    }
}

fn nid(h: HWND) -> NOTIFYICONDATAW {
    let mut n = NOTIFYICONDATAW::default();
    n.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    n.hWnd = h;
    n.uID = 1;
    n.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    n.uCallbackMessage = WM_TRAY;
    if TRAY_GUID_OK.load(Relaxed) {
        n.uFlags = n.uFlags | NIF_GUID;
        n.guidItem = TRAY_GUID;
    }
    for (i, c) in "Bark - System Volume Fix".encode_utf16().enumerate() {
        n.szTip[i] = c;
    }
    n
}

unsafe fn tray_add(h: HWND) {
    let (icon, own) = icon_pair();
    LAST_TB_DARK.store((api().get)(K_TASKBAR_DARK), Relaxed);
    TRAY_GUID_OK.store(true, Relaxed);
    let mut n = nid(h);
    n.hIcon = icon;
    let mut ok = Shell_NotifyIconW(NIM_ADD, &n).as_bool();
    if !ok {
        TRAY_GUID_OK.store(false, Relaxed);
        let mut n2 = nid(h);
        n2.hIcon = icon;
        ok = Shell_NotifyIconW(NIM_ADD, &n2).as_bool();
    }
    TRAY_ON.store(ok, Relaxed);
    if own {
        let _ = DestroyIcon(icon);
    }
    if ok {
        pin_start(h);
    }
}

// "Bark is running" balloon with the Bark icon; clicking it opens the flyout.
unsafe fn balloon(h: HWND) {
    if !TRAY_ON.load(Relaxed) {
        return;
    }
    let ic = (ui().tray_icon)(GetSystemMetrics(SM_CXICON).max(32));
    let mut n = nid(h);
    n.uFlags = NIF_INFO | NIF_MESSAGE;
    if TRAY_GUID_OK.load(Relaxed) {
        n.uFlags = n.uFlags | NIF_GUID;
    }
    for (i, c) in "Bark".encode_utf16().enumerate() {
        n.szInfoTitle[i] = c;
    }
    for (i, c) in "Bark is running. Configure here.".encode_utf16().enumerate() {
        n.szInfo[i] = c;
    }
    n.dwInfoFlags = NIIF_NOSOUND;
    if ic != 0 {
        n.dwInfoFlags = NIIF_USER | NIIF_LARGE_ICON | NIIF_NOSOUND;
        n.hBalloonIcon = HICON(ic as *mut c_void);
    }
    if Shell_NotifyIconW(NIM_MODIFY, &n).as_bool() {
        (api().set)(K_WELCOME, 1); // remembered in the registry, so it is never shown again
    }
    if ic != 0 {
        let _ = DestroyIcon(HICON(ic as *mut c_void));
    }
}

// Windows 11: put the icon in the visible tray (system icon) or the hidden-icons overflow (Bark icon). The settings
// entry appears a moment after the icon is added, so retry a few times, and once more after it is found.
unsafe fn pin_start(h: HWND) {
    PIN_TRIES.store(0, Relaxed);
    pin_tick(h);
}

unsafe fn pin_tick(h: HWND) {
    let _ = KillTimer(Some(h), T_PIN);
    let visible = ((api().get)(K_SYS_ICON) != 0) as i32;
    let found = (api().tray_pin)(visible) != 0;
    let n = PIN_TRIES.fetch_add(1, Relaxed);
    if (!found && n < 8) || (found && n == 0) {
        let _ = SetTimer(Some(h), T_PIN, if found { 1500 } else { 500 }, None);
    }
}

unsafe fn tray_mod(h: HWND) {
    let (icon, own) = icon_pair();
    let mut n = nid(h);
    n.hIcon = icon;
    let _ = Shell_NotifyIconW(NIM_MODIFY, &n);
    if own {
        let _ = DestroyIcon(icon);
    }
}

unsafe fn tray_del(h: HWND) {
    if TRAY_ON.swap(false, Relaxed) {
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid(h));
    }
}

unsafe fn theme_check(h: HWND) {
    if !TRAY_ON.load(Relaxed) {
        return;
    }
    let d = (api().get)(K_TASKBAR_DARK);
    if LAST_TB_DARK.swap(d, Relaxed) != d {
        tray_mod(h);
    }
}

// ---- flyout show / hide ----

unsafe fn show(h: HWND) {
    if !TRAY_ON.load(Relaxed) {
        tray_add(h);
    }
    (ui().on_show)(hv(h));
}

unsafe fn finish_hide(h: HWND) {
    let _ = ShowWindow(h, SW_HIDE);
    LAST_HIDE.store(GetTickCount64(), Relaxed);
    if (api().get)(K_HIDE_TRAY) != 0 {
        tray_del(h);
    }
    trim();
}

unsafe fn hide_fly(h: HWND) {
    if (ui().begin_hide)(hv(h)) == 0 {
        finish_hide(h);
    }
}

unsafe fn toggle(h: HWND) {
    if IsWindowVisible(h).as_bool() {
        hide_fly(h);
    } else if GetTickCount64().saturating_sub(LAST_HIDE.load(Relaxed)) > 300 {
        show(h);
    }
}

unsafe fn open_log() {
    if let Some(p) = LOGDIR.get() {
        let _ = ShellExecuteW(None, w!("open"), PCWSTR(p.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
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
        203 => show(h),
        _ => {}
    }
}

unsafe extern "system" fn wndproc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_TRAY => {
            match (lp.0 as u32) & 0xFFFF {
                WM_LBUTTONUP => toggle(h),
                NIN_BALLOONUSERCLICK => show(h),
                WM_RBUTTONUP => menu(h),
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSED => {
            finish_hide(h);
            LRESULT(0)
        }
        WM_BRAND => {
            if TRAY_ON.load(Relaxed) {
                tray_mod(h);
                pin_start(h);
            }
            LRESULT(0)
        }
        WM_SHOWME => {
            if IsWindowVisible(h).as_bool() {
                let _ = SetForegroundWindow(h);
            } else {
                show(h);
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
            (ui().paint)(hv(h), dc.0 as isize);
            let _ = EndPaint(h, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEMOVE => {
            let (x, y) = xy(lp);
            (ui().mouse_move)(hv(h), x, y);
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            (ui().mouse_leave)(hv(h));
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = xy(lp);
            (ui().mouse_down)(hv(h), x, y);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = xy(lp);
            (ui().mouse_up)(hv(h), x, y);
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            (ui().capture_lost)();
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
            let (sx, sy) = xy(lp);
            let mut pt = POINT { x: sx, y: sy };
            let _ = ScreenToClient(h, &mut pt);
            (ui().wheel)(hv(h), pt.x, pt.y, delta);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wp.0 == 0x1B {
                hide_fly(h);
            } else {
                (ui().key)(hv(h), wp.0 as u32);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if wp.0 == T_PIN {
                pin_tick(h);
            } else if wp.0 == T_BALLOON {
                let _ = KillTimer(Some(h), T_BALLOON);
                balloon(h);
            } else {
                (ui().timer)(hv(h), wp.0);
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            if (wp.0 & 0xFFFF) == 0 && IsWindowVisible(h).as_bool() {
                hide_fly(h);
            }
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            theme_check(h);
            DefWindowProcW(h, m, wp, lp)
        }
        WM_DESTROY => {
            tray_del(h);
            (api().shutdown)();
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => {
            let tb = TB_CREATED.load(Relaxed);
            if tb != 0 && m == tb && TRAY_ON.swap(false, Relaxed) {
                tray_add(h);
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
        tray_pin: sym!("sv_tray_pin"),
    })
}

unsafe fn load_ui(dll: &[u16]) -> Option<Ui> {
    let h = LoadLibraryExW(PCWSTR(dll.as_ptr()), None, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32).ok()?;
    macro_rules! sym {
        ($n:literal) => {
            transmute(GetProcAddress(h, s!($n))?)
        };
    }
    Some(Ui {
        init: sym!("bui_init"),
        on_show: sym!("bui_on_show"),
        begin_hide: sym!("bui_begin_hide"),
        paint: sym!("bui_paint"),
        timer: sym!("bui_timer"),
        mouse_move: sym!("bui_mouse_move"),
        mouse_leave: sym!("bui_mouse_leave"),
        mouse_down: sym!("bui_mouse_down"),
        mouse_up: sym!("bui_mouse_up"),
        capture_lost: sym!("bui_capture_lost"),
        wheel: sym!("bui_wheel"),
        key: sym!("bui_key"),
        set_tip: sym!("bui_set_tip"),
        tip_wndproc: sym!("bui_tip_wndproc"),
        icon_font: sym!("bui_icon_font"),
        tray_icon: sym!("bui_tray_icon"),
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
        if let Ok(other) = FindWindowW(w!("BarkWnd"), PCWSTR::null()) {
            let mut pid = 0u32;
            GetWindowThreadProcessId(other, Some(&mut pid));
            let _ = AllowSetForegroundWindow(pid);
            let _ = PostMessageW(Some(other), WM_SHOWME, WPARAM(0), LPARAM(0));
        }
        return;
    }

    let mut buf = [0u16; 1024];
    let n = GetModuleFileNameW(None, &mut buf) as usize;
    let mut exe: Vec<u16> = buf[..n].to_vec();
    let cut = exe.iter().rposition(|&c| c == b'\\' as u16).unwrap_or(0);
    let mut dll: Vec<u16> = exe[..cut].to_vec();
    let mut uid = dll.clone();
    let mut logd = dll.clone();
    logd.extend("\\log".encode_utf16());
    logd.push(0);
    let _ = LOGDIR.set(logd);
    dll.extend(CORE_DLL.encode_utf16());
    dll.push(0);
    uid.extend(UI_DLL.encode_utf16());
    uid.push(0);
    exe.push(0);

    let Some(a) = load_api(&dll) else {
        MessageBoxW(None, w!("The core DLL (sysvol*.dll) is missing or invalid. It must sit next to bark.exe and match its architecture."), w!("Bark"), MB_ICONERROR | MB_OK);
        return;
    };
    let _ = API.set(a);
    if (api().init)(exe.as_ptr()) != 0 {
        MessageBoxW(None, w!("The core engine failed to start."), w!("Bark"), MB_ICONERROR | MB_OK);
        return;
    }
    let Some(u) = load_ui(&uid) else {
        MessageBoxW(None, w!("The UI DLL (bui*.dll) is missing or invalid. It must sit next to bark.exe and match its architecture."), w!("Bark"), MB_ICONERROR | MB_OK);
        return;
    };
    let _ = UI.set(u);
    (ui().init)(api().get, set_wrap);

    let Ok(hm) = GetModuleHandleW(None) else { return };
    let hi = HINSTANCE(hm.0);
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
        lpfnWndProc: transmute((ui().tip_wndproc)()),
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
    (ui().set_tip)(hv(tw));
    MAIN_HWND.store(hv(hwnd), Relaxed);

    let round: u32 = 2; // DWMWCP_ROUND
    let _ = DwmSetWindowAttribute(hwnd, DWMWINDOWATTRIBUTE(33), &round as *const u32 as *const c_void, 4);

    if (api().get)(K_HIDE_TRAY) == 0 {
        tray_add(hwnd);
        // First run only (the flag is stored under Bark's registry key). Delayed a little so Explorer has the icon ready.
        if (api().get)(K_WELCOME) == 0 {
            let _ = SetTimer(Some(hwnd), T_BALLOON, 1500, None);
        }
    }
    let mods = MOD_WIN | MOD_ALT;
    let _ = RegisterHotKey(Some(hwnd), 1, mods, 0x21); // Win+Alt+PgUp
    let _ = RegisterHotKey(Some(hwnd), 2, mods, 0x22); // Win+Alt+PgDn
    let _ = RegisterHotKey(Some(hwnd), 3, mods, 0x51); // Win+Alt+Q
    trim();

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}
