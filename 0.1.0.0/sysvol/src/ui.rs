use crate::gfx::{mix, rgb, Canvas};
use crate::{
    api, APP_VERSION, K_AUTOSTART, K_DING, K_LOCK, K_QUIET_MANUAL, K_RESETS, K_SCHED_END, K_SCHED_ON,
    K_SCHED_START, K_SYS_DARK, K_UPDATE_AVAIL, K_VOLUME,
};
use std::{
    ffi::c_void,
    mem::size_of,
    sync::{
        atomic::{AtomicI32, AtomicIsize, Ordering::Relaxed},
        Mutex,
    },
};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, Shell::ShellExecuteW, WindowsAndMessaging::*},
    },
};

pub const W0: i32 = 320;
pub const H0: i32 = 364;

const SLIDER: i32 = 1;
const LOCK: i32 = 2;
const AUTO: i32 = 3;
const QUIET: i32 = 4;
const SCHED: i32 = 5;
const DING: i32 = 6;
const HELP: i32 = 20;
const UPD: i32 = 21;
const LEARN: i32 = 22;
const SEG0: i32 = 40; // 4 time segments x 3 parts (up, value, down)

pub const T_TIP: usize = 1;
pub const T_REFRESH: usize = 2;

static DPI: AtomicI32 = AtomicI32::new(96);
static PAGE: AtomicI32 = AtomicI32::new(0);
static HOVER: AtomicI32 = AtomicI32::new(0);
static PRESS: AtomicI32 = AtomicI32::new(0);
static DARK: AtomicI32 = AtomicI32::new(0);
static FN: AtomicIsize = AtomicIsize::new(0);
static FB: AtomicIsize = AtomicIsize::new(0);
static FS: AtomicIsize = AtomicIsize::new(0);
static TIPW: AtomicIsize = AtomicIsize::new(0);
static TIP_TEXT: Mutex<String> = Mutex::new(String::new());

const HELP_TEXT: &str = "Windows keeps resetting the “System sounds” volume, which makes notification dings far too loud. Bark holds it at the value you choose and restores it after sleep, sign-in or device changes.\n\nChange it here or in Windows Settings (Volume mixer): your changes are accepted, anything else is reverted.\n\nHotkeys: Ctrl+Alt+PgUp / PgDn adjust the volume, Ctrl+Alt+Q toggles Quiet mode.";

fn sc(v: i32) -> i32 {
    v * DPI.load(Relaxed) / 96
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Slider,
    Toggle,
    Up,
    Down,
    Seg,
    Round,
    Pill,
}

#[derive(Clone, Copy)]
struct Item {
    id: i32,
    kind: Kind,
    r: [i32; 4],
    label: &'static str,
    tip: &'static str,
}

struct Pal {
    bg: u32,
    fg: u32,
    dim: u32,
    acc: u32,
    acc_fg: u32,
    track: u32,
    border: u32,
    hover: u32,
    card: u32,
    tip: u32,
}

fn pal() -> Pal {
    if DARK.load(Relaxed) != 0 {
        Pal {
            bg: rgb(32, 32, 32),
            fg: rgb(255, 255, 255),
            dim: rgb(165, 165, 165),
            acc: rgb(96, 205, 255),
            acc_fg: rgb(0, 0, 0),
            track: rgb(100, 100, 100),
            border: rgb(64, 64, 64),
            hover: rgb(50, 50, 50),
            card: rgb(69, 69, 69),
            tip: rgb(44, 44, 44),
        }
    } else {
        Pal {
            bg: rgb(243, 243, 243),
            fg: rgb(26, 26, 26),
            dim: rgb(96, 96, 96),
            acc: rgb(0, 103, 192),
            acc_fg: rgb(255, 255, 255),
            track: rgb(168, 168, 168),
            border: rgb(204, 204, 204),
            hover: rgb(229, 229, 229),
            card: rgb(255, 255, 255),
            tip: rgb(255, 255, 255),
        }
    }
}

fn bgr(c: u32) -> COLORREF {
    COLORREF(((c & 255) << 16) | (c & 0xFF00) | ((c >> 16) & 255))
}

