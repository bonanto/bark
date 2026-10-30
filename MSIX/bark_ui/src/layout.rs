use crate::icons::*;
use crate::keys::*;
use crate::state::*;
use std::sync::atomic::Ordering::Relaxed;

pub const SLIDER: i32 = 1;
pub const LOCK: i32 = 2;
pub const DING: i32 = 3;
pub const QUIET: i32 = 4;
pub const SCHED_T: i32 = 5;
pub const SCHED_GO: i32 = 6;
pub const SETT_GO: i32 = 7;
pub const HELP: i32 = 8;
pub const BACK: i32 = 9;
pub const LEARN: i32 = 11;
pub const CHIP_FROM: i32 = 12;
pub const CHIP_TO: i32 = 13;
pub const PICKER: i32 = 14;
pub const SCHED_ROW: i32 = 15;
pub const CLOCK24: i32 = 16;
pub const AUTO: i32 = 17;
pub const HIDE: i32 = 18;
pub const BRAND: i32 = 20;

pub const SL_L: i32 = M + 30;

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Slider,
    Row,
    Tile,
    TileL,
    TileR,
    TileNav,
    Round,
    Pill,
    Chip,
    Picker,
}

#[derive(Clone, Copy)]
pub struct Item {
    pub id: i32,
    pub kind: Kind,
    pub r: [i32; 4],
    pub label: &'static str,
    pub tip: &'static str,
    pub glyph: u16,
}

pub struct PickGeom {
    pub x0: i32,
    pub cw: i32,
    pub gap: i32,
    pub n: i32,
    pub rh: i32,
    pub top: i32,
}

pub fn picker_geom(r: &[i32; 4], c24: bool) -> PickGeom {
    let n = if c24 { 2 } else { 3 };
    let (cw, gap) = (sc(72), sc(8));
    let total = n * cw + (n - 1) * gap;
    PickGeom { x0: (sc(W0) - total) / 2, cw, gap, n, rh: sc(28), top: r[1] }
}

pub fn slider_x() -> (i32, i32) {
    (sc(SL_L + 10), sc(W0 - M - 10))
}

pub fn toggle_key(id: i32) -> Option<u32> {
    match id {
        LOCK => Some(K_LOCK),
        DING => Some(K_DING),
        QUIET => Some(K_QUIET_MANUAL),
        SCHED_T | SCHED_ROW => Some(K_SCHED_ON),
        CLOCK24 => Some(K_CLOCK24),
        AUTO => Some(K_AUTOSTART),
        HIDE => Some(K_HIDE_TRAY),
        BRAND => Some(K_SYS_ICON),
        _ => None,
    }
}

fn row(id: i32, y: i32, label: &'static str, tip: &'static str, glyph: u16) -> Item {
    let (w, m) = (sc(W0), sc(M));
    Item { id, kind: Kind::Row, r: [m, sc(y), w - m, sc(y + 40)], label, tip, glyph }
}

