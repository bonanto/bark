fn main() {
    // The notification-area icon lives in this DLL (resource id 1), loaded at run time by bui_tray_icon().
    let mut r = winresource::WindowsResource::new();
    r.set_icon_with_id("assets/tray.ico", "1")
        .set("FileDescription", "Bark: UI")
        .set("ProductName", "Bark")
        .set("CompanyName", "Bonanto")
        .set("LegalCopyright", "Copyright (C) 2026 Bonanto. Licensed under GPLv3.")
        .set("FileVersion", "0.0.1.1")
        .set("ProductVersion", "1.0.1")
        .set("InternalName", "bui")
        .set_version_info(winresource::VersionInfo::FILEVERSION, 0x0000_0000_0001_0001)
        .set_version_info(winresource::VersionInfo::PRODUCTVERSION, 0x0001_0000_0001_0000);
    println!("cargo:rerun-if-changed=assets/tray.ico");
    r.compile().expect("resource compile failed");
}