fn toggle_key(id: i32) -> u32 {
    match id {
        LOCK => K_LOCK,
        AUTO => K_AUTOSTART,
        QUIET => K_QUIET_MANUAL,
        SCHED => K_SCHED_ON,
        _ => K_DING,
    }
}

fn items(page: i32) -> Vec<Item> {
    let w = sc(W0);
    let m = sc(20);
    let rb = sc(26);
    let fy = sc(326);
    let mut v = Vec::with_capacity(24);
    v.push(Item {
        id: HELP,
        kind: Kind::Round,
        r: [w - m - rb, fy, w - m, fy + rb],
        label: if page == 0 { "?" } else { "\u{00D7}" },
        tip: if page == 0 { "Help and About" } else { "Back" },
    });
    if page == 1 {
        v.push(Item {
            id: LEARN,
            kind: Kind::Pill,
            r: [m, fy, m + sc(110), fy + rb],
            label: "Learn More",
            tip: "Open the Bark project page on GitHub.",
        });
        return v;
    }
    if (api().get)(K_UPDATE_AVAIL) != 0 {
        v.push(Item {
            id: UPD,
            kind: Kind::Pill,
            r: [w - m - rb - sc(8) - sc(76), fy, w - m - rb - sc(8), fy + rb],
            label: "Update",
            tip: "A newer version is available. Opens the download page.",
        });
    }
    v.push(Item {
        id: SLIDER,
        kind: Kind::Slider,
        r: [m, sc(50), w - m, sc(78)],
        label: "",
        tip: "System sounds volume. Drag, click, use the mouse wheel or arrow keys.",
    });
    let rows = [
        (LOCK, "Lock volume", "Keep the System sounds volume at your value. Outside changes are reverted."),
        (AUTO, "Start with Windows", "Launch Bark automatically when you sign in."),
        (QUIET, "Quiet mode (No Sounds)", "Switch the Windows sound scheme to No Sounds until you turn this off."),
        (SCHED, "Quiet schedule", "Turn Quiet mode on automatically between the times below."),
        (DING, "Silence volume-change ding", "Briefly mute System sounds while the main volume changes, so the slider ding stays silent."),
    ];
    for (i, (id, label, tip)) in rows.iter().enumerate() {
        let y = sc(92) + i as i32 * sc(32);
        v.push(Item { id: *id, kind: Kind::Toggle, r: [m, y, w - m, y + sc(32)], label, tip });
    }
    let g1 = m + sc(40);
    let g2 = g1 + sc(82) + sc(36);
    let xs = [g1, g1 + sc(46), g2, g2 + sc(46)];
    let tips = [
        "Quiet start: hour. Use the arrows, click, or the mouse wheel.",
        "Quiet start: minute. Use the arrows, click, or the mouse wheel.",
        "Quiet end: hour. Use the arrows, click, or the mouse wheel.",
        "Quiet end: minute. Use the arrows, click, or the mouse wheel.",
    ];
    for (i, &x) in xs.iter().enumerate() {
        let id = SEG0 + i as i32 * 3;
        let (l, r) = (x, x + sc(36));
        v.push(Item { id, kind: Kind::Up, r: [l, sc(258), r, sc(272)], label: "", tip: tips[i] });
        v.push(Item { id: id + 1, kind: Kind::Seg, r: [l, sc(272), r, sc(296)], label: "", tip: tips[i] });
        v.push(Item { id: id + 2, kind: Kind::Down, r: [l, sc(296), r, sc(310)], label: "", tip: tips[i] });
    }
    v
}

fn hit(x: i32, y: i32) -> Option<Item> {
    items(PAGE.load(Relaxed))
        .into_iter()
        .rev()
        .find(|i| x >= i.r[0] && x < i.r[2] && y >= i.r[1] && y < i.r[3])
}

fn seg_minutes(seg: i32) -> i32 {
    (api().get)(if seg < 2 { K_SCHED_START } else { K_SCHED_END })
}

fn seg_adjust(seg: i32, dir: i32, wheel: bool) {
    let key = if seg < 2 { K_SCHED_START } else { K_SCHED_END };
    let m = (api().get)(key);
    let (mut hh, mut mm) = (m / 60, m % 60);
    if seg % 2 == 0 {
        hh = (hh + dir).rem_euclid(24);
    } else {
        mm = (mm + dir * if wheel { 1 } else { 5 }).rem_euclid(60);
    }
    (api().set)(key, hh * 60 + mm);
}

