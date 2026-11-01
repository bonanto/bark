use crate::layout::items;
use crate::state::*;
use crate::T_TIP;
use std::{ffi::c_void, sync::atomic::Ordering::Relaxed};
use windows::Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::*};

pub fn set_tip(h: HWND) {
    TIPW.store(h.0 as isize, Relaxed);
}

pub unsafe fn tip_hide() {
    let t = HWND(TIPW.load(Relaxed) as *mut c_void);
    if !t.is_invalid() {
        let _ = ShowWindow(t, SW_HIDE);
    }
}

pub unsafe fn tip_timer(h: HWND) {
    let _ = KillTimer(Some(h), T_TIP);
    let id = HOVER.load(Relaxed);
    if id == 0 || PRESS.load(Relaxed) != 0 || ANIMATING.load(Relaxed) != 0 {
        return;
    }
    let Some(it) = items(PAGE.load(Relaxed)).into_iter().find(|i| i.id == id) else {
        return;
    };
    let tw = HWND(TIPW.load(Relaxed) as *mut c_void);
    if it.tip.is_empty() || tw.is_invalid() || FS.load(Relaxed) == 0 {
        return;
    }
    if let Ok(mut s) = TIP_TEXT.lock() {
        *s = it.tip.to_string();
    }
    let dc = GetDC(Some(tw));
    let old = SelectObject(dc, HGDIOBJ(FS.load(Relaxed) as *mut c_void));
    let mut rc = RECT { left: 0, top: 0, right: sc(240), bottom: 0 };
    let mut t: Vec<u16> = it.tip.encode_utf16().collect();
    DrawTextW(dc, &mut t, &mut rc, DT_CALCRECT | DT_WORDBREAK | DT_LEFT | DT_NOPREFIX);
    SelectObject(dc, old);
    ReleaseDC(Some(tw), dc);
    let pad = sc(6);
    let (w, ht) = (rc.right + 2 * pad, rc.bottom + 2 * pad);
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let wa = work();
    let x = (pt.x - w / 2).clamp(wa.left, (wa.right - w).max(wa.left));
    let mut y = pt.y + sc(22);
    if y + ht > wa.bottom {
        y = pt.y - ht - sc(8);
    }
    let _ = SetWindowPos(tw, Some(HWND_TOPMOST), x, y, w, ht, SWP_NOACTIVATE | SWP_SHOWWINDOW);
    let _ = InvalidateRect(Some(tw), None, true);
}

pub unsafe extern "system" fn tip_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if m == WM_PAINT {
        let mut ps = PAINTSTRUCT::default();
        let dc = BeginPaint(h, &mut ps);
        let mut rc = RECT::default();
        let _ = GetClientRect(h, &mut rc);
        let p = pal();
        let bg = CreateSolidBrush(bgr(p.tip));
        let bd = CreateSolidBrush(bgr(p.border));
        FillRect(dc, &rc, bg);
        FrameRect(dc, &rc, bd);
        let _ = DeleteObject(bg.into());
        let _ = DeleteObject(bd.into());
        let pad = sc(6);
        let mut r2 = RECT { left: pad, top: pad, right: rc.right - pad, bottom: rc.bottom - pad };
        SelectObject(dc, HGDIOBJ(FS.load(Relaxed) as *mut c_void));
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, bgr(p.fg));
        let mut t: Vec<u16> = TIP_TEXT.lock().map(|s| s.encode_utf16().collect()).unwrap_or_default();
        DrawTextW(dc, &mut t, &mut r2, DT_WORDBREAK | DT_LEFT | DT_NOPREFIX);
        let _ = EndPaint(h, &ps);
        return LRESULT(0);
    }
    DefWindowProcW(h, m, wp, lp)
}
