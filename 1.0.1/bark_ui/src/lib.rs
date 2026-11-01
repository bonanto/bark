mod draw;
mod ffi;
mod font;
mod gfx;
mod icons;
mod input;
mod layout;
mod nav;
mod state;
mod tip;

use std::sync::OnceLock;

const APP_VERSION: &str = "1.0.1";

const T_TIP: usize = 1;
const T_REFRESH: usize = 2;
const T_NAV: usize = 3;
const T_OPEN: usize = 4;
const T_SCROLL: usize = 5;
const T_CLOSE: usize = 6;

// Posted to the host window when the close animation has finished (host: WM_APP + 3).
const WM_CLOSED: u32 = 0x8003;

mod keys {
    pub const K_VOLUME: u32 = 0;
    pub const K_LOCK: u32 = 1;
    pub const K_QUIET_MANUAL: u32 = 2;
    pub const K_SCHED_ON: u32 = 3;
    pub const K_SCHED_START: u32 = 4;
    pub const K_SCHED_END: u32 = 5;
    pub const K_RESETS: u32 = 7;
    pub const K_AUTOSTART: u32 = 9;
    pub const K_DING: u32 = 10;
    pub const K_UPDATE_AVAIL: u32 = 11;
    pub const K_SYS_DARK: u32 = 12;
    pub const K_UPDATE_CHECK: u32 = 13;
    pub const K_HIDE_TRAY: u32 = 14;
    pub const K_CLOCK24: u32 = 15;
    pub const K_SYS_ICON: u32 = 17;
    pub const K_ACC_L3: u32 = 18;
    pub const K_ACC_L2: u32 = 19;
    pub const K_ACC_L1: u32 = 20;
    pub const K_ACC_D1: u32 = 21;
}

struct Api {
    get: extern "C" fn(u32) -> i32,
    set: extern "C" fn(u32, i32),
}

static API: OnceLock<Api> = OnceLock::new();

fn init(get: extern "C" fn(u32) -> i32, set: extern "C" fn(u32, i32)) {
    let _ = API.set(Api { get, set });
}

pub(crate) fn get(k: u32) -> i32 {
    API.get().map_or(0, |a| (a.get)(k))
}

pub(crate) fn set(k: u32, v: i32) {
    if let Some(a) = API.get() {
        (a.set)(k, v);
    }
}
