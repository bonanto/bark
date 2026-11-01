use crate::draw::{blit, free, render, snap, Surf};
use crate::font::{free_fonts, make_fonts};
use crate::input::scroll_tick;
use crate::keys::{K_ACC_D1, K_ACC_L1, K_ACC_L2, K_ACC_L3, K_SYS_DARK};
use crate::state::*;
use crate::tip::{tip_hide, tip_timer};
use crate::{get, T_CLOSE, T_NAV, T_OPEN, T_REFRESH, T_SCROLL, T_TIP, WM_CLOSED};
use std::{
    ffi::c_void,
    sync::{
        atomic::{AtomicI32, AtomicU64, Ordering::Relaxed},
        Mutex,
    },
};
use windows::Win32::{
    Foundation::*,
    Graphics::{Dwm::*, Gdi::*},
    System::SystemInformation::GetTickCount64,
    UI::{HiDpi::GetDpiForSystem, WindowsAndMessaging::*},
};

struct Anim {
    from: Surf,
    to: Surf,
    t0: u64,
    dir: i32,
}

static ANIM: Mutex<Option<Anim>> = Mutex::new(None);
static OPEN_T0: AtomicU64 = AtomicU64::new(0);
static OPEN_X: AtomicI32 = AtomicI32::new(0);
static OPEN_Y: AtomicI32 = AtomicI32::new(0);
static WA_BOTTOM: AtomicI32 = AtomicI32::new(0);
static CLOSING: AtomicI32 = AtomicI32::new(0);
static CLOSE_T0: AtomicU64 = AtomicU64::new(0);
static CLOSE_Y0: AtomicI32 = AtomicI32::new(0);
// Pre-rendered flyout used while it slides in or out, so every frame is a plain blit.
static SLIDE: Mutex<Option<Surf>> = Mutex::new(None);

const NAV_MS: f32 = 180.0;
const OPEN_MS: f32 = 320.0;
const CLOSE_MS: f32 = 200.0;

unsafe fn put_slide(s: Option<Surf>) {
    if let Ok(mut g) = SLIDE.lock() {
        if let Some(old) = g.take() {
            free(old);
        }
        *g = s;
    }
}

unsafe fn snapshot(h: HWND, page: i32) {
    let hdc = GetDC(Some(h));
    let s = render(hdc, page);
    ReleaseDC(Some(h), hdc);
    put_slide(s);
}

unsafe fn ease(t0: u64, ms: f32) -> (f32, f32) {
    let p = (GetTickCount64().saturating_sub(t0) as f32 / ms).min(1.0);
    (p, 1.0 - (1.0 - p).powi(3))
}

unsafe fn inv(h: HWND) {
    let _ = InvalidateRect(Some(h), None, false);
}

// While sliding, only the part above the work-area edge is visible, like a flyout leaving the taskbar.
// The region change must repaint (redraw = true + UpdateWindow). With redraw = false the newly exposed rows were never
// painted, which left the bottom of the flyout transparent.
unsafe fn clip(h: HWND, y: i32) {
    let vis = (WA_BOTTOM.load(Relaxed) - y).clamp(0, sc(H0));
    let rgn = CreateRectRgn(0, 0, sc(W0), vis);
    let _ = SetWindowRgn(h, Some(rgn), true);
    let _ = UpdateWindow(h);
}

unsafe fn unclip(h: HWND) {
    let _ = SetWindowRgn(h, None, true);
    let _ = UpdateWindow(h);
}

