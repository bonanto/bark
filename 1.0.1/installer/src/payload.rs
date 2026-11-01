// The embedded, compressed Bark files (made by build.rs).
use std::{mem::transmute, sync::OnceLock};
use windows::{
    core::{s, w, BOOL},
    Win32::{Foundation::*, System::LibraryLoader::*, System::Threading::*},
};

static BLOB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.bin"));

pub struct Entry {
    pub arch: u8, // 0 x86, 1 x64, 2 arm64, 255 common
    pub name: String,
    pub data: Vec<u8>,
}

fn take<'a>(raw: &'a [u8], p: &mut usize, n: usize) -> Option<&'a [u8]> {
    let s = raw.get(*p..p.checked_add(n)?)?;
    *p += n;
    Some(s)
}

fn load() -> Vec<Entry> {
    let raw = miniz_oxide::inflate::decompress_to_vec(BLOB).unwrap_or_default();
    let mut v = Vec::new();
    let mut p = 0usize;
    let Some(c) = take(&raw, &mut p, 4) else { return v };
    let count = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
    for _ in 0..count {
        let Some(a) = take(&raw, &mut p, 1).map(|b| b[0]) else { break };
        let Some(l) = take(&raw, &mut p, 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize) else { break };
        let Some(name) = take(&raw, &mut p, l).map(|b| String::from_utf8_lossy(b).into_owned()) else { break };
        let Some(dl) = take(&raw, &mut p, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize) else { break };
        let Some(data) = take(&raw, &mut p, dl).map(|b| b.to_vec()) else { break };
        v.push(Entry { arch: a, name, data });
    }
    v
}

/// The architecture of Windows itself (the installer is a 32-bit program that also runs on x64 and ARM64).
pub fn native_arch() -> u8 {
    unsafe {
        let Ok(k) = GetModuleHandleW(w!("kernel32.dll")) else { return 0 };
        if let Some(p) = GetProcAddress(k, s!("IsWow64Process2")) {
            type F = unsafe extern "system" fn(HANDLE, *mut u16, *mut u16) -> BOOL;
            let f: F = transmute(p);
            let (mut pm, mut nm) = (0u16, 0u16);
            if f(GetCurrentProcess(), &mut pm, &mut nm).as_bool() {
                return match nm {
                    0xAA64 => 2,
                    0x8664 => 1,
                    _ => 0,
                };
            }
        }
        let mut wow = BOOL(0);
        if IsWow64Process(GetCurrentProcess(), &mut wow).is_ok() && wow.as_bool() {
            return 1;
        }
        0
    }
}

static ALL: OnceLock<Vec<Entry>> = OnceLock::new();

/// Files to install on this PC: the best matching architecture plus the common files.
/// Order of preference: arm64 -> x64 -> x86 (Windows on ARM can run x64 and x86 programs, but not the other way round).
pub fn files_for_this_pc() -> Result<Vec<&'static Entry>, String> {
    let all = ALL.get_or_init(load);
    let prefs: &[u8] = match native_arch() {
        2 => &[2, 1, 0],
        1 => &[1, 0],
        _ => &[0],
    };
    for &a in prefs {
        let set: Vec<&Entry> = all.iter().filter(|e| e.arch == a || e.arch == 255).collect();
        if set.iter().any(|e| e.arch == a) {
            return Ok(set);
        }
    }
    Err("This installer does not contain Bark for this version of Windows.".to_string())
}
