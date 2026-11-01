// Packs everything from ..\dist\{x86,x64,arm64} (plus LICENSE and EULA.md from the root, when present) into one
// compressed blob that is embedded in the installer. Build Bark first (dist.ps1), then build the installer.
use std::{fs, path::Path};

fn main() {
    let root = Path::new("..");
    let dist = root.join("dist");
    let mut files: Vec<(u8, String, Vec<u8>)> = Vec::new();

    for (dir, tag) in [("x86", 0u8), ("x64", 1u8), ("arm64", 2u8)] {
        let d = dist.join(dir);
        let mut n = 0;
        if let Ok(rd) = fs::read_dir(&d) {
            for e in rd.flatten() {
                if e.path().is_file() {
                    if let Ok(data) = fs::read(e.path()) {
                        files.push((tag, e.file_name().to_string_lossy().into_owned(), data));
                        n += 1;
                    }
                }
            }
        }
        if n == 0 {
            println!("cargo:warning=dist\\{dir} is empty or missing: this installer will not support that architecture");
        }
    }
    for name in ["LICENSE", "EULA.md"] {
        if let Ok(data) = fs::read(root.join(name)) {
            files.push((255, name.to_string(), data));
        } else if name == "LICENSE" {
            println!("cargo:warning=no LICENSE file in the root folder: it will not be installed");
        }
    }

    // [u32 count] then per file: [u8 arch][u16 name len][name][u32 data len][data]
    let mut raw: Vec<u8> = Vec::new();
    raw.extend((files.len() as u32).to_le_bytes());
    for (tag, name, data) in &files {
        raw.push(*tag);
        raw.extend((name.len() as u16).to_le_bytes());
        raw.extend(name.as_bytes());
        raw.extend((data.len() as u32).to_le_bytes());
        raw.extend(data);
    }
    let packed = miniz_oxide::deflate::compress_to_vec(&raw, 10);
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("payload.bin");
    fs::write(out, packed).unwrap();

    println!("cargo:rerun-if-changed=../dist");
    println!("cargo:rerun-if-changed=../LICENSE");
    println!("cargo:rerun-if-changed=../EULA.md");
    println!("cargo:rerun-if-changed=../sysvol/assets/bark.ico");

    // asInvoker matters: a 32-bit exe whose name contains "setup" and has no manifest triggers a UAC prompt.
    let manifest = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security><requestedPrivileges><requestedExecutionLevel level="asInvoker" uiAccess="false"/></requestedPrivileges></security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application><supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/></application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings><dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true</dpiAware></windowsSettings>
  </application>
</assembly>"#;
    let mut r = winresource::WindowsResource::new();
    r.set_icon("../sysvol/assets/bark.ico")
        .set_manifest(manifest)
        .set("FileDescription", "Bark Setup")
        .set("ProductName", "Bark")
        .set("CompanyName", "Bonanto")
        .set("LegalCopyright", "Copyright (C) 2026 Bonanto. Licensed under GPLv3.")
        .set("FileVersion", "0.0.1.1")
        .set("ProductVersion", "1.0.1")
        .set("OriginalFilename", "BarkSetup.exe")
        .set("InternalName", "BarkSetup")
        .set_version_info(winresource::VersionInfo::FILEVERSION, 0x0000_0000_0001_0001)
        .set_version_info(winresource::VersionInfo::PRODUCTVERSION, 0x0001_0000_0001_0000);
    r.compile().expect("resource compile failed");
}
