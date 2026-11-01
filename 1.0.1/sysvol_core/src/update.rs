use crate::cfg::*;
use std::{
    ffi::c_void,
    mem::transmute,
    ptr::{null, null_mut},
};
use windows::{
    core::{s, w, PCWSTR},
    Win32::{
        Foundation::{FreeLibrary, HMODULE},
        System::LibraryLoader::*,
    },
};

pub const VERSION: &str = "1.0.1";

type H = *mut c_void;
type FOpen = unsafe extern "system" fn(PCWSTR, u32, PCWSTR, PCWSTR, u32) -> H;
type FConnect = unsafe extern "system" fn(H, PCWSTR, u16, u32) -> H;
type FOpenReq = unsafe extern "system" fn(H, PCWSTR, PCWSTR, PCWSTR, PCWSTR, *const PCWSTR, u32) -> H;
type FTime = unsafe extern "system" fn(H, i32, i32, i32, i32) -> i32;
type FSend = unsafe extern "system" fn(H, PCWSTR, u32, *const c_void, u32, u32, usize) -> i32;
type FRecv = unsafe extern "system" fn(H, *mut c_void) -> i32;
type FSetOpt = unsafe extern "system" fn(H, u32, *const c_void, u32) -> i32;
type FQuery = unsafe extern "system" fn(H, u32, PCWSTR, *mut c_void, *mut u32, *mut u32) -> i32;
type FClose = unsafe extern "system" fn(H) -> i32;

struct Hd(H, FClose);
impl Drop for Hd {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                (self.1)(self.0);
            }
        }
    }
}

// winhttp.dll is loaded only for the check and freed right after (keeps idle memory low).
unsafe fn get_tag(lib: HMODULE) -> Option<String> {
    macro_rules! f {
        ($n:literal) => {
            transmute(GetProcAddress(lib, s!($n))?)
        };
    }
    let open: FOpen = f!("WinHttpOpen");
    let connect: FConnect = f!("WinHttpConnect");
    let openreq: FOpenReq = f!("WinHttpOpenRequest");
    let timeouts: FTime = f!("WinHttpSetTimeouts");
    let send: FSend = f!("WinHttpSendRequest");
    let recv: FRecv = f!("WinHttpReceiveResponse");
    let setopt: FSetOpt = f!("WinHttpSetOption");
    let query: FQuery = f!("WinHttpQueryHeaders");
    let close: FClose = f!("WinHttpCloseHandle");

    let ses = Hd(open(w!("Bark-UpdateCheck"), 4, PCWSTR::null(), PCWSTR::null(), 0), close);
    if ses.0.is_null() {
        return None;
    }
    timeouts(ses.0, 8000, 8000, 8000, 8000);
    let con = Hd(connect(ses.0, w!("github.com"), 443, 0), close);
    if con.0.is_null() {
        return None;
    }
    let req = Hd(
        openreq(
            con.0,
            w!("GET"),
            w!("/bonanto/bark/releases/latest"),
            PCWSTR::null(),
            PCWSTR::null(),
            null(),
            0x0080_0000, // WINHTTP_FLAG_SECURE
        ),
        close,
    );
    if req.0.is_null() {
        return None;
    }
    let never = 0u32; // WINHTTP_DISABLE_REDIRECTS
    if setopt(req.0, 88, &never as *const u32 as *const c_void, 4) == 0 {
        return None;
    }
    if send(req.0, PCWSTR::null(), 0, null(), 0, 0, 0) == 0 || recv(req.0, null_mut()) == 0 {
        return None;
    }
    let mut buf = [0u16; 512];
    let mut len = (buf.len() * 2) as u32;
    // 33 = WINHTTP_QUERY_LOCATION
    if query(req.0, 33, PCWSTR::null(), buf.as_mut_ptr() as *mut c_void, &mut len, null_mut()) == 0 {
        return None;
    }
    let loc = String::from_utf16_lossy(&buf[..(len as usize / 2).min(buf.len())]);
    let (_, tag) = loc.split_once("/releases/tag/")?;
    let tag = tag.trim().trim_end_matches('/');
    let ok = !tag.is_empty()
        && tag.len() <= 32
        && tag.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'+'));
    ok.then(|| tag.to_string())
}

fn nums(s: &str) -> Vec<u32> {
    let core = s
        .trim()
        .trim_start_matches(['v', 'V'])
        .split(['-', '+'])
        .next()
        .unwrap_or("");
    core.split('.').map(|p| p.trim().parse().unwrap_or(0)).collect()
}

fn is_newer(latest: &str, cur: &str) -> bool {
    let (a, b) = (nums(latest), nums(cur));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (*a.get(i).unwrap_or(&0), *b.get(i).unwrap_or(&0));
        if x != y {
            return x > y;
        }
    }
    false
}

pub fn check() {
    if get(K_UPDATE_CHECK) == 0 {
        return;
    }
    let tag = unsafe {
        let Ok(lib) = LoadLibraryExW(w!("winhttp.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32) else {
            return;
        };
        let t = get_tag(lib);
        let _ = FreeLibrary(lib);
        t
    };
    let Some(tag) = tag else {
        lg!("update check failed");
        return;
    };
    let newer = is_newer(&tag, VERSION);
    put(K_UPDATE_AVAIL, newer as i32);
    lg!("update check: latest {tag}, current {VERSION}, newer={newer}");
}
