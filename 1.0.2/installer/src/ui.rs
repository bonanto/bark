// The installer window: one custom-drawn window (same drawing approach as Bark's own flyout).
use crate::{
    gfx::*,
    md,
    reg::Key,
    work::{self, Settings, WM_WORK},
    VERSION,
};
use std::{cell::RefCell, ffi::c_void, mem::size_of, path::PathBuf, sync::atomic::Ordering::Relaxed};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        System::LibraryLoader::GetModuleHandleW,
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
};

const EULA: &str = include_str!("../../EULA.md");

// Layout, in 96-dpi units.
const CW: i32 = 660;
const CH: i32 = 440;
const BAN: i32 = 210;
const X0: i32 = BAN + 30;
const X1: i32 = CW - 30;

const BTN_NEXT: i32 = 1;
const BTN_BACK: i32 = 2;
const BTN_CANCEL: i32 = 3;
const CHK_ACCEPT: i32 = 10;
const RAD_DEFAULT: i32 = 11;
const RAD_CUSTOM: i32 = 12;
const TG_DING: i32 = 13;
const TG_UPD: i32 = 14;
const TG_AUTO: i32 = 15;
const RAD_MODIFY: i32 = 16;
const RAD_REMOVE: i32 = 17;
const CHK_LOGS: i32 = 18;
const SCROLL: i32 = 19;
const CHK_START: i32 = 20;

// font slots
const F_BODY: usize = 0;
const F_BOLD: usize = 1;
const F_SMALL: usize = 2;
const F_TITLE: usize = 3;
const F_BIG: usize = 4;
const F_EB: usize = 5; // EULA bold
const F_EH: usize = 6; // EULA heading
const F_EN: usize = 7; // EULA normal

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Install,
    Maint,
    Uninstall,
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Welcome,
    License,
    Settings,
    Maint,
    Confirm,
    Working,
    Done,
}

#[derive(Clone, Copy, PartialEq)]
enum Flow {
    Install,
    Uninstall,
}

#[derive(Clone, Copy, PartialEq)]
enum K {
    Primary,
    Secondary,
    Check,
    Radio,
    Toggle,
    Scroll,
}

#[derive(Clone)]
struct Item {
    id: i32,
    kind: K,
    r: [i32; 4],
    label: String,
    sub: String,
    on: bool,
    enabled: bool,
}

struct Pal {
    bg: u32,
    fg: u32,
    dim: u32,
    acc: u32,
    acc_hover: u32,
    acc_press: u32,
    acc_fg: u32,
    tile: u32,
    tile_hover: u32,
    tile_press: u32,
    tile_bd: u32,
    track: u32,
    card: u32,
}

struct Run {
    x: i32,
    y: i32,
    text: Vec<u16>,
    font: usize,
}