pub fn items(page: i32) -> Vec<Item> {
    let w = sc(W0);
    let m = sc(M);
    let mut v = Vec::with_capacity(12);
    if page != P_MAIN {
        v.push(Item {
            id: BACK,
            kind: Kind::Round,
            r: [m, sc(10), m + sc(32), sc(42)],
            label: "",
            tip: "Back",
            glyph: G_BACK,
        });
    }
    match page {
        P_MAIN => {
            v.push(Item {
                id: SLIDER,
                kind: Kind::Slider,
                r: [sc(SL_L), sc(52), w - m, sc(84)],
                label: "",
                tip: "System sounds volume. Drag, click, use the mouse wheel or arrow keys.",
                glyph: 0,
            });
            v.push(row(
                LOCK,
                108,
                "Lock volume",
                "Keep the System sounds volume at your value. Outside changes are reverted.",
                G_LOCK,
            ));
            v.push(row(
                DING,
                148,
                "Silence volume-change ding",
                "Mute annoying Volume-Change Ding sound. Media playback sounds unaffected. May chop notification sound only when you change Volume during a notification is shown - which is rare!",
                G_DING,
            ));
            let gap = sc(12);
            let tw = ((w - 2 * m - 2 * gap) / 3) & !1;
            let (ty, tb) = (sc(206), sc(254));
            let x1 = m + tw + gap;
            let x2 = x1 + tw + gap;
            v.push(Item {
                id: QUIET,
                kind: Kind::Tile,
                r: [m, ty, m + tw, tb],
                label: "Quiet mode",
                tip: "Turns off Windows System Sounds. This mutes Volume Ding as well. Media playback unaffected",
                glyph: G_MUTE,
            });
            v.push(Item {
                id: SCHED_T,
                kind: Kind::TileL,
                r: [x1, ty, x1 + tw / 2, tb],
                label: "Quiet schedule",
                tip: "Turn Quiet mode on automatically during the scheduled hours.",
                glyph: G_CLOCK,
            });
            v.push(Item {
                id: SCHED_GO,
                kind: Kind::TileR,
                r: [x1 + tw / 2, ty, x1 + tw, tb],
                label: "",
                tip: "Open the quiet schedule.",
                glyph: G_CHEVRON,
            });
            v.push(Item {
                id: SETT_GO,
                kind: Kind::TileNav,
                r: [x2, ty, x2 + tw, tb],
                label: "Settings",
                tip: "Open settings.",
                glyph: G_SETTINGS,
            });
            let (hy0, hy1) = (sc(FOOT + 9), sc(FOOT + 39));
            v.push(Item {
                id: HELP,
                kind: Kind::Round,
                r: [w - m - sc(30), hy0, w - m, hy1],
                label: "",
                tip: "About Bark",
                glyph: G_HELP,
            });
        }
        P_SCHED => {
            v.push(row(
                SCHED_ROW,
                54,
                "Quiet schedule",
                "Turn Quiet mode on automatically between the two times.",
                G_CLOCK,
            ));
            let cw = (w - 2 * m - sc(8)) / 2;
            v.push(Item {
                id: CHIP_FROM,
                kind: Kind::Chip,
                r: [m, sc(106), m + cw, sc(154)],
                label: "From",
                tip: "Choose the start time.",
                glyph: 0,
            });
            v.push(Item {
                id: CHIP_TO,
                kind: Kind::Chip,
                r: [w - m - cw, sc(106), w - m, sc(154)],
                label: "To",
                tip: "Choose the end time.",
                glyph: 0,
            });
            v.push(Item {
                id: PICKER,
                kind: Kind::Picker,
                r: [m, sc(166), w - m, sc(166) + 5 * sc(28)],
                label: "",
                tip: "",
                glyph: 0,
            });
            v.push(row(CLOCK24, 314, "24-hour clock", "Show times in 24-hour format.", G_CLOCK));
        }
        P_SET => {
            v.push(row(AUTO, 54, "Start with Windows", "Launch Bark automatically when you sign in.", G_PLAY));
            v.push(row(
                HIDE,
                94,
                "Hide tray icon",
                "The tray icon appears only while Bark is open.",
                G_HIDE,
            ));
            v.push(row(
                BRAND,
                134,
                "Use system icon",
                "Show the system-style icon in the visible tray. When off, the Bark icon stays in the hidden-icons area.",
                G_PIN,
            ));
        }
        _ => {
            v.push(Item {
                id: LEARN,
                kind: Kind::Pill,
                r: [m, sc(326), m + sc(110), sc(352)],
                label: "Learn More",
                tip: "Open the Bark project page on GitHub.",
                glyph: 0,
            });
        }
    }
    v
}

pub fn hit(x: i32, y: i32) -> Option<Item> {
    if ANIMATING.load(Relaxed) != 0 {
        return None;
    }
    items(PAGE.load(Relaxed))
        .into_iter()
        .rev()
        .find(|i| x >= i.r[0] && x < i.r[2] && y >= i.r[1] && y < i.r[3])
}
