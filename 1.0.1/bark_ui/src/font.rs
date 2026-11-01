use crate::state::*;
use std::{
    ffi::c_void,
    mem::size_of,
    sync::atomic::{AtomicU8, Ordering::Relaxed},
};
use windows::Win32::{Graphics::Gdi::*, UI::HiDpi::*, UI::WindowsAndMessaging::*};

static FACE: AtomicU8 = AtomicU8::new(0);

fn face_name(id: u8) -> &'static str {
    if id == 1 {
        "Segoe Fluent Icons"
    } else {
        "Segoe MDL2 Assets"
    }
}

unsafe fn raw_icon_font(face: &str, px: i32) -> HFONT {
    let mut lf = LOGFONTW::default();
    lf.lfHeight = -px;
    lf.lfWeight = 400;
    lf.lfQuality = ANTIALIASED_QUALITY;
    for (i, c) in face.encode_utf16().take(31).enumerate() {
        lf.lfFaceName[i] = c;
    }
    CreateFontIndirectW(&lf)
}

unsafe fn face_id() -> u8 {
    let cached = FACE.load(Relaxed);
    if cached != 0 {
        return cached;
    }
    let dc = GetDC(None);
    let f = raw_icon_font("Segoe Fluent Icons", 16);
    let old = SelectObject(dc, f.into());
    let mut buf = [0u16; 64];
    let n = (GetTextFaceW(dc, Some(&mut buf)).max(1) as usize - 1).min(63);
    let got = String::from_utf16_lossy(&buf[..n]);
    SelectObject(dc, old);
    let _ = DeleteObject(f.into());
    ReleaseDC(None, dc);
    let id = if got.eq_ignore_ascii_case("Segoe Fluent Icons") { 1 } else { 2 };
    FACE.store(id, Relaxed);
    id
}

/// Raw HFONT handle for the best available icon font. The caller deletes it.
pub unsafe fn icon_font(px: i32) -> isize {
    raw_icon_font(face_name(face_id()), px).0 as isize
}

pub unsafe fn make_fonts(dpi: u32) {
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
    FN.store(mk(1.17, 400), Relaxed);
    FB.store(mk(1.3, 600), Relaxed);
    FS.store(mk(1.0, 400), Relaxed);
    let d = dpi as i32;
    FI_S.store(icon_font(12 * d / 96), Relaxed);
    FI_M.store(icon_font(16 * d / 96), Relaxed);
}

pub unsafe fn free_fonts() {
    for f in [&FN, &FB, &FS, &FI_S, &FI_M] {
        let v = f.swap(0, Relaxed);
        if v != 0 {
            let _ = DeleteObject(HGDIOBJ(v as *mut c_void));
        }
    }
}
