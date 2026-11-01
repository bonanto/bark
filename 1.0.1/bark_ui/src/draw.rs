use crate::gfx::{mix, Canvas};
use crate::icons::*;
use crate::keys::*;
use crate::layout::*;
use crate::state::*;
use crate::{get, APP_VERSION};
use std::{ffi::c_void, mem::size_of, sync::atomic::Ordering::Relaxed};
use windows::Win32::{Foundation::*, Graphics::Gdi::*};

const ABOUT_TEXT: &str = "Windows keeps resetting the volume of \u{201C}System Sounds\u{201D} to 100%, which affects system sounds and notifications. This is a well-known issue on Windows 10 and 11, and Bark patches it by holding the volume at the level you choose.\n\nOnly manual changes are accepted, here in Bark or in Windows Settings > Sound > Volume mixer (System sounds). Windows\u{2019} automatic resets are ignored.\n\nHotkeys: Win+Alt+PgUp / PgDn adjust the volume, Win+Alt+Q toggles Quiet mode.";

pub struct Surf {
    dc: isize,
    bmp: isize,
    old: isize,
}

pub fn snap() -> u64 {
    let mut a: u64 = 1469598103934665603;
    for k in [
        K_VOLUME,
        K_RESETS,
        K_UPDATE_AVAIL,
        K_LOCK,
        K_DING,
        K_QUIET_MANUAL,
        K_SCHED_ON,
        K_SCHED_START,
        K_SCHED_END,
        K_HIDE_TRAY,
        K_CLOCK24,
        K_UPDATE_CHECK,
        K_SYS_ICON,
    ] {
        a = (a ^ get(k) as u32 as u64).wrapping_mul(1099511628211);
    }
    a
}

fn fmt_time(m: i32, c24: bool) -> String {
    let (h, mn) = (m / 60, m % 60);
    if c24 {
        format!("{h:02}:{mn:02}")
    } else {
        let h12 = if h % 12 == 0 { 12 } else { h % 12 };
        format!("{h12}:{mn:02} {}", if h < 12 { "AM" } else { "PM" })
    }
}

fn ampm(h: i32) -> &'static str {
    if h < 12 {
        "AM"
    } else {
        "PM"
    }
}

fn tile_on(id: i32) -> bool {
    if id == QUIET {
        get(K_QUIET_MANUAL) != 0
    } else {
        get(K_SCHED_ON) != 0
    }
}

unsafe fn tx(dc: HDC, t: &str, rc: [i32; 4], font: isize, col: u32, fl: DRAW_TEXT_FORMAT) {
    SelectObject(dc, HGDIOBJ(font as *mut c_void));
    SetTextColor(dc, bgr(col));
    let mut v: Vec<u16> = t.encode_utf16().collect();
    let mut r = RECT { left: rc[0], top: rc[1], right: rc[2], bottom: rc[3] };
    DrawTextW(dc, &mut v, &mut r, fl);
}

