use crate::{font, input, nav, tip};
use std::ffi::c_void;
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{HINSTANCE, HMODULE, HWND},
        Graphics::Gdi::HDC,
        System::LibraryLoader::{
            GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        },
        UI::WindowsAndMessaging::{LoadImageW, IMAGE_ICON, LR_DEFAULTCOLOR},
    },
};

fn hw(h: isize) -> HWND {
    HWND(h as *mut c_void)
}

#[no_mangle]
pub extern "C" fn bui_init(get: extern "C" fn(u32) -> i32, set: extern "C" fn(u32, i32)) {
    crate::init(get, set);
}

#[no_mangle]
pub extern "C" fn bui_on_show(h: isize) {
    unsafe { nav::on_show(hw(h)) }
}

#[no_mangle]
pub extern "C" fn bui_begin_hide(h: isize) -> i32 {
    unsafe { nav::begin_hide(hw(h)) as i32 }
}

#[no_mangle]
pub extern "C" fn bui_paint(h: isize, dc: isize) {
    unsafe { nav::paint(hw(h), HDC(dc as *mut c_void)) }
}

#[no_mangle]
pub extern "C" fn bui_timer(h: isize, id: usize) {
    unsafe { nav::timer(hw(h), id) }
}

#[no_mangle]
pub extern "C" fn bui_mouse_move(h: isize, x: i32, y: i32) {
    unsafe { input::mouse_move(hw(h), x, y) }
}

#[no_mangle]
pub extern "C" fn bui_mouse_leave(h: isize) {
    unsafe { input::mouse_leave(hw(h)) }
}

#[no_mangle]
pub extern "C" fn bui_mouse_down(h: isize, x: i32, y: i32) {
    unsafe { input::mouse_down(hw(h), x, y) }
}

#[no_mangle]
pub extern "C" fn bui_mouse_up(h: isize, x: i32, y: i32) {
    unsafe { input::mouse_up(hw(h), x, y) }
}

#[no_mangle]
pub extern "C" fn bui_capture_lost() {
    unsafe { input::capture_lost() }
}

#[no_mangle]
pub extern "C" fn bui_wheel(h: isize, x: i32, y: i32, delta: i32) {
    unsafe { input::wheel(hw(h), x, y, delta) }
}

#[no_mangle]
pub extern "C" fn bui_key(h: isize, vk: u32) {
    unsafe { input::key(hw(h), vk) }
}

#[no_mangle]
pub extern "C" fn bui_set_tip(h: isize) {
    tip::set_tip(hw(h));
}

/// Address of the tooltip window procedure (stdcall), as an integer.
#[no_mangle]
pub extern "C" fn bui_tip_wndproc() -> usize {
    tip::tip_proc as *const () as usize
}

/// Raw HFONT of the best available icon font. The caller deletes it.
#[no_mangle]
pub extern "C" fn bui_icon_font(px: i32) -> isize {
    unsafe { font::icon_font(px) }
}

/// Brand tray icon (resource id 1 of this DLL, from tray.ico) at `px` x `px`. The caller destroys it. 0 = failed.
#[no_mangle]
pub extern "C" fn bui_tray_icon(px: i32) -> isize {
    unsafe {
        let mut hm = HMODULE::default();
        let here = bui_tray_icon as *const () as *const u16;
        if GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR(here),
            &mut hm,
        )
        .is_err()
        {
            return 0;
        }
        LoadImageW(Some(HINSTANCE(hm.0)), PCWSTR(1usize as *const u16), IMAGE_ICON, px, px, LR_DEFAULTCOLOR)
            .map_or(0, |h| h.0 as isize)
    }
}
