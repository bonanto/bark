use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use windows::Win32::System::SystemInformation::GetLocalTime;

static DIR: OnceLock<PathBuf> = OnceLock::new();
static START: OnceLock<String> = OnceLock::new();
static DAY: Mutex<u32> = Mutex::new(0);
const KEEP_DAYS: i64 = 7;
const MAX_SESSIONS: usize = 200;

// Days since 1970-01-01 (civil calendar).
fn ord(y: i32, m: i32, d: i32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era as i64 * 146_097 + doe as i64 - 719_468
}

pub fn stamp() -> String {
    let t = unsafe { GetLocalTime() };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
    )
}

pub fn init(app_dir: &Path) {
    let dir = app_dir.join("log");
    let _ = fs::create_dir_all(&dir);
    let _ = START.set(stamp());
    let _ = DIR.set(dir);
    prune();
}

// Deletes daily logs older than KEEP_DAYS and the old single-file logs.
fn prune() {
    let Some(dir) = DIR.get() else { return };
    let t = unsafe { GetLocalTime() };
    let today = ord(t.wYear as i32, t.wMonth as i32, t.wDay as i32);
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let Some(d) = name.strip_prefix("sysvol-").and_then(|s| s.strip_suffix(".log")) else {
                continue;
            };
            let p: Vec<i32> = d.split('-').filter_map(|x| x.parse().ok()).collect();
            if p.len() == 3 && today - ord(p[0], p[1], p[2]) >= KEEP_DAYS {
                let _ = fs::remove_file(e.path());
            }
        }
    }
    let _ = fs::remove_file(dir.join("sysvol.log"));
    let _ = fs::remove_file(dir.join("sysvol.old"));
}

pub fn write(msg: &str) {
    let Some(dir) = DIR.get() else { return };
    let t = unsafe { GetLocalTime() };
    let key = t.wYear as u32 * 10_000 + t.wMonth as u32 * 100 + t.wDay as u32;
    let mut d = DAY.lock().unwrap_or_else(|e| e.into_inner());
    if *d != key {
        let first = *d == 0;
        *d = key;
        if !first {
            prune();
        }
    }
    let p = dir.join(format!("sysvol-{:04}-{:02}-{:02}.log", t.wYear, t.wMonth, t.wDay));
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(p) {
        let _ = writeln!(f, "{:02}:{:02}:{:02} {}", t.wHour, t.wMinute, t.wSecond, msg);
    }
}

// One JSON line per session in sessions.jsonl (last MAX_SESSIONS kept).
pub fn session_end(blocked: i32, changes: u32, dings: u32) {
    let Some(dir) = DIR.get() else { return };
    let line = format!(
        "{{\"start\":\"{}\",\"end\":\"{}\",\"blocked\":{},\"volume_changes\":{},\"ding_mutes\":{}}}",
        START.get().map(|s| s.as_str()).unwrap_or(""),
        stamp(),
        blocked,
        changes,
        dings
    );
    let p = dir.join("sessions.jsonl");
    let mut lines: Vec<String> = fs::read_to_string(&p)
        .map(|s| s.lines().map(String::from).collect())
        .unwrap_or_default();
    lines.push(line);
    if lines.len() > MAX_SESSIONS {
        let n = lines.len() - MAX_SESSIONS;
        lines.drain(..n);
    }
    let _ = fs::write(&p, lines.join("\n") + "\n");
}