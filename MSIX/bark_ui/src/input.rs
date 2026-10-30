use crate::layout::*;
use crate::keys::*;
use crate::state::*;
use crate::tip::tip_hide;
use crate::{get, nav, set, T_SCROLL, T_TIP};
use std::{mem::size_of, sync::atomic::Ordering::Relaxed};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::*,
        Graphics::Gdi::InvalidateRect,
        UI::{Input::KeyboardAndMouse::*, Shell::ShellExecuteW, WindowsAndMessaging::*},
    },
};

unsafe fn inv(h: HWND) {
    let _ = InvalidateRect(Some(h), None, false);
}

unsafe fn open(url: PCWSTR) {
    let _ = ShellExecuteW(None, w!("open"), url, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
}

fn set_vol(v: i32) {
    set(K_VOLUME, v.clamp(0, 100));
}

fn slider_val(x: i32) -> i32 {
    let (x0, x1) = slider_x();
    (((x - x0) * 100 + (x1 - x0) / 2) / (x1 - x0)).clamp(0, 100)
}

fn edit_key() -> u32 {
    if EDIT.load(Relaxed) == 0 {
        K_SCHED_START
    } else {
        K_SCHED_END
    }
}

unsafe fn pick_apply(h: HWND, col: i32, off: i32) {
    let key = edit_key();
    let m = get(key);
    let (mut hh, mut mn) = (m / 60, m % 60);
    match col {
        0 => hh = (hh + off).rem_euclid(24),
        1 => mn = (mn + off).rem_euclid(60),
        _ => {
            if off != 0 {
                hh = (hh + 12) % 24;
            }
        }
    }
    set(key, hh * 60 + mn);
    if col < 2 {
        let a = &SCROLL[col as usize];
        a.store((a.load(Relaxed) + off * 1000).clamp(-3000, 3000), Relaxed);
        let _ = SetTimer(Some(h), T_SCROLL, 15, None);
    }
}

pub unsafe fn scroll_tick(h: HWND) {
    let mut any = false;
    for a in SCROLL.iter() {
        let v = a.load(Relaxed) * 4 / 5;
        let v = if v.abs() < 40 { 0 } else { v };
        any |= v != 0;
        a.store(v, Relaxed);
    }
    if !any {
        let _ = KillTimer(Some(h), T_SCROLL);
    }
    inv(h);
}

fn pick_col(g: &PickGeom, x: i32) -> Option<i32> {
    let rel = x - g.x0;
    if rel < 0 {
        return None;
    }
    let col = rel / (g.cw + g.gap);
    if col < g.n && rel % (g.cw + g.gap) < g.cw {
        Some(col)
    } else {
        None
    }
}

fn pick_geom_now() -> PickGeom {
    let r = items(P_SCHED).into_iter().find(|i| i.id == PICKER).map_or([0; 4], |i| i.r);
    picker_geom(&r, get(K_CLOCK24) != 0)
}

unsafe fn pick_click(h: HWND, x: i32, y: i32) {
    let g = pick_geom_now();
    let row = (y - g.top) / g.rh;
    if let (Some(col), true) = (pick_col(&g, x), (0..5).contains(&row)) {
        pick_apply(h, col, row - 2);
    }
}

unsafe fn click(h: HWND, it: Item, x: i32, y: i32) {
    match it.id {
        HELP => nav::go(h, P_ABOUT),
        BACK => nav::go(h, P_MAIN),
        SCHED_GO => nav::go(h, P_SCHED),
        SETT_GO => nav::go(h, P_SET),
        LEARN => open(w!("https://github.com/bonanto/bark")),
        CHIP_FROM => EDIT.store(0, Relaxed),
        CHIP_TO => EDIT.store(1, Relaxed),
        PICKER => pick_click(h, x, y),
        id => {
            if let Some(k) = toggle_key(id) {
                let on = if id == QUIET { get(K_QUIET_ACTIVE) != 0 } else { get(k) != 0 };
                set(k, if on { 0 } else { 1 });
            }
        }
    }
    inv(h);
}

pub unsafe fn mouse_move(h: HWND, x: i32, y: i32) {
    if PRESS.load(Relaxed) == SLIDER {
        set_vol(slider_val(x));
        inv(h);
    }
    let id = hit(x, y).map_or(0, |i| i.id);
    if id != HOVER.load(Relaxed) {
        HOVER.store(id, Relaxed);
        tip_hide();
        if id != 0 {
            let _ = SetTimer(Some(h), T_TIP, 600, None);
        } else {
            let _ = KillTimer(Some(h), T_TIP);
        }
        inv(h);
    }
    let mut te = TRACKMOUSEEVENT {
        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: h,
        dwHoverTime: 0,
    };
    let _ = TrackMouseEvent(&mut te);
}

pub unsafe fn mouse_leave(h: HWND) {
    HOVER.store(0, Relaxed);
    let _ = KillTimer(Some(h), T_TIP);
    tip_hide();
    inv(h);
}

pub unsafe fn mouse_down(h: HWND, x: i32, y: i32) {
    tip_hide();
    let id = hit(x, y).map_or(0, |i| i.id);
    PRESS.store(id, Relaxed);
    if id == SLIDER {
        let _ = SetCapture(h);
        set_vol(slider_val(x));
    }
    inv(h);
}

pub unsafe fn mouse_up(h: HWND, x: i32, y: i32) {
    let was = PRESS.swap(0, Relaxed);
    if was == SLIDER {
        let _ = ReleaseCapture();
    } else if was != 0 {
        if let Some(it) = hit(x, y) {
            if it.id == was {
                click(h, it, x, y);
            }
        }
    }
    inv(h);
}

pub unsafe fn capture_lost() {
    if PRESS.load(Relaxed) == SLIDER {
        PRESS.store(0, Relaxed);
    }
}

pub unsafe fn wheel(h: HWND, x: i32, y: i32, delta: i32) {
    let Some(it) = hit(x, y) else { return };
    let prev = WACC.load(Relaxed);
    let acc = if prev != 0 && (prev > 0) != (delta > 0) { delta } else { prev + delta };
    let steps = acc / 120;
    WACC.store(acc - steps * 120, Relaxed);
    if steps == 0 {
        return;
    }
    if it.id == SLIDER {
        set_vol(get(K_VOLUME) + steps);
    } else if it.id == PICKER {
        let g = pick_geom_now();
        if let Some(col) = pick_col(&g, x) {
            pick_apply(h, col, -steps.signum());
        }
    }
    inv(h);
}

pub unsafe fn key(h: HWND, vk: u32) {
    if PAGE.load(Relaxed) != P_MAIN {
        return;
    }
    let d = match vk {
        0x25 | 0x28 => -1,
        0x27 | 0x26 => 1,
        _ => return,
    };
    set_vol(get(K_VOLUME) + d);
    inv(h);
}