// Draws an icon glyph centred in `rc`. It is rendered white-on-black into a scratch bitmap and then used as a
// coverage mask, so the stroke weight is the same for any colour (GDI makes dark-on-light text look thin/faint).
unsafe fn gl(cv: &Canvas, dc: HDC, g: u16, rc: [i32; 4], font: isize, col: u32) {
    let (w, h) = (rc[2] - rc[0], rc[3] - rc[1]);
    if w <= 0 || h <= 0 {
        return;
    }
    let mut bi = BITMAPINFO::default();
    bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = w;
    bi.bmiHeader.biHeight = -h;
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    let mut bits: *mut c_void = std::ptr::null_mut();
    let Ok(bmp) = CreateDIBSection(Some(dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0) else {
        return;
    };
    if !bits.is_null() {
        let mdc = CreateCompatibleDC(Some(dc));
        let old = SelectObject(mdc, bmp.into());
        let prev = SelectObject(mdc, HGDIOBJ(font as *mut c_void));
        SetBkMode(mdc, TRANSPARENT);
        SetTextColor(mdc, COLORREF(0x00FF_FFFF));
        let mut v = [g];
        let mut r = RECT { left: 0, top: 0, right: w, bottom: h };
        DrawTextW(mdc, &mut v, &mut r, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        let _ = GdiFlush();
        cv.mask(rc[0], rc[1], w, h, bits as *const u32, col);
        SelectObject(mdc, prev);
        SelectObject(mdc, old);
        let _ = DeleteDC(mdc);
    }
    let _ = DeleteObject(bmp.into());
}

// Fill and 1px edge colours of a tile in its current state (values measured from the Windows 11 quick settings).
fn tile_cols(p: &Pal, on: bool, hot: bool, down: bool) -> (u32, u32) {
    if on {
        let f = if down {
            p.acc_press
        } else if hot {
            p.acc_hover
        } else {
            p.acc
        };
        (f, mix(f, p.edge, 0.08))
    } else {
        let f = if down {
            p.tile_press
        } else if hot {
            p.tile_hover
        } else {
            p.tile
        };
        (f, p.tile_bd)
    }
}

unsafe fn shape(cv: &Canvas, p: &Pal, it: &Item, s: f32, hover: i32, press: i32) {
    let [l, t, r, b] = it.r.map(|v| v as f32);
    let (cx, cy) = ((l + r) / 2.0, (t + b) / 2.0);
    let hot = hover == it.id;
    let down = press == it.id;
    match it.kind {
        Kind::Slider => {
            let (x0, x1) = slider_x();
            let (x0, x1) = (x0 as f32, x1 as f32);
            let v = get(K_VOLUME).clamp(0, 100) as f32;
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
        Kind::Row => {
            if hot {
                cv.rrect(l - 6.0 * s, t + 2.0 * s, r + 6.0 * s, b - 2.0 * s, 6.0 * s, p.hover);
            }
            let on = toggle_key(it.id).map_or(false, |k| get(k) != 0);
            let (sh, sw) = (18.0 * s, 36.0 * s);
            let x0 = r - sw;
            let (y0, y1) = (cy - sh / 2.0, cy + sh / 2.0);
            if on {
                cv.rrect(x0, y0, r, y1, sh / 2.0, p.acc);
                cv.circle(r - sh / 2.0, cy, (if hot { 6.5 } else { 6.0 }) * s, p.acc_fg);
            } else {
                let inner = if hot { p.hover } else { p.bg };
                cv.rrect(x0, y0, r, y1, sh / 2.0, p.dim);
                cv.rrect(x0 + 1.5 * s, y0 + 1.5 * s, r - 1.5 * s, y1 - 1.5 * s, sh / 2.0 - 1.5 * s, inner);
                cv.circle(x0 + sh / 2.0, cy, (if hot { 5.5 } else { 5.0 }) * s, p.dim);
            }
        }
        Kind::Tile | Kind::TileL | Kind::TileR | Kind::TileNav => {
            let on = it.kind != Kind::TileNav && tile_on(it.id);
            let (fill, edge) = tile_cols(p, on, hot, down);
            let (rad, bd) = (4.0 * s, s.round().max(1.0));
            let ri = (rad - bd).max(0.0);
            // Corner radii [tl, tr, br, bl]; the seam of a split tile is square. Each half owns its own edge,
            // the right half also draws the 1px divider on its left side.
            let (ro, rn, il, ir) = match it.kind {
                Kind::TileL => ([rad, 0.0, 0.0, rad], [ri, 0.0, 0.0, ri], l + bd, r),
                Kind::TileR => ([0.0, rad, rad, 0.0], [0.0, ri, ri, 0.0], l + bd, r - bd),
                _ => ([rad; 4], [ri; 4], l + bd, r - bd),
            };
            cv.rrect4(l, t, r, b, ro, edge);
            cv.rrect4(il, t + bd, ir, b - bd, rn, fill);
        }
        Kind::Round => {
            let rad = (r - l) / 2.0;
            let idle = if it.id == HELP { p.footer } else { p.bg };
            cv.circle(cx, cy, rad, p.tile_bd);
            cv.circle(cx, cy, rad - s.max(1.0), if hot || down { p.hover } else { idle });
        }
        Kind::Pill => {
            let c = if down {
                mix(p.acc, 0, 0.2)
            } else if hot {
                mix(p.acc, 0x00FF_FFFF, 0.15)
            } else {
                p.acc
            };
            cv.rrect(l, t, r, b, (b - t) / 2.0, c);
        }
        Kind::Chip => {
            let sel = (it.id == CHIP_FROM && EDIT.load(Relaxed) == 0) || (it.id == CHIP_TO && EDIT.load(Relaxed) == 1);
            let fill = if hot { p.tile_hover } else { p.tile };
            if sel {
                cv.rrect(l, t, r, b, 6.0 * s, p.acc);
                cv.rrect(l + 1.5 * s, t + 1.5 * s, r - 1.5 * s, b - 1.5 * s, 5.0 * s, fill);
            } else {
                cv.rrect(l, t, r, b, 6.0 * s, fill);
            }
        }
        Kind::Picker => {
            let g = picker_geom(&it.r, get(K_CLOCK24) != 0);
            cv.rrect(l, t, r, b, 6.0 * s, p.tile);
            let total = g.n * g.cw + (g.n - 1) * g.gap;
            let by = (g.top + 2 * g.rh) as f32;
            cv.rrect((g.x0 - sc(8)) as f32, by, (g.x0 + total + sc(8)) as f32, by + g.rh as f32, 4.0 * s, p.acc);
        }
    }
}

unsafe fn picker_rows(mdc: HDC, p: &Pal, g: &PickGeom, c24: bool, inband: bool, font: isize) {
    let mins = get(if EDIT.load(Relaxed) == 0 { K_SCHED_START } else { K_SCHED_END });
    let (hh, mn) = (mins / 60, mins % 60);
    let flags = DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_CENTER;
    for col in 0..g.n {
        let fr = SCROLL[col as usize].load(Relaxed) as f32 / 1000.0;
        for off in -3..=3i32 {
            let txt = match col {
                0 => {
                    let v = (hh + off).rem_euclid(24);
                    if c24 {
                        format!("{v:02}")
                    } else {
                        format!("{}", if v % 12 == 0 { 12 } else { v % 12 })
                    }
                }
                1 => format!("{:02}", (mn + off).rem_euclid(60)),
                _ => match off {
                    0 => ampm(hh).to_string(),
                    -1 | 1 => ampm((hh + 12) % 24).to_string(),
                    _ => continue,
                },
            };
            let x = g.x0 + col * (g.cw + g.gap);
            let y = g.top + ((off as f32 + 2.0 + fr) * g.rh as f32) as i32;
            let c = if inband {
                p.acc_fg
            } else if (off as f32 + fr).abs() < 1.5 {
                p.fg
            } else {
                p.dim
            };
            tx(mdc, &txt, [x, y, x + g.cw, y + g.rh], font, c, flags);
        }
    }
}

unsafe fn draw_page(mdc: HDC, cv: &Canvas, page: i32) {
    let p = pal();
    let s = DPI.load(Relaxed) as f32 / 96.0;
    let (w, h) = (cv.w, cv.h);
    let m = sc(M);
    cv.fill(p.bg);
    if page == P_MAIN {
        cv.rect(0, sc(FOOT), w, h, p.footer);
        cv.rect(m, sc(98), w - m, sc(98) + sc(1).max(1), p.div);
    }
    cv.border(p.border);
    let (hover, press) = (HOVER.load(Relaxed), PRESS.load(Relaxed));
    let its = items(page);
    for it in &its {
        shape(cv, &p, it, s, hover, press);
    }
    let _ = GdiFlush();

    SetBkMode(mdc, TRANSPARENT);
    let (fnt, fb, fs) = (FN.load(Relaxed), FB.load(Relaxed), FS.load(Relaxed));
    let (fis, fim) = (FI_S.load(Relaxed), FI_M.load(Relaxed));
    let vcs = DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX;
    let c24 = get(K_CLOCK24) != 0;

    match page {
        P_MAIN => {
            let v = get(K_VOLUME).clamp(0, 100);
            tx(mdc, "System sounds volume", [m, sc(18), w - m - sc(70), sc(46)], fnt, p.fg, vcs | DT_END_ELLIPSIS);
            tx(mdc, &format!("{v}%"), [w - m - sc(70), sc(18), w - m, sc(46)], fb, p.fg, vcs | DT_RIGHT);
            let g = if v == 0 {
                G_MUTE
            } else if v < 34 {
                G_VOL1
            } else if v < 67 {
                G_VOL2
            } else {
                G_VOL3
            };
            gl(cv, mdc, g, [m, sc(52), m + sc(30), sc(84)], fim, p.fg);
            let lim = its.iter().find(|i| i.id == UPD).map_or(w - m - sc(30), |i| i.r[0]) - sc(8);
            tx(
                mdc,
                &format!("Blocked this session: {}", get(K_RESETS)),
                [m, sc(FOOT), lim, h],
                fs,
                p.dim,
                vcs | DT_END_ELLIPSIS,
            );
        }
        P_ABOUT => {
            tx(mdc, "About", [sc(62), sc(10), w - m, sc(42)], fb, p.fg, vcs);
            tx(mdc, "Bark \u{2013} System Volume Fix", [m, sc(52), w - m, sc(76)], fb, p.fg, vcs);
            tx(mdc, ABOUT_TEXT, [m, sc(80), w - m, sc(298)], fs, p.fg, DT_WORDBREAK | DT_NOPREFIX);
            tx(
                mdc,
                &format!("Version {APP_VERSION} \u{00B7} GitHub \u{00B7} \u{00A9} Bonanto, 2026 \u{00B7} GPLv3"),
                [m, sc(300), w - m, sc(320)],
                fs,
                p.dim,
                vcs,
            );
        }
        P_SCHED => tx(mdc, "Quiet schedule", [sc(62), sc(10), w - m, sc(42)], fb, p.fg, vcs),
        _ => tx(mdc, "Settings", [sc(62), sc(10), w - m, sc(42)], fb, p.fg, vcs),
    }

    if page == P_SET {
        tx(
            mdc,
            "While the tray icon is hidden, it appears only when Bark is open. To open Bark again, start it from the Start menu or run bark.exe.",
            [m, sc(228), w - m, sc(298)],
            fs,
            p.dim,
            DT_WORDBREAK | DT_NOPREFIX,
        );
    }

    for it in &its {
        let [l, t, r, b] = it.r;
        match it.kind {
            Kind::Row => {
                gl(cv, mdc, it.glyph, [l, t, l + sc(24), b], fim, p.fg);
                tx(mdc, it.label, [l + sc(32), t, r - sc(48), b], fnt, p.fg, vcs | DT_END_ELLIPSIS);
            }
            Kind::Tile | Kind::TileL | Kind::TileR => {
                let col = if tile_on(it.id) { p.acc_fg } else { p.fg };
                gl(cv, mdc, it.glyph, it.r, fim, col);
                if it.kind != Kind::TileR {
                    let rr = if it.kind == Kind::TileL { r + (r - l) } else { r };
                    tx(mdc, it.label, [l, b + sc(6), rr, b + sc(24)], fs, p.fg, vcs | DT_CENTER | DT_END_ELLIPSIS);
                }
            }
            Kind::TileNav => {
                let cx = (l + r) / 2;
                gl(cv, mdc, it.glyph, [cx - sc(16), t, cx, b], fim, p.fg);
                gl(cv, mdc, G_CHEVRON, [cx + sc(4), t, cx + sc(16), b], fis, p.fg);
                tx(mdc, it.label, [l, b + sc(6), r, b + sc(24)], fs, p.fg, vcs | DT_CENTER | DT_END_ELLIPSIS);
            }
            Kind::Round => gl(cv, mdc, it.glyph, it.r, fim, p.fg),
            Kind::Pill => tx(mdc, it.label, it.r, fnt, p.acc_fg, vcs | DT_CENTER),
            Kind::Chip => {
                let mins = get(if it.id == CHIP_FROM { K_SCHED_START } else { K_SCHED_END });
                tx(mdc, it.label, [l + sc(12), t + sc(5), r, t + sc(21)], fs, p.dim, DT_SINGLELINE | DT_NOPREFIX);
                tx(mdc, &fmt_time(mins, c24), [l + sc(12), t + sc(20), r, b - sc(3)], fb, p.fg, vcs);
            }
            Kind::Picker => {
                let g = picker_geom(&it.r, c24);
                let (bt, bb) = (g.top + 2 * g.rh, g.top + 3 * g.rh);
                for (clip, inband) in [([l, t, r, bt], false), ([l, bb, r, b], false), ([l, bt, r, bb], true)] {
                    let saved = SaveDC(mdc);
                    let _ = IntersectClipRect(mdc, clip[0], clip[1], clip[2], clip[3]);
                    picker_rows(mdc, &p, &g, c24, inband, fnt);
                    let _ = RestoreDC(mdc, saved);
                }
            }
            Kind::Slider => {}
        }
    }
    let _ = GdiFlush();
}

pub unsafe fn render(hdc: HDC, page: i32) -> Option<Surf> {
    let (w, ht) = (sc(W0), sc(H0));
    let mut bi = BITMAPINFO::default();
    bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = w;
    bi.bmiHeader.biHeight = -ht;
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    let mut bits: *mut c_void = std::ptr::null_mut();
    let bmp = CreateDIBSection(Some(hdc), &bi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
    if bits.is_null() {
        let _ = DeleteObject(bmp.into());
        return None;
    }
    let mdc = CreateCompatibleDC(Some(hdc));
    let old = SelectObject(mdc, bmp.into());
    let cv = Canvas { p: bits as *mut u32, w, h: ht };
    draw_page(mdc, &cv, page);
    Some(Surf { dc: mdc.0 as isize, bmp: bmp.0 as isize, old: old.0 as isize })
}

pub unsafe fn blit(hdc: HDC, s: &Surf, x: i32) {
    let _ = BitBlt(hdc, x, 0, sc(W0), sc(H0), Some(HDC(s.dc as *mut c_void)), 0, 0, SRCCOPY);
}

pub unsafe fn free(s: Surf) {
    let dc = HDC(s.dc as *mut c_void);
    SelectObject(dc, HGDIOBJ(s.old as *mut c_void));
    let _ = DeleteDC(dc);
    let _ = DeleteObject(HGDIOBJ(s.bmp as *mut c_void));
}
