use crate::reg::Key;
use std::sync::atomic::{AtomicI32, Ordering::Relaxed};

pub const ROOT: &str = r"Software\{7C1E4B52-9A3D-4F68-B0E2-5D81A6C3F974}";

pub const K_VOLUME: u32 = 0;
pub const K_LOCK: u32 = 1;
pub const K_QUIET_MANUAL: u32 = 2;
pub const K_SCHED_ON: u32 = 3;
pub const K_SCHED_START: u32 = 4; // minutes since midnight
pub const K_SCHED_END: u32 = 5;
pub const K_STEP: u32 = 6;
pub const K_RESETS: u32 = 7; // read-only, per run
pub const K_QUIET_ACTIVE: u32 = 8; // read-only
pub const K_AUTOSTART: u32 = 9; // virtual, Run key
pub const K_DING: u32 = 10; // silence volume-change ding
pub const K_UPDATE_AVAIL: u32 = 11; // read-only
pub const K_SYS_DARK: u32 = 12; // virtual, read-only
pub const K_UPDATE_CHECK: u32 = 13; // registry only

const N: usize = 14;
static CFG: [AtomicI32; N] = [
    AtomicI32::new(-1),
    AtomicI32::new(1),
    AtomicI32::new(0),
    AtomicI32::new(0),
    AtomicI32::new(1320),
    AtomicI32::new(420),
    AtomicI32::new(5),
    AtomicI32::new(0),
    AtomicI32::new(0),
    AtomicI32::new(0),
    AtomicI32::new(1),
    AtomicI32::new(0),
    AtomicI32::new(0),
    AtomicI32::new(1),
];
const NAMES: [&str; N] = [
    "Volume",
    "Lock",
    "QuietManual",
    "QuietSchedule",
    "QuietStart",
    "QuietEnd",
    "HotkeyStep",
    "",
    "QuietActive",
    "",
    "DingMute",
    "",
    "",
    "UpdateCheck",
];

pub fn get(k: u32) -> i32 {
    CFG.get(k as usize).map_or(0, |a| a.load(Relaxed))
}

pub fn put(k: u32, v: i32) {
    if let Some(a) = CFG.get(k as usize) {
        a.store(v, Relaxed);
    }
}

pub fn inc(k: u32) {
    if let Some(a) = CFG.get(k as usize) {
        a.fetch_add(1, Relaxed);
    }
}

pub fn load() {
    if let Some(key) = Key::open(ROOT, false) {
        for (i, n) in NAMES.iter().enumerate() {
            if n.is_empty() {
                continue;
            }
            if let Some(v) = key.dword(n) {
                CFG[i].store(v as i32, Relaxed);
            }
        }
    }
}

pub fn save(k: u32) {
    let n = NAMES.get(k as usize).copied().unwrap_or("");
    if n.is_empty() {
        return;
    }
    if let Some(key) = Key::create(ROOT) {
        key.set_dword(n, get(k) as u32);
    }
}