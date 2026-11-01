#![windows_subsystem = "windows"]

mod gfx;
mod md;
mod payload;
mod reg;
mod ui;
mod work;

pub const VERSION: &str = "1.0.1";

fn main() {
    std::panic::set_hook(Box::new(|info| work::log(&format!("PANIC: {info}"))));
    // A process whose current directory is the install folder keeps that folder from being deleted.
    let _ = std::env::set_current_dir(std::env::temp_dir());
    let args: Vec<String> = std::env::args().collect();
    let has = |f: &str| args.iter().any(|a| a.eq_ignore_ascii_case(f));
    let existing = work::existing();

    let mode = if has("/uninstall") {
        ui::Mode::Uninstall
    } else if has("/modify") && existing.is_some() {
        ui::Mode::Maint
    } else if existing.as_ref().map_or(false, |(v, _)| v == VERSION) {
        // The same version is already installed: offer Modify/Repair or Uninstall.
        ui::Mode::Maint
    } else {
        ui::Mode::Install
    };

    // Setup must not run from the folder it is about to change or delete (a running exe cannot be deleted):
    // when started from the install folder, re-launch a copy from %TEMP% with the same arguments.
    let relocated = has("/relocated");
    if !relocated {
        if let (Some((_, dir)), Ok(me)) = (&existing, std::env::current_exe()) {
            let inside = me.parent().map_or(false, |p| p.to_string_lossy().to_lowercase() == dir.to_string_lossy().to_lowercase());
            if inside {
                let tmp = std::env::temp_dir().join(format!("BarkSetup-{}.exe", std::process::id()));
                if std::fs::copy(&me, &tmp).is_ok() && std::process::Command::new(&tmp).args(&args[1..]).arg("/relocated").spawn().is_ok() {
                    return;
                }
            }
        }
    }

    unsafe { ui::run(mode, existing) };

    // After exit: remove the temporary copy of ourselves and, if it could not be deleted before, the Bark folder.
    let leftover = work::LEFTOVER.lock().ok().and_then(|l| l.clone());
    if relocated || leftover.is_some() {
        use std::os::windows::process::CommandExt;
        let mut cmd = String::from("/C ping 127.0.0.1 -n 3 >nul");
        if relocated {
            if let Ok(me) = std::env::current_exe() {
                cmd.push_str(&format!(" & del /f /q \"{}\"", me.display()));
            }
        }
        if let Some(d) = leftover {
            cmd.push_str(&format!(" & rmdir /s /q \"{}\"", d.display()));
        }
        let _ = std::process::Command::new("cmd")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .current_dir(std::env::temp_dir())
            .raw_arg(cmd)
            .creation_flags(0x0800_0000)
            .spawn();
    }
}