fn set_vol(v: i32) {
    (api().set)(K_VOLUME, v.clamp(0, 100));
}

fn slider_val(x: i32) -> i32 {
    let x0 = sc(20) + sc(10);
    let x1 = sc(W0) - sc(20) - sc(10);
    (((x - x0) * 100 + (x1 - x0) / 2) / (x1 - x0)).clamp(0, 100)
}

unsafe fn inv(h: HWND) {
    let _ = InvalidateRect(Some(h), None, false);
}

unsafe fn open(url: PCWSTR) {
    let _ = ShellExecuteW(None, w!("open"), url, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
}

unsafe fn work() -> RECT {
    let mut wa = RECT::default();
    let _ = SystemParametersInfoW(
        SPI_GETWORKAREA,
        0,
        Some(&mut wa as *mut RECT as *mut c_void),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    );
    wa
}

// ---- fonts ----

unsafe fn make_fonts(dpi: u32) {
    let mut ncm = NONCLIENTMETRICSW::default();
    ncm.cbSize = size_of::<NONCLIENTMETRICSW>() as u32;
    let ok = SystemParametersInfoForDpi(
        SPI_GETNONCLIENTMETRICS.0,
        ncm.cbSize,
        Some(&mut ncm as *mut NONCLIENTMETRICSW as *mut c_void),
        0,
        dpi,
    );
    if ok.is_err() {
        return;
    }
    let mk = |scale: f32, weight: i32| -> isize {
        let mut lf = ncm.lfMessageFont;
        lf.lfHeight = (lf.lfHeight as f32 * scale) as i32;
        lf.lfWeight = weight;
        lf.lfQuality = CLEARTYPE_QUALITY;
        CreateFontIndirectW(&lf).0 as isize
    };
    FN.store(mk(1.0, 400), Relaxed);
    FB.store(mk(1.15, 600), Relaxed);
    FS.store(mk(0.9, 400), Relaxed);
}

unsafe fn free_fonts() {
    for f in [&FN, &FB, &FS] {
        let v = f.swap(0, Relaxed);
        if v != 0 {
            let _ = DeleteObject(HGDIOBJ(v as *mut c_void));
        }
    }
}

// ---- show / hide ----

pub unsafe fn on_show(h: HWND) {
    let dpi = GetDpiForSystem();
    DPI.store(dpi as i32, Relaxed);
    let dark = (api().get)(K_SYS_DARK);
    DARK.store(dark, Relaxed);
    PAGE.store(0, Relaxed);
    HOVER.store(0, Relaxed);
    PRESS.store(0, Relaxed);
    make_fonts(dpi);
    let _ = DwmSetWindowAttribute(h, DWMWINDOWATTRIBUTE(20), &dark as *const i32 as *const c_void, 4);
    let (w, ht, mg) = (sc(W0), sc(H0), sc(12));
    let wa = work();
    let _ = SetWindowPos(h, Some(HWND_TOPMOST), wa.right - w - mg, wa.bottom - ht - mg, w, ht, SWP_SHOWWINDOW);
    let _ = SetForegroundWindow(h);
    let _ = SetTimer(Some(h), T_REFRESH, 1000, None);
}

pub unsafe fn on_hide(h: HWND) {
    let _ = KillTimer(Some(h), T_TIP);
    let _ = KillTimer(Some(h), T_REFRESH);
    tip_hide();
    free_fonts();
}

// ---- painting ----

unsafe fn tx(dc: HDC, t: &str, rc: [i32; 4], font: isize, col: u32, fl: DRAW_TEXT_FORMAT) {
    SelectObject(dc, HGDIOBJ(font as *mut c_void));
    SetTextColor(dc, bgr(col));
    let mut v: Vec<u16> = t.encode_utf16().collect();
    let mut r = RECT { left: rc[0], top: rc[1], right: rc[2], bottom: rc[3] };
    DrawTextW(dc, &mut v, &mut r, fl);
}

unsafe fn shape(cv: &Canvas, p: &Pal, it: &Item, s: f32, hover: i32, press: i32) {
    let [l, t, r, b] = it.r.map(|v| v as f32);
    let (cx, cy) = ((l + r) / 2.0, (t + b) / 2.0);
    let hot = hover == it.id;
    let down = press == it.id;
    match it.kind {
        Kind::Slider => {
            let (x0, x1) = (l + 10.0 * s, r - 10.0 * s);
            let v = (api().get)(K_VOLUME).clamp(0, 100) as f32;
            let tx = x0 + (x1 - x0) * v / 100.0;
            cv.rrect(x0, cy - 2.0 * s, x1, cy + 2.0 * s, 2.0 * s, p.track);
            if v > 0.0 {
                cv.rrect(x0, cy - 2.0 * s, tx, cy + 2.0 * s, 2.0 * s, p.acc);
            }
            cv.circle(tx, cy, 10.0 * s, p.border);
            cv.circle(tx, cy, 10.0 * s - 1.0, p.card);
            let ri = (if down { 4.0 } else if hot { 7.0 } else { 6.0 }) * s;
            cv.circle(tx, cy, ri, p.acc);
        }
        Kind::Toggle => {
            if hot {
                cv.rrect(l - 6.0 * s, t + 2.0 * s, r + 6.0 * s, b - 2.0 * s, 6.0 * s, p.hover);
            }
            let on = (api().get)(toggle_key(it.id)) != 0;
            let (sh, x1) = (20.0 * s, r);
            let x0 = x1 - 40.0 * s;
            let (y0, y1) = (cy - sh / 2.0, cy + sh / 2.0);
            if on {
                cv.rrect(x0, y0, x1, y1, sh / 2.0, p.acc);
                cv.circle(x1 - sh / 2.0, cy, (if hot { 7.0 } else { 6.0 }) * s, p.acc_fg);
            } else {
                let inner = if hot { p.hover } else { p.bg };
                cv.rrect(x0, y0, x1, y1, sh / 2.0, p.dim);
                cv.rrect(x0 + 1.5 * s, y0 + 1.5 * s, x1 - 1.5 * s, y1 - 1.5 * s, sh / 2.0 - 1.5 * s, inner);
                cv.circle(x0 + sh / 2.0, cy, (if hot { 6.0 } else { 5.0 }) * s, p.dim);
            }
        }
        Kind::Up | Kind::Down => {
            let c = if hot { p.fg } else { p.dim };
            let d = if it.kind == Kind::Up { -1.0 } else { 1.0 };
            let k = 3.5 * s;
            cv.line(cx - k, cy - d * k * 0.5, cx, cy + d * k * 0.5, 1.6 * s, c);
            cv.line(cx, cy + d * k * 0.5, cx + k, cy - d * k * 0.5, 1.6 * s, c);
        }
        Kind::Seg => {
            if hot || down {
                cv.rrect(l, t, r, b, 4.0 * s, p.hover);
            }
        }
        Kind::Round => {
            let rad = (r - l) / 2.0;
            cv.circle(cx, cy, rad, p.border);
            cv.circle(cx, cy, rad - 1.0, if hot { p.hover } else { p.bg });
        }
        Kind::Pill => {
            let c = if down {
                mix(p.acc, 0, 0.2)
            } else if hot {
                mix(p.acc, 0xFF_FFFF, 0.15)
            } else {
                p.acc
            };
            cv.rrect(l, t, r, b, (b - t) / 2.0, c);
        }
    }
}

pub unsafe fn paint(_h: HWND, hdc: HDC) {
    let (w, ht) = (sc(W0), sc(H0));
    let page = PAGE.load(Relaxed);
    let p = pal();
    let a = api();

    let mut bi = BITMAPINFO::default();
    bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = w;
    bi.bmiHeader.biHeight = -ht;
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    let mut bits: *mut c_void = std::ptr::null_mut();
    let Ok(bmp) = CreateDIBSection(Some(hdc), &bi, DIB_RGB_COLORS, &mut bits, None, 0) else {
        return;
    };
    if bits.is_null() {
        let _ = DeleteObject(bmp.into());
        return;
    }
    let mdc = CreateCompatibleDC(Some(hdc));
    let old = SelectObject(mdc, bmp.into());

    let cv = Canvas { p: bits as *mut u32, w, h: ht };
    cv.fill(p.bg);
    cv.border(p.border);
    let its = items(page);
    let s = DPI.load(Relaxed) as f32 / 96.0;
    let (hover, press) = (HOVER.load(Relaxed), PRESS.load(Relaxed));
    for it in &its {
        shape(&cv, &p, it, s, hover, press);
    }
    let _ = GdiFlush();

    SetBkMode(mdc, TRANSPARENT);
    let (fnt, fb, fs) = (FN.load(Relaxed), FB.load(Relaxed), FS.load(Relaxed));
    let m = sc(20);
    let vcs = DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX;
    let tc = if (a.get)(K_SCHED_ON) != 0 { p.fg } else { p.dim };

    if page == 0 {
        let v = (a.get)(K_VOLUME).clamp(0, 100);
        tx(mdc, "System sounds volume", [m, sc(14), w - m - sc(70), sc(44)], fnt, p.fg, vcs | DT_END_ELLIPSIS);
        tx(mdc, &format!("{v}%"), [w - m - sc(70), sc(14), w - m, sc(44)], fb, p.fg, vcs | DT_RIGHT);
        let g1 = m + sc(40);
        let g2 = g1 + sc(82) + sc(36);
        tx(mdc, "From", [m, sc(272), g1, sc(296)], fnt, p.dim, vcs);
        tx(mdc, ":", [g1 + sc(36), sc(272), g1 + sc(46), sc(296)], fnt, tc, vcs | DT_CENTER);
        tx(mdc, "to", [g1 + sc(82), sc(272), g2, sc(296)], fnt, p.dim, vcs | DT_CENTER);
        tx(mdc, ":", [g2 + sc(36), sc(272), g2 + sc(46), sc(296)], fnt, tc, vcs | DT_CENTER);
        let lim = its.iter().find(|i| i.id == UPD).map_or(its[0].r[0], |i| i.r[0]) - sc(8);
        tx(
            mdc,
            &format!("Blocked this session: {}", (a.get)(K_RESETS)),
            [m, sc(326), lim, sc(352)],
            fs,
            p.dim,
            vcs | DT_END_ELLIPSIS,
        );
    } else {
        tx(mdc, "Bark \u{2013} System Volume Fix", [m, sc(16), w - m, sc(42)], fb, p.fg, vcs);
        tx(mdc, HELP_TEXT, [m, sc(52), w - m, sc(244)], fnt, p.fg, DT_WORDBREAK | DT_NOPREFIX);
        tx(mdc, &format!("Version {APP_VERSION}"), [m, sc(252), w - m, sc(270)], fnt, p.fg, vcs);
        tx(mdc, "Author: Bonanto", [m, sc(270), w - m, sc(288)], fnt, p.dim, vcs);
        tx(mdc, "\u{00A9} 2026 Bonanto \u{00B7} Licensed under GPLv3", [m, sc(288), w - m, sc(308)], fs, p.dim, vcs);
    }

    for it in &its {
        match it.kind {
            Kind::Toggle => {
                tx(mdc, it.label, [it.r[0], it.r[1], it.r[2] - sc(52), it.r[3]], fnt, p.fg, vcs | DT_END_ELLIPSIS)
            }
            Kind::Round => tx(mdc, it.label, it.r, fb, p.fg, vcs | DT_CENTER),
            Kind::Pill => tx(mdc, it.label, it.r, fnt, p.acc_fg, vcs | DT_CENTER),
            Kind::Seg => {
                let seg = (it.id - SEG0) / 3;
                let mnt = seg_minutes(seg);
                let val = if seg % 2 == 0 { mnt / 60 } else { mnt % 60 };
                tx(mdc, &format!("{val:02}"), it.r, fnt, tc, vcs | DT_CENTER);
            }
            _ => {}
        }
    }

    let _ = GdiFlush();
    let _ = BitBlt(hdc, 0, 0, w, ht, Some(mdc), 0, 0, SRCCOPY);
    SelectObject(mdc, old);
    let _ = DeleteDC(mdc);
    let _ = DeleteObject(bmp.into());
}

// ---- input ----

unsafe fn click(h: HWND, id: i32, right: bool) {
    let a = api();
    match id {
        HELP => {
            PAGE.store(1 - PAGE.load(Relaxed), Relaxed);
            HOVER.store(0, Relaxed);
            tip_hide();
        }
        UPD => open(w!("https://github.com/bonanto/bark/releases/latest")),
        LEARN => open(w!("https://github.com/bonanto/bark")),
        LOCK | AUTO | QUIET | SCHED | DING => {
            let k = toggle_key(id);
            (a.set)(k, if (a.get)(k) != 0 { 0 } else { 1 });
        }
        x if x >= SEG0 => {
            let (seg, part) = ((x - SEG0) / 3, (x - SEG0) % 3);
            let dir = match part {
                0 => 1,
                2 => -1,
                _ => {
                    if right {
                        -1
                    } else {
                        1
                    }
                }
            };
            seg_adjust(seg, dir, false);
        }
        _ => {}
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
                click(h, was, false);
            }
        }
    }
    inv(h);
}

