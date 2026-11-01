use crate::gfx::{mix, rgb};
use std::{
    ffi::c_void,
    sync::{
        atomic::{AtomicI32, AtomicIsize, AtomicU64, Ordering::Relaxed},
        Mutex,
    },
};
use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};

pub const W0: i32 = 360;
pub const H0: i32 = 364;
pub const M: i32 = 20;
pub const FOOT: i32 = 316;

pub const P_MAIN: i32 = 0;
pub const P_SCHED: i32 = 1;
pub const P_SET: i32 = 2;
pub const P_ABOUT: i32 = 3;

pub static DPI: AtomicI32 = AtomicI32::new(96);
pub static PAGE: AtomicI32 = AtomicI32::new(0);
pub static HOVER: AtomicI32 = AtomicI32::new(0);
pub static PRESS: AtomicI32 = AtomicI32::new(0);
pub static DARK: AtomicI32 = AtomicI32::new(0);
pub static EDIT: AtomicI32 = AtomicI32::new(0);
pub static ANIMATING: AtomicI32 = AtomicI32::new(0);
pub static FN: AtomicIsize = AtomicIsize::new(0);
pub static FB: AtomicIsize = AtomicIsize::new(0);
pub static FS: AtomicIsize = AtomicIsize::new(0);
pub static FI_S: AtomicIsize = AtomicIsize::new(0);
pub static FI_M: AtomicIsize = AtomicIsize::new(0);
pub static TIPW: AtomicIsize = AtomicIsize::new(0);
pub static TIP_TEXT: Mutex<String> = Mutex::new(String::new());
pub static SNAP: AtomicU64 = AtomicU64::new(0);
pub static WACC: AtomicI32 = AtomicI32::new(0);
// System accent shades read at show time: [Light3, Light2, Light1, Dark1] as 0x00RRGGBB, -1 = not available.
pub static ACC: [AtomicI32; 4] = [AtomicI32::new(-1), AtomicI32::new(-1), AtomicI32::new(-1), AtomicI32::new(-1)];

fn acc(i: usize, default: u32) -> u32 {
    let v = ACC[i].load(Relaxed);
    if v >= 0 {
        v as u32
    } else {
        default
    }
}
// Picker glide offsets in 1/1000 rows, one per column.
pub static SCROLL: [AtomicI32; 3] = [AtomicI32::new(0), AtomicI32::new(0), AtomicI32::new(0)];

pub fn sc(v: i32) -> i32 {
    v * DPI.load(Relaxed) / 96
}

pub struct Pal {
    pub bg: u32,
    pub footer: u32,
    pub fg: u32,
    pub dim: u32,
    pub acc: u32,
    pub acc_fg: u32,
    pub track: u32,
    pub border: u32,
    pub hover: u32,
    pub card: u32,
    pub tile: u32,
    pub tile_hover: u32,
    pub tile_press: u32,
    pub acc_hover: u32,
    pub acc_press: u32,
    pub edge: u32,
    pub tile_bd: u32,
    pub div: u32,
    pub tip: u32,
}

pub fn pal() -> Pal {
    let light_bg = rgb(243, 243, 243);
    let light_acc = acc(3, rgb(0, 103, 192));
    if DARK.load(Relaxed) != 0 {
        Pal {
            bg: rgb(44, 44, 44),
            footer: rgb(32, 32, 32),
            fg: rgb(255, 255, 255),
            dim: rgb(165, 165, 165),
            acc: acc(1, rgb(76, 194, 255)),
            acc_fg: rgb(0, 0, 0),
            track: rgb(110, 110, 110),
            border: rgb(72, 72, 72),
            hover: rgb(55, 55, 55),
            card: rgb(69, 69, 69),
            tile: rgb(60, 60, 60),
            tile_hover: rgb(70, 70, 70),
            tile_press: rgb(52, 52, 52),
            acc_hover: acc(0, rgb(153, 235, 255)),
            acc_press: acc(2, rgb(0, 145, 248)),
            edge: rgb(255, 255, 255),
            tile_bd: rgb(68, 68, 68),
            div: rgb(62, 62, 62),
            tip: rgb(44, 44, 44),
        }
    } else {
        Pal {
            bg: light_bg,
            footer: rgb(232, 232, 232),
            fg: rgb(26, 26, 26),
            dim: rgb(96, 96, 96),
            acc: light_acc,
            acc_fg: rgb(255, 255, 255),
            track: rgb(168, 168, 168),
            border: rgb(208, 208, 208),
            hover: rgb(231, 231, 231),
            card: rgb(255, 255, 255),
            tile: rgb(251, 251, 251),
            tile_hover: rgb(244, 244, 244),
            tile_press: rgb(240, 240, 240),
            acc_hover: mix(light_acc, light_bg, 0.10),
            acc_press: mix(light_acc, light_bg, 0.20),
            edge: rgb(0, 0, 0),
            tile_bd: rgb(229, 229, 229),
            div: rgb(220, 220, 220),
            tip: rgb(255, 255, 255),
        }
    }
}

pub fn bgr(c: u32) -> COLORREF {
    COLORREF(((c & 255) << 16) | (c & 0xFF00) | ((c >> 16) & 255))
}

pub unsafe fn work() -> RECT {
    let mut wa = RECT::default();
    let _ = SystemParametersInfoW(
        SPI_GETWORKAREA,
        0,
        Some(&mut wa as *mut RECT as *mut c_void),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    );
    wa
}