struct App {
    mode: Mode,
    page: Page,
    flow: Flow,
    existing: Option<(String, PathBuf)>,
    accepted: bool,
    custom: bool,
    ding: bool,
    updates: bool,
    autostart: bool,
    remove: bool,
    clear_logs: bool,
    hover: i32,
    press: i32,
    dpi: i32,
    pal: Pal,
    fonts: [isize; 8],
    icon: isize,
    runs: Vec<Run>,
    line_h: i32,
    content_h: i32,
    scroll: i32,
    drag: Option<(i32, i32)>,
    shown: f32,  // progress bar as drawn (eases towards the real value)
    target: i32, // real progress reported by the worker
    state: i32,  // 0 running, 1 finished, 2 failed
    start_bark: bool,
    dir: Option<PathBuf>,
    status: String,
    done_ok: bool,
    done_msg: String,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

// try_borrow_mut: a re-entrant call just skips its work instead of panicking.
fn with<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|c| c.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

enum Cmd {
    None,
    Redraw,
    Quit,
    Work(Flow),
    Launch(PathBuf),
}

fn sc(a: &App, v: i32) -> i32 {
    v * a.dpi / 96
}

fn bgr(c: u32) -> COLORREF {
    COLORREF(((c & 255) << 16) | (c & 0xFF00) | ((c >> 16) & 255))
}

// ------------------------------------------------------------------ palette / fonts

fn make_pal(dark: bool) -> Pal {
    let acc_at = |i: usize| -> Option<u32> {
        Key::open(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent", false)
            .and_then(|k| k.bytes("AccentPalette"))
            .and_then(|b| b.get(i * 4..i * 4 + 3).map(|c| rgb(c[0], c[1], c[2])))
    };
    if dark {
        Pal {
            bg: rgb(44, 44, 44),
            fg: rgb(255, 255, 255),
            dim: rgb(165, 165, 165),
            acc: acc_at(1).unwrap_or(rgb(76, 194, 255)),
            acc_hover: acc_at(0).unwrap_or(rgb(153, 235, 255)),
            acc_press: acc_at(2).unwrap_or(rgb(0, 145, 248)),
            acc_fg: rgb(0, 0, 0),
            tile: rgb(60, 60, 60),
            tile_hover: rgb(70, 70, 70),
            tile_press: rgb(52, 52, 52),
            tile_bd: rgb(72, 72, 72),
            track: rgb(110, 110, 110),
            card: rgb(34, 34, 34),
        }
    } else {
        let bg = rgb(243, 243, 243);
        let acc = acc_at(4).unwrap_or(rgb(0, 103, 192));
        Pal {
            bg,
            fg: rgb(26, 26, 26),
            dim: rgb(96, 96, 96),
            acc,
            acc_hover: mix(acc, bg, 0.10),
            acc_press: mix(acc, bg, 0.20),
            acc_fg: rgb(255, 255, 255),
            tile: rgb(251, 251, 251),
            tile_hover: rgb(244, 244, 244),
            tile_press: rgb(240, 240, 240),
            tile_bd: rgb(224, 224, 224),
            track: rgb(168, 168, 168),
            card: rgb(255, 255, 255),
        }
    }
}

unsafe fn make_fonts(dpi: u32) -> [isize; 8] {
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
        return [0; 8];
    }
    let mk = |scale: f32, weight: i32| -> isize {
        let mut lf = ncm.lfMessageFont;
        lf.lfHeight = (lf.lfHeight as f32 * scale) as i32;
        lf.lfWeight = weight;
        lf.lfQuality = CLEARTYPE_QUALITY;
        CreateFontIndirectW(&lf).0 as isize
    };
    [mk(1.12, 400), mk(1.12, 600), mk(1.0, 400), mk(2.0, 600), mk(2.7, 600), mk(1.0, 600), mk(1.3, 600), mk(1.0, 400)]
}

// ------------------------------------------------------------------ EULA layout

unsafe fn measure(dc: HDC, font: isize, s: &[u16]) -> (i32, i32) {
    SelectObject(dc, HGDIOBJ(font as *mut c_void));
    let mut sz = SIZE::default();
    let _ = GetTextExtentPoint32W(dc, s, &mut sz);
    (sz.cx, sz.cy)
}

fn view_rect(a: &App) -> [i32; 4] {
    [sc(a, X0), sc(a, 92), sc(a, X1), sc(a, 322)]
}

unsafe fn build_layout(a: &mut App) {
    let blocks = md::parse(EULA);
    let dc = GetDC(None);
    let dpi = a.dpi;
    let g = |v: i32| v * dpi / 96;
    let v = view_rect(a);
    let width = (v[2] - v[0]) - 2 * g(14) - g(16);
    let fonts = a.fonts;
    let space_w = measure(dc, fonts[F_EN], &[32u16]).0;
    let line_h = measure(dc, fonts[F_EN], &[65u16, 103u16]).1 * 125 / 100;
    let mut runs: Vec<Run> = Vec::new();
    let mut y = 0;
    for b in &blocks {
        let (base_font, base_bold, before, after) = match b.kind {
            md::Kind::H1 => (F_EH, true, 0, 10),
            md::Kind::H2 => (F_EB, true, 12, 2),
            md::Kind::H3 => (F_EB, true, 8, 2),
            md::Kind::P => (F_EN, false, 0, 8),
        };
        if y > 0 {
            y += g(before);
        }
        let mut x = 0;
        let mut ly = y;
        for (word, bold) in md::words(&b.text, base_bold) {
            let f = if b.kind == md::Kind::P {
                if bold {
                    F_EB
                } else {
                    F_EN
                }
            } else {
                base_font
            };
            let wv: Vec<u16> = word.encode_utf16().collect();
            let ww = measure(dc, fonts[f], &wv).0;
            if x > 0 && x + ww > width {
                x = 0;
                ly += line_h;
            }
            runs.push(Run { x, y: ly, text: wv, font: f });
            x += ww + space_w;
        }
        y = ly + line_h + g(after);
    }
    ReleaseDC(None, dc);
    a.runs = runs;
    a.line_h = line_h;
    a.content_h = y;
}

fn max_scroll(a: &App) -> i32 {
    let v = view_rect(a);
    (a.content_h - ((v[3] - v[1]) - 2 * sc(a, 10))).max(0)
}

// thumb (top, bottom) and track (top, bottom) of the EULA scrollbar
fn thumb(a: &App) -> ((i32, i32), (i32, i32)) {
    let v = view_rect(a);
    let (tt, tb) = (v[1] + sc(a, 6), v[3] - sc(a, 6));
    let th_full = tb - tt;
    let ms = max_scroll(a);
    let inner = (v[3] - v[1]) - 2 * sc(a, 10);
    let th = if a.content_h > 0 { (th_full * inner / a.content_h.max(1)).clamp(sc(a, 30), th_full) } else { th_full };
    let top = if ms > 0 { tt + (th_full - th) * a.scroll / ms } else { tt };
    ((top, top + th), (tt, tb))
}

// ------------------------------------------------------------------ items (everything clickable)

fn items(a: &App) -> Vec<Item> {
    let s = |v: i32| sc(a, v);
    let mut v: Vec<Item> = Vec::new();
    let mk = |id: i32, kind: K, r: [i32; 4], label: &str, sub: &str, on: bool, enabled: bool| Item {
        id,
        kind,
        r,
        label: label.to_string(),
        sub: sub.to_string(),
        on,
        enabled,
    };
    let (by, bh) = (s(CH - 60), s(34));
    let cancel = [s(X1 - 88), by, s(X1), by + bh];
    let next = [s(X1 - 88 - 8 - 104), by, s(X1 - 88 - 8), by + bh];
    let back = [s(X1 - 88 - 8 - 104 - 8 - 88), by, s(X1 - 88 - 8 - 104 - 8), by + bh];
    let (l, r) = (s(X0), s(X1));

    match a.page {
        Page::Welcome => {
            v.push(mk(BTN_NEXT, K::Primary, next, "Next", "", false, true));
            v.push(mk(BTN_CANCEL, K::Secondary, cancel, "Cancel", "", false, true));
        }
        Page::License => {
            v.push(mk(SCROLL, K::Scroll, view_rect(a), "", "", false, true));
            v.push(mk(CHK_ACCEPT, K::Check, [l, s(334), r, s(360)], "I accept the terms of this agreement", "", a.accepted, true));
            v.push(mk(BTN_BACK, K::Secondary, back, "Back", "", false, true));
            v.push(mk(BTN_NEXT, K::Primary, next, "Next", "", false, a.accepted));
            v.push(mk(BTN_CANCEL, K::Secondary, cancel, "Cancel", "", false, true));
        }
        Page::Settings => {
            v.push(mk(
                RAD_DEFAULT,
                K::Radio,
                [l, s(92), r, s(148)],
                "Default settings",
                "Recommended. Check for updates and start with Windows.",
                !a.custom,
                true,
            ));
            v.push(mk(
                RAD_CUSTOM,
                K::Radio,
                [l, s(156), r, s(212)],
                "Custom settings",
                "Choose exactly what Bark does.",
                a.custom,
                true,
            ));
            if a.custom {
                v.push(mk(TG_DING, K::Toggle, [l, s(224), r, s(258)], "Silence volume-change ding", "", a.ding, true));
                v.push(mk(TG_UPD, K::Toggle, [l, s(260), r, s(294)], "Check for updates automatically", "", a.updates, true));
                v.push(mk(TG_AUTO, K::Toggle, [l, s(296), r, s(330)], "Start with Windows", "", a.autostart, true));
            }
            if a.mode == Mode::Maint {
                v.push(mk(BTN_BACK, K::Secondary, back, "Back", "", false, true));
            } else {
                v.push(mk(BTN_BACK, K::Secondary, back, "Back", "", false, true));
            }
            let label = if a.mode == Mode::Maint { "Apply" } else { "Install" };
            v.push(mk(BTN_NEXT, K::Primary, next, label, "", false, true));
            v.push(mk(BTN_CANCEL, K::Secondary, cancel, "Cancel", "", false, true));
        }
        Page::Maint => {
            v.push(mk(
                RAD_MODIFY,
                K::Radio,
                [l, s(110), r, s(170)],
                "Modify or repair",
                "Reinstall Bark's files and change its settings.",
                !a.remove,
                true,
            ));
            v.push(mk(RAD_REMOVE, K::Radio, [l, s(178), r, s(238)], "Uninstall", "Remove Bark from this PC.", a.remove, true));
            v.push(mk(BTN_NEXT, K::Primary, next, "Next", "", false, true));
            v.push(mk(BTN_CANCEL, K::Secondary, cancel, "Cancel", "", false, true));
        }
        Page::Confirm => {
            v.push(mk(CHK_LOGS, K::Check, [l, s(318), r, s(344)], "Also delete the log files and the whole Bark folder", "", a.clear_logs, true));
            if a.mode == Mode::Maint {
                v.push(mk(BTN_BACK, K::Secondary, back, "Back", "", false, true));
            }
            v.push(mk(BTN_NEXT, K::Primary, next, "Uninstall", "", false, true));
            v.push(mk(BTN_CANCEL, K::Secondary, cancel, "Cancel", "", false, true));
        }
        Page::Working => {}
        Page::Done => {
            if a.done_ok && a.flow == Flow::Install {
                v.push(mk(CHK_START, K::Check, [l, s(318), r, s(344)], "Start Bark after Installation", "", a.start_bark, true));
            }
            v.push(mk(BTN_NEXT, K::Primary, cancel, "Finish", "", false, true));
        }
    }
    v
}

fn hit(a: &App, x: i32, y: i32) -> Option<Item> {
    items(a).into_iter().rev().find(|i| i.enabled && x >= i.r[0] && x < i.r[2] && y >= i.r[1] && y < i.r[3])
}

// ------------------------------------------------------------------ drawing

unsafe fn tx(dc: HDC, t: &str, rc: [i32; 4], font: isize, col: u32, fl: DRAW_TEXT_FORMAT) {
    // DrawTextW with an empty string (zero length, dangling pointer) plus an ellipsis flag faults inside GDI.
    // The first paint of the "Installing" page has an empty status line, so never draw empty text.
    if t.is_empty() {
        return;
    }
    SelectObject(dc, HGDIOBJ(font as *mut c_void));
    SetTextColor(dc, bgr(col));
    let mut v: Vec<u16> = t.encode_utf16().collect();
    let mut r = RECT { left: rc[0], top: rc[1], right: rc[2], bottom: rc[3] };
    DrawTextW(dc, &mut v, &mut r, fl);
}

unsafe fn draw_shapes(cv: &Canvas, a: &App, it: &Item) {
    let p = &a.pal;
    let s = a.dpi as f32 / 96.0;
    let [l, t, r, b] = it.r.map(|v| v as f32);
    let cy = (t + b) / 2.0;
    let hot = a.hover == it.id;
    let down = a.press == it.id;
    let bd = s.round().max(1.0);
    match it.kind {
        K::Primary => {
            let f = if !it.enabled {
                p.tile
            } else if down {
                p.acc_press
            } else if hot {
                p.acc_hover
            } else {
                p.acc
            };
            cv.rrect(l, t, r, b, 4.0 * s, if it.enabled { mix(f, p.acc_fg, 0.0) } else { p.tile_bd });
            cv.rrect(l + bd, t + bd, r - bd, b - bd, 4.0 * s - bd, f);
        }
        K::Secondary => {
            let f = if down {
                p.tile_press
            } else if hot {
                p.tile_hover
            } else {
                p.tile
            };
            cv.rrect(l, t, r, b, 4.0 * s, p.tile_bd);
            cv.rrect(l + bd, t + bd, r - bd, b - bd, 4.0 * s - bd, f);
        }
        K::Check => {
            let bx = 18.0 * s;
            let (x0, y0) = (l, cy - bx / 2.0);
            if it.on {
                cv.rrect(x0, y0, x0 + bx, y0 + bx, 4.0 * s, p.acc);
                cv.line(x0 + 4.5 * s, cy, x0 + 7.8 * s, cy + 3.6 * s, 2.0 * s, p.acc_fg);
                cv.line(x0 + 7.8 * s, cy + 3.6 * s, x0 + 13.6 * s, cy - 4.0 * s, 2.0 * s, p.acc_fg);
            } else {
                cv.rrect(x0, y0, x0 + bx, y0 + bx, 4.0 * s, p.dim);
                cv.rrect(x0 + bd, y0 + bd, x0 + bx - bd, y0 + bx - bd, 4.0 * s - bd, if hot { p.tile_hover } else { p.bg });
            }
        }
        K::Radio => {
            let f = if down {
                p.tile_press
            } else if hot {
                p.tile_hover
            } else {
                p.tile
            };
            cv.rrect(l, t, r, b, 6.0 * s, if it.on { p.acc } else { p.tile_bd });
            let w2 = if it.on { 1.5 * s } else { bd };
            cv.rrect(l + w2, t + w2, r - w2, b - w2, 6.0 * s - w2, f);
            let (cx, rad) = (l + 24.0 * s, 9.0 * s);
            let ccol = if it.on { p.acc } else { p.dim };
            cv.circle(cx, cy, rad, ccol);
            cv.circle(cx, cy, rad - 1.5 * s, f);
            if it.on {
                cv.circle(cx, cy, 5.0 * s, p.acc);
            }
        }
        K::Toggle => {
            let (sh, sw) = (18.0 * s, 36.0 * s);
            let x0 = r - sw;
            if it.on {
                cv.rrect(x0, cy - sh / 2.0, r, cy + sh / 2.0, sh / 2.0, p.acc);
                cv.circle(r - sh / 2.0, cy, (if hot { 6.5 } else { 6.0 }) * s, p.acc_fg);
            } else {
                cv.rrect(x0, cy - sh / 2.0, r, cy + sh / 2.0, sh / 2.0, p.dim);
                cv.rrect(x0 + 1.5 * s, cy - sh / 2.0 + 1.5 * s, r - 1.5 * s, cy + sh / 2.0 - 1.5 * s, sh / 2.0 - 1.5 * s, p.bg);
                cv.circle(x0 + sh / 2.0, cy, (if hot { 5.5 } else { 5.0 }) * s, p.dim);
            }
        }
        K::Scroll => {
            cv.rrect(l, t, r, b, 6.0 * s, p.tile_bd);
            cv.rrect(l + bd, t + bd, r - bd, b - bd, 6.0 * s - bd, p.card);
            if max_scroll(a) > 0 {
                let ((tt, tb), _) = thumb(a);
                let x1 = r - 5.0 * s;
                let col = if a.drag.is_some() || hot { p.dim } else { p.track };
                cv.rrect(x1 - 5.0 * s, tt as f32, x1, tb as f32, 2.5 * s, col);
            }
        }
    }
}

unsafe fn draw_item_text(mdc: HDC, a: &App, it: &Item) {
    let p = &a.pal;
    let s = |v: i32| sc(a, v);
    let [l, t, r, b] = it.r;
    let f = &a.fonts;
    let vcs = DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS;
    match it.kind {
        K::Primary => tx(mdc, &it.label, it.r, f[F_BODY], if it.enabled { p.acc_fg } else { p.dim }, vcs | DT_CENTER),
        K::Secondary => tx(mdc, &it.label, it.r, f[F_BODY], p.fg, vcs | DT_CENTER),
        K::Check => tx(mdc, &it.label, [l + s(28), t, r, b], f[F_BODY], p.fg, vcs),
        K::Toggle => tx(mdc, &it.label, [l, t, r - s(50), b], f[F_BODY], p.fg, vcs),
        K::Radio => {
            tx(mdc, &it.label, [l + s(46), t + s(9), r - s(10), t + s(31)], f[F_BOLD], p.fg, DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS);
            tx(mdc, &it.sub, [l + s(46), t + s(31), r - s(10), b - s(4)], f[F_SMALL], p.dim, DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS);
        }
        K::Scroll => {
            let pad = s(14);
            let (vt, vb) = (t + s(10), b - s(10));
            let saved = SaveDC(mdc);
            let _ = IntersectClipRect(mdc, l + s(2), vt, r - s(2), vb);
            for run in &a.runs {
                let y = vt + run.y - a.scroll;
                if y + a.line_h < vt || y > vb {
                    continue;
                }
                SelectObject(mdc, HGDIOBJ(f[run.font] as *mut c_void));
                SetTextColor(mdc, bgr(p.fg));
                let mut r2 = RECT { left: l + pad + run.x, top: y, right: r, bottom: y + a.line_h };
                let mut t2 = run.text.clone();
                DrawTextW(mdc, &mut t2, &mut r2, DT_SINGLELINE | DT_NOPREFIX | DT_NOCLIP);
            }
            let _ = RestoreDC(mdc, saved);
        }
    }
}

unsafe fn draw(cv: &Canvas, mdc: HDC, a: &App) {
    let p = &a.pal;
    let s = |v: i32| sc(a, v);
    let (w, h) = (cv.w, cv.h);
    cv.fill(p.bg);

    // banner
    let bw = s(BAN);
    let (c0, c1) = (rgb(30, 118, 247), rgb(8, 56, 176));
    for y in 0..h {
        cv.rect(0, y, bw, y + 1, mix(c0, c1, y as f32 / h as f32));
    }
    // footer divider
    cv.rect(bw, s(CH - 72), w, s(CH - 72) + 1, p.tile_bd);

    let its = items(a);
    for it in &its {
        draw_shapes(cv, a, it);
    }
    let _ = GdiFlush();

    // banner icon (alpha-blended by GDI)
    if a.icon != 0 {
        let sz = s(112);
        let _ = DrawIconEx(mdc, (bw - sz) / 2, s(64), HICON(a.icon as *mut c_void), sz, sz, 0, None, DI_NORMAL);
    }
    SetBkMode(mdc, TRANSPARENT);
    let f = &a.fonts;
    let white = rgb(255, 255, 255);
    let soft = rgb(200, 224, 255);
    tx(mdc, "Bark", [0, s(196), bw, s(246)], f[F_BIG], white, DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
    tx(mdc, "System Volume Fix", [0, s(246), bw, s(272)], f[F_BODY], soft, DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
    tx(mdc, &format!("Version {VERSION}"), [0, h - s(40), bw, h - s(14)], f[F_SMALL], soft, DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);

    let title = |t: &str| tx(mdc, t, [s(X0), s(34), s(X1), s(82)], f[F_TITLE], p.fg, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS);
    let body = |t: &str, top: i32, bottom: i32| {
        tx(mdc, t, [s(X0), s(top), s(X1), s(bottom)], f[F_BODY], p.fg, DT_WORDBREAK | DT_NOPREFIX)
    };
    match a.page {
        Page::Welcome => {
            title("Welcome to Bark");
            let mut t = String::from(
                "Windows keeps resetting the volume of \u{201C}System sounds\u{201D}, which affects system sounds and notifications. Bark fixes this by holding the volume at the value you choose.\n\nIt lives in the notification area, has hotkeys, and can switch the sound scheme to No Sounds when you want quiet.\n\nThis wizard installs Bark for your account. No administrator rights are needed.",
            );
            if let Some((v, _)) = &a.existing {
                t.push_str(&format!("\n\nBark {v} is installed and will be updated to {VERSION}."));
            }
            body(&t, 92, 360);
        }
        Page::License => title("License agreement"),
        Page::Settings => {
            title(if a.mode == Mode::Maint { "Modify Bark" } else { "Choose settings" });
            let dir = a.existing.as_ref().map(|e| e.1.clone()).unwrap_or_else(work::default_dir);
            tx(
                mdc,
                &format!("Install location:  {}", dir.display()),
                [s(X0), s(338), s(X1), s(362)],
                f[F_SMALL],
                p.dim,
                DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_PATH_ELLIPSIS,
            );
        }
        Page::Maint => {
            title("Bark is installed");
            let v = a.existing.as_ref().map(|e| e.0.clone()).unwrap_or_default();
            body(&format!("Version {v} is already on this PC. What would you like to do?"), 88, 112);
        }
        Page::Confirm => {
            title("Uninstall Bark");
            body(
                "Uninstalling Bark will:\n\u{2022}  close Bark if it is running\n\u{2022}  restore your Windows sound scheme, if Quiet mode is on\n\u{2022}  stop holding the System sounds volume (Windows manages it again)\n\u{2022}  remove Bark's settings, autostart entry and Start menu shortcut\n\u{2022}  delete the program files\n\nLog files are kept unless you tick the box below.",
                92,
                310,
            );
        }
        Page::Working => {
            title(if a.flow == Flow::Install { "Installing Bark" } else { "Uninstalling Bark" });
            tx(mdc, &a.status, [s(X0), s(170), s(X1 - 60), s(196)], f[F_BODY], p.dim, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS);
            tx(mdc, &format!("{}%", a.shown as i32), [s(X1 - 56), s(170), s(X1), s(196)], f[F_BODY], p.fg, DT_RIGHT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
        }
        Page::Done => {
            if !a.done_ok {
                title("Something went wrong");
                body(&a.done_msg, 92, 330);
            } else if a.flow == Flow::Install {
                title("Bark is installed");
                body(
                    "Bark has been installed. Once it is running, its icon is in the hidden-icons area of the taskbar (the ^ arrow). Click it to open the volume panel.\n\nBark is also in the Start menu, and in Settings > Apps where you can change or remove it.",
                    92,
                    300,
                );
            } else {
                title("Bark was removed");
                body("Bark has been uninstalled. Thank you for trying it.", 92, 200);
            }
        }
    }

    for it in &its {
        draw_item_text(mdc, a, it);
    }

    // progress bar (drawn last, text above it is already in place)
    if a.page == Page::Working {
        let sf = a.dpi as f32 / 96.0;
        let (l, t, r) = (s(X0) as f32, s(206) as f32, s(X1) as f32);
        let bh = 10.0 * sf;
        cv.rrect(l, t, r, t + bh, bh / 2.0, p.track);
        let fillw = (r - l) * (a.shown.clamp(0.0, 100.0) / 100.0);
        if fillw > 1.0 {
            cv.rrect(l, t, l + fillw.max(bh), t + bh, bh / 2.0, p.acc);
        }
    }
    let _ = GdiFlush();
}

unsafe fn paint(h: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(h, &mut ps);
    with(|a| {
        let (w, ht) = (sc(a, CW), sc(a, CH));
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = w;
        bi.bmiHeader.biHeight = -ht;
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        let mut bits: *mut c_void = std::ptr::null_mut();
        if let Ok(bmp) = CreateDIBSection(Some(hdc), &bi, DIB_RGB_COLORS, &mut bits, None, 0) {
            if !bits.is_null() {
                let mdc = CreateCompatibleDC(Some(hdc));
                let old = SelectObject(mdc, bmp.into());
                let cv = Canvas { p: bits as *mut u32, w, h: ht };
                draw(&cv, mdc, a);
                let _ = BitBlt(hdc, 0, 0, w, ht, Some(mdc), 0, 0, SRCCOPY);
                SelectObject(mdc, old);
                let _ = DeleteDC(mdc);
            }
            let _ = DeleteObject(bmp.into());
        }
    });
    let _ = EndPaint(h, &ps);
}

// ------------------------------------------------------------------ behaviour

fn begin_work(a: &mut App) {
    a.page = Page::Working;
    a.shown = 0.0;
    a.target = 0;
    a.state = 0;
    a.status = String::new();
}

fn click(a: &mut App, id: i32) -> Cmd {
    match id {
        CHK_ACCEPT => a.accepted = !a.accepted,
        RAD_DEFAULT => a.custom = false,
        RAD_CUSTOM => a.custom = true,
        TG_DING => a.ding = !a.ding,
        TG_UPD => a.updates = !a.updates,
        TG_AUTO => a.autostart = !a.autostart,
        RAD_MODIFY => a.remove = false,
        RAD_REMOVE => a.remove = true,
        CHK_LOGS => a.clear_logs = !a.clear_logs,
        CHK_START => a.start_bark = !a.start_bark,
        BTN_CANCEL => return Cmd::Quit,
        BTN_BACK => {
            a.page = match a.page {
                Page::License => Page::Welcome,
                Page::Settings => {
                    if a.mode == Mode::Maint {
                        Page::Maint
                    } else {
                        Page::License
                    }
                }
                Page::Confirm => Page::Maint,
                p => p,
            }
        }
        BTN_NEXT => match a.page {
            Page::Welcome => a.page = Page::License,
            Page::License => {
                if a.accepted {
                    a.page = Page::Settings
                }
            }
            Page::Settings => {
                if !a.custom {
                    a.ding = false;
                    a.updates = true;
                    a.autostart = true;
                }
                a.flow = Flow::Install;
                begin_work(a);
                return Cmd::Work(Flow::Install);
            }
            Page::Maint => {
                if a.remove {
                    a.page = Page::Confirm
                } else {
                    a.page = Page::Settings
                }
            }
            Page::Confirm => {
                a.flow = Flow::Uninstall;
                begin_work(a);
                return Cmd::Work(Flow::Uninstall);
            }
            Page::Done => {
                if a.done_ok && a.flow == Flow::Install && a.start_bark {
                    if let Some(d) = a.dir.clone() {
                        return Cmd::Launch(d);
                    }
                }
                return Cmd::Quit;
            }
            Page::Working => {}
        },
        _ => {}
    }
    Cmd::Redraw
}

unsafe fn run_cmd(h: HWND, cmd: Cmd) {
    match cmd {
        Cmd::None => {}
        Cmd::Redraw => {
            let _ = InvalidateRect(Some(h), None, false);
        }
        Cmd::Quit => {
            let _ = DestroyWindow(h);
        }
        Cmd::Launch(dir) => {
            work::launch_bark(&dir);
            let _ = DestroyWindow(h);
        }
        Cmd::Work(flow) => {
            let info = with(|a| {
                let s = Settings { ding: a.ding, updates: a.updates, autostart: a.autostart };
                let dir = a.existing.as_ref().map(|e| e.1.clone()).unwrap_or_else(work::default_dir);
                a.dir = Some(dir.clone());
                (s, dir, a.clear_logs)
            });
            if let Some((s, dir, clear)) = info {
                work::STATE.store(0, Relaxed);
                work::PROGRESS.store(0, Relaxed);
                let hw = h.0 as isize;
                let started = std::thread::Builder::new().stack_size(8 << 20).spawn(move || match flow {
                    Flow::Install => work::install(hw, s, dir),
                    Flow::Uninstall => work::uninstall(hw, clear, dir),
                });
                if started.is_err() {
                    work::fail(hw, "Could not start the setup worker.");
                }
                let _ = SetTimer(Some(h), 1, 16, None); // drives the progress bar animation
            }
            let _ = InvalidateRect(Some(h), None, false);
        }
    }
}

fn xy(lp: LPARAM) -> (i32, i32) {
    ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32)
}

unsafe fn on_move(h: HWND, x: i32, y: i32) {
    let redraw = with(|a| {
        if let Some((y0, s0)) = a.drag {
            let (th, tr) = thumb(a);
            let room = (tr.1 - tr.0) - (th.1 - th.0);
            if room > 0 {
                let ms = max_scroll(a);
                a.scroll = (s0 + (y - y0) * ms / room).clamp(0, ms);
            }
            return true;
        }
        let id = hit(a, x, y).map_or(0, |i| i.id);
        if id != a.hover {
            a.hover = id;
            return true;
        }
        false
    });
    if redraw == Some(true) {
        let _ = InvalidateRect(Some(h), None, false);
    }
    let mut te = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: h, dwHoverTime: 0 };
    let _ = TrackMouseEvent(&mut te);
}

unsafe fn on_down(h: HWND, x: i32, y: i32) {
    let cap = with(|a| {
        let it = hit(a, x, y);
        a.press = it.as_ref().map_or(0, |i| i.id);
        if let Some(it) = it {
            if it.id == SCROLL && max_scroll(a) > 0 && x >= it.r[2] - sc(a, 18) {
                let (th, tr) = thumb(a);
                if y < th.0 || y > th.1 {
                    // jump: centre the thumb on the click
                    let room = ((tr.1 - tr.0) - (th.1 - th.0)).max(1);
                    let ms = max_scroll(a);
                    a.scroll = ((y - (th.1 - th.0) / 2 - tr.0) * ms / room).clamp(0, ms);
                }
                a.drag = Some((y, a.scroll));
                a.press = 0;
                return true;
            }
        }
        false
    });
    if cap == Some(true) {
        let _ = SetCapture(h);
    }
    let _ = InvalidateRect(Some(h), None, false);
}

unsafe fn on_up(h: HWND, x: i32, y: i32) {
    let dragging = with(|a| a.drag.take().is_some());
    if dragging == Some(true) {
        let _ = ReleaseCapture();
        let _ = InvalidateRect(Some(h), None, false);
        return;
    }
    let cmd = with(|a| {
        let was = a.press;
        a.press = 0;
        if was != 0 {
            if let Some(it) = hit(a, x, y) {
                if it.id == was {
                    return click(a, was);
                }
            }
        }
        Cmd::Redraw
    })
    .unwrap_or(Cmd::None);
    run_cmd(h, cmd);
}

unsafe fn on_key(h: HWND, vk: u32) {
    let cmd = with(|a| {
        let busy = a.page == Page::Working;
        match vk {
            0x0D if !busy => {
                // Enter = the primary button
                if items(a).iter().any(|i| i.id == BTN_NEXT && i.enabled) {
                    return click(a, BTN_NEXT);
                }
            }
            0x1B if !busy => return Cmd::Quit,
            0x20 if a.page == Page::License => {
                a.accepted = !a.accepted;
                return Cmd::Redraw;
            }
            0x20 if a.page == Page::Done && a.done_ok && a.flow == Flow::Install => {
                a.start_bark = !a.start_bark;
                return Cmd::Redraw;
            }
            0x26 | 0x28 | 0x21 | 0x22 | 0x24 | 0x23 if a.page == Page::License => {
                let page = (a.line_h * 8).max(1);
                let ms = max_scroll(a);
                a.scroll = match vk {
                    0x26 => a.scroll - a.line_h * 2,
                    0x28 => a.scroll + a.line_h * 2,
                    0x21 => a.scroll - page,
                    0x22 => a.scroll + page,
                    0x24 => 0,
                    _ => ms,
                }
                .clamp(0, ms);
                return Cmd::Redraw;
            }
            _ => {}
        }
        Cmd::None
    })
    .unwrap_or(Cmd::None);
    run_cmd(h, cmd);
}

fn sync_work(a: &mut App) {
    a.target = work::PROGRESS.load(Relaxed);
    a.status = work::status();
    a.state = work::STATE.load(Relaxed);
}

unsafe extern "system" fn wndproc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_PAINT => {
            paint(h);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEMOVE => {
            let (x, y) = xy(lp);
            on_move(h, x, y);
            LRESULT(0)
        }
        0x02A3 => {
            // WM_MOUSELEAVE
            with(|a| a.hover = 0);
            let _ = InvalidateRect(Some(h), None, false);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = xy(lp);
            on_down(h, x, y);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = xy(lp);
            on_up(h, x, y);
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
            let redraw = with(|a| {
                if a.page == Page::License {
                    let ms = max_scroll(a);
                    a.scroll = (a.scroll - delta * a.line_h * 3 / 120).clamp(0, ms);
                    true
                } else {
                    false
                }
            });
            if redraw == Some(true) {
                let _ = InvalidateRect(Some(h), None, false);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            on_key(h, wp.0 as u32);
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            with(|a| a.drag = None);
            LRESULT(0)
        }
        WM_WORK => {
            with(sync_work);
            let _ = InvalidateRect(Some(h), None, false);
            LRESULT(0)
        }
        WM_TIMER => {
            // Ease the bar towards the real progress; show the final page once it has reached 100% (or at once on error).
            let working = with(|a| {
                if a.page != Page::Working {
                    return false;
                }
                sync_work(a);
                let goal = if a.state == 1 { 100.0 } else { a.target as f32 };
                if goal > a.shown {
                    a.shown = (a.shown + ((goal - a.shown) * 0.12).max(0.6)).min(goal);
                }
                if a.state == 2 || (a.state == 1 && a.shown >= 100.0) {
                    a.page = Page::Done;
                    a.done_ok = a.state == 1;
                    a.done_msg = work::error();
                    return false;
                }
                true
            });
            if working == Some(false) {
                let _ = KillTimer(Some(h), 1);
            }
            let _ = InvalidateRect(Some(h), None, false);
            LRESULT(0)
        }
        WM_CLOSE => {
            // Closing is not allowed while files are being changed.
            if with(|a| a.page == Page::Working) != Some(true) {
                let _ = DestroyWindow(h);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

// ------------------------------------------------------------------ start-up

pub unsafe fn run(mode: Mode, existing: Option<(String, PathBuf)>) {
    let dpi = GetDpiForSystem();
    let dark = Key::open(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", false)
        .and_then(|k| k.dword("AppsUseLightTheme"))
        == Some(0);

    let Ok(hm) = GetModuleHandleW(None) else { return };
    let hi = HINSTANCE(hm.0);

    let cur = if existing.is_some() { work::read_settings() } else { Settings { ding: false, updates: true, autostart: true } };
    let page = match mode {
        Mode::Install => Page::Welcome,
        Mode::Maint => Page::Maint,
        Mode::Uninstall => {
            if existing.is_some() {
                Page::Confirm
            } else {
                Page::Done
            }
        }
    };
    let mut app = App {
        mode,
        page,
        flow: if mode == Mode::Uninstall { Flow::Uninstall } else { Flow::Install },
        existing: existing.clone(),
        accepted: false,
        custom: existing.is_some(),
        ding: cur.ding,
        updates: cur.updates,
        autostart: cur.autostart,
        remove: false,
        clear_logs: false,
        hover: 0,
        press: 0,
        dpi: dpi as i32,
        pal: make_pal(dark),
        fonts: make_fonts(dpi),
        icon: 0,
        runs: Vec::new(),
        line_h: 16,
        content_h: 0,
        scroll: 0,
        drag: None,
        shown: 0.0,
        target: 0,
        state: 0,
        start_bark: true,
        dir: None,
        status: String::new(),
        done_ok: mode != Mode::Uninstall || existing.is_some(),
        done_msg: if mode == Mode::Uninstall && existing.is_none() { "Bark is not installed on this PC.".to_string() } else { String::new() },
    };
    app.icon = LoadImageW(Some(hi), PCWSTR(1usize as *const u16), IMAGE_ICON, sc(&app, 112), sc(&app, 112), LR_DEFAULTCOLOR)
        .map_or(0, |i| i.0 as isize);
    build_layout(&mut app);
    let (cw, ch) = (sc(&app, CW), sc(&app, CH));
    let title = if mode == Mode::Uninstall { w!("Uninstall Bark") } else { w!("Bark Setup") };
    APP.with(|c| *c.borrow_mut() = Some(app));

    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wndproc),
        hInstance: hi,
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        hIcon: LoadIconW(Some(hi), PCWSTR(1usize as *const u16)).unwrap_or_default(),
        hIconSm: LoadIconW(Some(hi), PCWSTR(1usize as *const u16)).unwrap_or_default(),
        lpszClassName: w!("BarkSetupWnd"),
        ..Default::default()
    };
    if RegisterClassExW(&wc) == 0 {
        return;
    }

    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
    let mut rc = RECT { left: 0, top: 0, right: cw, bottom: ch };
    let _ = AdjustWindowRectExForDpi(&mut rc, style, false, WINDOW_EX_STYLE(0), dpi);
    let (ow, oh) = (rc.right - rc.left, rc.bottom - rc.top);
    let mut wa = RECT::default();
    let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut wa as *mut RECT as *mut c_void), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    let (x, y) = (wa.left + ((wa.right - wa.left) - ow) / 2, wa.top + ((wa.bottom - wa.top) - oh) / 2);

    let Ok(hwnd) = CreateWindowExW(WINDOW_EX_STYLE(0), w!("BarkSetupWnd"), title, style, x, y, ow, oh, None, None, Some(hi), None) else {
        return;
    };
    let d = dark as i32;
    let _ = DwmSetWindowAttribute(hwnd, DWMWINDOWATTRIBUTE(20), &d as *const i32 as *const c_void, 4);
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}