pub unsafe fn right_up(h: HWND, x: i32, y: i32) {
    if let Some(it) = hit(x, y) {
        if it.kind == Kind::Seg {
            click(h, it.id, true);
        }
    }
}

pub unsafe fn capture_lost() {
    if PRESS.load(Relaxed) == SLIDER {
        PRESS.store(0, Relaxed);
    }
}

pub unsafe fn wheel(h: HWND, x: i32, y: i32, delta: i32) {
    let Some(it) = hit(x, y) else { return };
    let dir = if delta > 0 { 1 } else { -1 };
    if it.id == SLIDER {
        set_vol((api().get)(K_VOLUME) + dir);
    } else if it.id >= SEG0 {
        seg_adjust((it.id - SEG0) / 3, dir, true);
    }
    inv(h);
}

pub unsafe fn key(h: HWND, vk: u32) {
    let d = match vk {
        0x25 | 0x28 => -1,
        0x27 | 0x26 => 1,
        _ => return,
    };
    set_vol((api().get)(K_VOLUME) + d);
    inv(h);
}

// ---- tooltips (own popup, follows the theme) ----

pub fn set_tip(h: HWND) {
    TIPW.store(h.0 as isize, Relaxed);
}

unsafe fn tip_hide() {
    let t = HWND(TIPW.load(Relaxed) as *mut c_void);
    if !t.is_invalid() {
        let _ = ShowWindow(t, SW_HIDE);
    }
}

