use crate::cfg::ROOT;
use crate::reg::{delete_key, Key};
use windows::Win32::System::Registry::REG_SZ;

const SCHEMES: &str = r"AppEvents\Schemes";
const APPS: &str = r"AppEvents\Schemes\Apps";

fn bk_path() -> String {
    format!(r"{ROOT}\SoundBackup")
}

// (backup value name, path of the event's .Current key)
fn events() -> Vec<(String, String)> {
    let mut v = Vec::new();
    if let Some(apps) = Key::open(APPS, false) {
        for a in apps.subkeys() {
            let ak = format!(r"{APPS}\{a}");
            if let Some(k) = Key::open(&ak, false) {
                for e in k.subkeys() {
                    v.push((format!("{a}|{e}"), format!(r"{ak}\{e}\.Current")));
                }
            }
        }
    }
    v
}

/// Backs up current sounds, then applies "No Sounds".
pub fn enter() -> bool {
    let Some(bk) = Key::create(&bk_path()) else { return false };
    let mut n = 0;
    for (name, path) in events() {
        let Some(k) = Key::open(&path, true) else { continue };
        let Some((ty, val)) = k.raw_str("") else { continue };
        if val.is_empty() {
            continue;
        }
        if bk.raw_str(&name).is_none() {
            bk.set_str(&name, ty, &val);
        }
        k.set_str("", REG_SZ.0, "");
        n += 1;
    }
    if let Some(s) = Key::open(SCHEMES, true) {
        if let Some((_, cur)) = s.raw_str("") {
            if cur != ".None" {
                if bk.raw_str("__scheme").is_none() {
                    bk.set_str("__scheme", REG_SZ.0, &cur);
                }
                s.set_str("", REG_SZ.0, ".None");
            }
        }
    }
    lg!("quiet: cleared {n} sound events");
    true
}

/// Restores the backed-up sounds and scheme.
pub fn leave() {
    let Some(bk) = Key::open(&bk_path(), true) else { return };
    let mut n = 0;
    for (name, path) in events() {
        if let Some((ty, val)) = bk.raw_str(&name) {
            if let Some(k) = Key::open(&path, true) {
                k.set_str("", ty, &val);
                n += 1;
            }
            bk.del_value(&name);
        }
    }
    if let Some((_, s)) = bk.raw_str("__scheme") {
        if let Some(k) = Key::open(SCHEMES, true) {
            k.set_str("", REG_SZ.0, &s);
        }
        bk.del_value("__scheme");
    }
    drop(bk);
    delete_key(&bk_path());
    lg!("quiet: restored {n} sound events");
}