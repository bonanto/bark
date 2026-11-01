fn main() {
    let mut r = winresource::WindowsResource::new();
    r.set("FileDescription", "Bark: System Volume Fix (core engine)")
        .set("ProductName", "Bark")
        .set("CompanyName", "Bonanto")
        .set("LegalCopyright", "Copyright (C) 2026 Bonanto. Licensed under GPLv3.")
        .set("FileVersion", "0.0.1.1")
        .set("ProductVersion", "1.0.1")
        .set("OriginalFilename", "sysvol.dll")
        .set("InternalName", "sysvol")
        .set_version_info(winresource::VersionInfo::FILEVERSION, 0x0000_0000_0001_0001)
        .set_version_info(winresource::VersionInfo::PRODUCTVERSION, 0x0001_0000_0001_0000);
    r.compile().expect("resource compile failed");
}
