//! Append-only local log with size-capped rotation. Callers must never pass secrets.

use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

pub const MAX_BYTES: u64 = 1_048_576;
const FILE_NAME: &str = "claude-usage-monitor.log";

pub fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(FILE_NAME))
}

/// Appends one line, rotating the file to `<name>.1` first when it is over `MAX_BYTES`.
pub fn append_at(path: &Path, now: i64, line: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let oversized = std::fs::metadata(path)
        .map(|m| m.len() > MAX_BYTES)
        .unwrap_or(false);
    if oversized {
        std::fs::rename(path, path.with_extension("log.1"))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{} {line}", rfc3339(now))
}

pub fn append(path: &Path, line: &str) -> std::io::Result<()> {
    append_at(path, crate::poll::now(), line)
}

/// Best-effort logging from anywhere that holds an `AppHandle`.
pub fn write(app: &AppHandle, line: &str) {
    if let Some(p) = path(app) {
        let _ = append(&p, line);
    }
}

/// `YYYY-MM-DDTHH:MM:SSZ` from unix seconds (Howard Hinnant's civil_from_days).
pub fn rfc3339(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_log(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cum-log-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("claude-usage-monitor.log")
    }

    #[test]
    fn rfc3339_formats_utc() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(1_789_588_800), "2026-09-16T20:00:00Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn append_prefixes_timestamp_and_creates_parents() {
        let p = temp_log("append");
        append_at(&p, 1_789_588_800, "startup v0.4.0").expect("append");
        append_at(&p, 1_789_588_860, "poll ok").expect("append");
        let raw = std::fs::read_to_string(&p).expect("read");
        assert_eq!(
            raw,
            "2026-09-16T20:00:00Z startup v0.4.0\n2026-09-16T20:01:00Z poll ok\n"
        );
    }

    #[test]
    fn rotates_when_over_max_bytes() {
        let p = temp_log("rotate");
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        std::fs::write(&p, vec![b'x'; MAX_BYTES as usize + 1]).expect("fill");
        append_at(&p, 0, "after rotate").expect("append");
        let rotated = p.with_extension("log.1");
        assert!(rotated.exists());
        assert_eq!(
            std::fs::metadata(&rotated).expect("meta").len(),
            MAX_BYTES + 1
        );
        assert_eq!(
            std::fs::read_to_string(&p).expect("read"),
            "1970-01-01T00:00:00Z after rotate\n"
        );
    }
}