pub unsafe fn tip_timer(h: HWND) {
    let _ = KillTimer(Some(h), T_TIP);
    let id = HOVER.load(Relaxed);
    if id == 0 || PRESS.load(Relaxed) != 0 {
        return;
    }
    let Some(it) = items(PAGE.load(Relaxed)).into_iter().find(|i| i.id == id) else {
        return;
    };
    let tw = HWND(TIPW.load(Relaxed) as *mut c_void);
    if tw.is_invalid() || FN.load(Relaxed) == 0 {
        return;
    }
    if let Ok(mut s) = TIP_TEXT.lock() {
        *s = it.tip.to_string();
    }
    let dc = GetDC(Some(tw));
    let old = SelectObject(dc, HGDIOBJ(FN.load(Relaxed) as *mut c_void));
    let mut rc = RECT { left: 0, top: 0, right: sc(240), bottom: 0 };
    let mut t: Vec<u16> = it.tip.encode_utf16().collect();
    DrawTextW(dc, &mut t, &mut rc, DT_CALCRECT | DT_WORDBREAK | DT_LEFT | DT_NOPREFIX);
    SelectObject(dc, old);
    ReleaseDC(Some(tw), dc);
    let pad = sc(8);
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
        let pad = sc(8);
        let mut r2 = RECT { left: pad, top: pad, right: rc.right - pad, bottom: rc.bottom - pad };
        SelectObject(dc, HGDIOBJ(FN.load(Relaxed) as *mut c_void));
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, bgr(p.fg));
        let mut t: Vec<u16> = TIP_TEXT.lock().map(|s| s.encode_utf16().collect()).unwrap_or_default();
        DrawTextW(dc, &mut t, &mut r2, DT_WORDBREAK | DT_LEFT | DT_NOPREFIX);
        let _ = EndPaint(h, &ps);
        return LRESULT(0);
    }
    DefWindowProcW(h, m, wp, lp)
}