unsafe fn mv(h: HWND, y: i32) {
    let _ = SetWindowPos(h, None, OPEN_X.load(Relaxed), y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
}

// Moves the flyout and its clip together. Order matters so nothing ever shows below the work area:
// rising = move first, then grow the region; falling = shrink the region first, then move.
unsafe fn place(h: HWND, y: i32, rising: bool) {
    if rising {
        mv(h, y);
        clip(h, y);
    } else {
        clip(h, y);
        mv(h, y);
    }
}

unsafe fn end_anim() {
    let a = ANIM.lock().ok().and_then(|mut g| g.take());
    if let Some(a) = a {
        free(a.from);
        free(a.to);
    }
    ANIMATING.store(0, Relaxed);
}

pub unsafe fn go(h: HWND, new: i32) {
    let old = PAGE.load(Relaxed);
    if old == new {
        return;
    }
    end_anim();
    let _ = KillTimer(Some(h), T_NAV);
    tip_hide();
    HOVER.store(0, Relaxed);
    PRESS.store(0, Relaxed);
    let hdc = GetDC(Some(h));
    let from = render(hdc, old);
    PAGE.store(new, Relaxed);
    let to = render(hdc, new);
    ReleaseDC(Some(h), hdc);
    match (from, to) {
        (Some(from), Some(to)) => {
            let dir = if new == P_MAIN { -1 } else { 1 };
            if let Ok(mut g) = ANIM.lock() {
                *g = Some(Anim { from, to, t0: GetTickCount64(), dir });
                ANIMATING.store(1, Relaxed);
                let _ = SetTimer(Some(h), T_NAV, 15, None);
            }
        }
        (a, b) => {
            if let Some(s) = a {
                free(s);
            }
            if let Some(s) = b {
                free(s);
            }
        }
    }
    inv(h);
}

pub unsafe fn on_show(h: HWND) {
    let _ = KillTimer(Some(h), T_CLOSE);
    let _ = KillTimer(Some(h), T_OPEN);
    CLOSING.store(0, Relaxed);
    end_anim();
    put_slide(None);
    let dpi = GetDpiForSystem();
    DPI.store(dpi as i32, Relaxed);
    let dark = get(K_SYS_DARK);
    DARK.store(dark, Relaxed);
    for (i, k) in [K_ACC_L3, K_ACC_L2, K_ACC_L1, K_ACC_D1].into_iter().enumerate() {
        ACC[i].store(get(k), Relaxed);
    }
    PAGE.store(P_MAIN, Relaxed);
    HOVER.store(0, Relaxed);
    PRESS.store(0, Relaxed);
    free_fonts();
    make_fonts(dpi);
    let _ = DwmSetWindowAttribute(h, DWMWINDOWATTRIBUTE(20), &dark as *const i32 as *const c_void, 4);
    let (w, ht, mg) = (sc(W0), sc(H0), sc(12));
    let wa = work();
    let (x, y) = (wa.right - w - mg, wa.bottom - ht - mg);
    WA_BOTTOM.store(wa.bottom, Relaxed);
    OPEN_X.store(x, Relaxed);
    OPEN_Y.store(y, Relaxed);
    OPEN_T0.store(GetTickCount64(), Relaxed);
    snapshot(h, P_MAIN);
    ANIMATING.store(1, Relaxed);
    // Start fully hidden below the work-area edge (empty clip), then rise into place.
    clip(h, wa.bottom);
    let _ = SetWindowPos(h, Some(HWND_TOPMOST), x, wa.bottom, w, ht, SWP_SHOWWINDOW);
    let _ = SetForegroundWindow(h);
    let _ = SetTimer(Some(h), T_OPEN, 15, None);
    let _ = SetTimer(Some(h), T_REFRESH, 1000, None);
    SNAP.store(snap(), Relaxed);
}

/// Starts the slide-down. Returns false if there is nothing to animate (window not visible).
pub unsafe fn begin_hide(h: HWND) -> bool {
    if !IsWindowVisible(h).as_bool() {
        return false;
    }
    if CLOSING.load(Relaxed) != 0 {
        return true;
    }
    for t in [T_TIP, T_REFRESH, T_NAV, T_OPEN, T_SCROLL] {
        let _ = KillTimer(Some(h), t);
    }
    end_anim();
    tip_hide();
    HOVER.store(0, Relaxed);
    PRESS.store(0, Relaxed);
    snapshot(h, PAGE.load(Relaxed));
    ANIMATING.store(1, Relaxed);
    let mut rc = RECT::default();
    let _ = GetWindowRect(h, &mut rc);
    CLOSE_Y0.store(rc.top, Relaxed);
    CLOSE_T0.store(GetTickCount64(), Relaxed);
    CLOSING.store(1, Relaxed);
    let _ = SetTimer(Some(h), T_CLOSE, 15, None);
    true
}

pub unsafe fn paint(_h: HWND, hdc: HDC) {
    if let Ok(g) = SLIDE.lock() {
        if let Some(s) = g.as_ref() {
            blit(hdc, s, 0);
            return;
        }
    }
    if let Ok(g) = ANIM.lock() {
        if let Some(a) = g.as_ref() {
            let w = sc(W0) as f32;
            let (_, e) = ease(a.t0, NAV_MS);
            blit(hdc, &a.from, (-(a.dir as f32) * e * w) as i32);
            blit(hdc, &a.to, (a.dir as f32 * (1.0 - e) * w) as i32);
            return;
        }
    }
    if let Some(s) = render(hdc, PAGE.load(Relaxed)) {
        blit(hdc, &s, 0);
        free(s);
    }
    SNAP.store(snap(), Relaxed);
}

unsafe fn nav_tick(h: HWND) {
    let t0 = ANIM.lock().ok().and_then(|g| g.as_ref().map(|a| a.t0));
    let Some(t0) = t0 else {
        let _ = KillTimer(Some(h), T_NAV);
        return;
    };
    if ease(t0, NAV_MS).0 >= 1.0 {
        let _ = KillTimer(Some(h), T_NAV);
        end_anim();
    }
    inv(h);
}

unsafe fn open_tick(h: HWND) {
    let (p, _) = ease(OPEN_T0.load(Relaxed), OPEN_MS);
    let e = 1.0 - (1.0 - p).powi(4); // strong ease-out, like the Windows flyouts
    let (y1, ybot) = (OPEN_Y.load(Relaxed), WA_BOTTOM.load(Relaxed));
    let y = y1 + ((1.0 - e) * (ybot - y1) as f32).round() as i32;
    place(h, y, true);
    if p >= 1.0 {
        let _ = KillTimer(Some(h), T_OPEN);
        put_slide(None);
        ANIMATING.store(0, Relaxed);
        unclip(h);
        inv(h);
    }
}

unsafe fn close_tick(h: HWND) {
    let (p, _) = ease(CLOSE_T0.load(Relaxed), CLOSE_MS);
    let (y0, y1) = (CLOSE_Y0.load(Relaxed), WA_BOTTOM.load(Relaxed));
    let y = y0 + ((y1 - y0) as f32 * p * p).round() as i32; // ease-in: leaves the screen accelerating
    place(h, y, false);
    if p >= 1.0 {
        let _ = KillTimer(Some(h), T_CLOSE);
        CLOSING.store(0, Relaxed);
        ANIMATING.store(0, Relaxed);
        put_slide(None);
        for a in SCROLL.iter() {
            a.store(0, Relaxed);
        }
        WACC.store(0, Relaxed);
        free_fonts();
        // The clip is empty here, so the window stays invisible until the host hides it.
        let _ = PostMessageW(Some(h), WM_CLOSED, WPARAM(0), LPARAM(0));
    }
}

unsafe fn refresh(h: HWND) {
    if ANIMATING.load(Relaxed) != 0 {
        return;
    }
    let s = snap();
    if s != SNAP.swap(s, Relaxed) {
        inv(h);
    }
}

pub unsafe fn timer(h: HWND, id: usize) {
    match id {
        T_TIP => tip_timer(h),
        T_REFRESH => refresh(h),
        T_NAV => nav_tick(h),
        T_OPEN => open_tick(h),
        T_SCROLL => scroll_tick(h),
        T_CLOSE => close_tick(h),
        _ => {}
    }
}
