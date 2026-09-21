//! Menu bar display settings: which quota halves and which parts of each half to show.
//! Persisted as a small JSON file; every field defaults to true so older or hand-edited files
//! still load.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub session: bool,
    pub weekly: bool,
    pub glyph: bool,
    pub percent: bool,
    pub remaining: bool,
    pub alert_session: bool,
    pub alert_weekly: bool,
    pub alert_reset: bool,
    pub auto_update_check: bool,
    pub show_time_ticks: bool,
    pub show_elapsed_marker: bool,
    pub show_threshold_marks: bool,
    pub show_history: bool,
    pub session_levels: Vec<u8>,
    pub weekly_levels: Vec<u8>,
    pub poll_interval_secs: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            session: true,
            weekly: true,
            glyph: true,
            percent: true,
            remaining: true,
            alert_session: true,
            alert_weekly: true,
            alert_reset: true,
            auto_update_check: true,
            show_time_ticks: true,
            show_elapsed_marker: true,
            show_threshold_marks: true,
            show_history: true,
            session_levels: vec![80, 95],
            weekly_levels: vec![95],
            poll_interval_secs: 180,
        }
    }
}

pub const KEYS: [&str; 13] = [
    "session",
    "weekly",
    "glyph",
    "percent",
    "remaining",
    "alert_session",
    "alert_weekly",
    "alert_reset",
    "auto_update_check",
    "show_time_ticks",
    "show_elapsed_marker",
    "show_threshold_marks",
    "show_history",
];
pub const MIN_POLL_SECS: u64 = 120;
pub const MAX_POLL_SECS: u64 = 900;
pub const SESSION_LEVEL_PRESETS: [&[u8]; 3] = [&[80, 95], &[50, 80, 95], &[90, 95]];
pub const WEEKLY_LEVEL_PRESETS: [&[u8]; 3] = [&[95], &[80, 95], &[90]];
pub const INTERVAL_PRESETS: [u64; 4] = [180, 300, 600, 900];

impl Settings {
    pub fn get(&self, key: &str) -> bool {
        match key {
            "session" => self.session,
            "weekly" => self.weekly,
            "glyph" => self.glyph,
            "percent" => self.percent,
            "remaining" => self.remaining,
            "alert_session" => self.alert_session,
            "alert_weekly" => self.alert_weekly,
            "alert_reset" => self.alert_reset,
            "auto_update_check" => self.auto_update_check,
            "show_time_ticks" => self.show_time_ticks,
            "show_elapsed_marker" => self.show_elapsed_marker,
            "show_threshold_marks" => self.show_threshold_marks,
            "show_history" => self.show_history,
            _ => false,
        }
    }

    /// Flips `key`. Returns false and changes nothing when the flip would disable the last
    /// enabled member of a pair (session/weekly, percent/remaining) or the key is unknown.
    /// `glyph`, `alert_session`, `alert_weekly`, and `alert_reset` have no partner and can be freely toggled.
    pub fn toggle(&mut self, key: &str) -> bool {
        let partner_on = match key {
            "session" => self.weekly,
            "weekly" => self.session,
            "percent" => self.remaining,
            "remaining" => self.percent,
            "glyph" => true,
            "alert_session" | "alert_weekly" | "alert_reset" | "auto_update_check" => true,
            "show_time_ticks" | "show_elapsed_marker" | "show_threshold_marks" | "show_history" => {
                true
            }
            _ => return false,
        };
        if self.get(key) && !partner_on {
            return false;
        }
        match key {
            "session" => self.session = !self.session,
            "weekly" => self.weekly = !self.weekly,
            "glyph" => self.glyph = !self.glyph,
            "percent" => self.percent = !self.percent,
            "remaining" => self.remaining = !self.remaining,
            "alert_session" => self.alert_session = !self.alert_session,
            "alert_weekly" => self.alert_weekly = !self.alert_weekly,
            "alert_reset" => self.alert_reset = !self.alert_reset,
            "auto_update_check" => self.auto_update_check = !self.auto_update_check,
            "show_time_ticks" => self.show_time_ticks = !self.show_time_ticks,
            "show_elapsed_marker" => self.show_elapsed_marker = !self.show_elapsed_marker,
            "show_threshold_marks" => self.show_threshold_marks = !self.show_threshold_marks,
            "show_history" => self.show_history = !self.show_history,
            _ => return false,
        }
        true
    }

    pub fn levels(&self, key: &str) -> &[u8] {
        match key {
            "session" => &self.session_levels,
            "weekly" => &self.weekly_levels,
            _ => &[],
        }
    }

    /// Replaces a quota's levels. Rejects unknown keys and empty/invalid lists.
    pub fn set_levels(&mut self, key: &str, levels: &[u8]) -> bool {
        let Some(clean) = normalize_levels(levels) else {
            return false;
        };
        match key {
            "session" => self.session_levels = clean,
            "weekly" => self.weekly_levels = clean,
            _ => return false,
        }
        true
    }

    pub fn set_poll_interval(&mut self, secs: u64) {
        self.poll_interval_secs = secs.clamp(MIN_POLL_SECS, MAX_POLL_SECS);
    }

    /// Heals a hand-edited file that violates the "at least one of each pair is on" invariant,
    /// so the title can never end up blank.
    pub fn repair(&mut self) {
        if !self.session && !self.weekly {
            self.session = true;
        }
        if !self.percent && !self.remaining {
            self.percent = true;
        }
        self.session_levels =
            normalize_levels(&self.session_levels).unwrap_or_else(|| vec![80, 95]);
        self.weekly_levels = normalize_levels(&self.weekly_levels).unwrap_or_else(|| vec![95]);
        self.poll_interval_secs = self.poll_interval_secs.clamp(MIN_POLL_SECS, MAX_POLL_SECS);
    }
}

/// Sorted, deduplicated, 1..=100 only. None when nothing valid remains.
fn normalize_levels(levels: &[u8]) -> Option<Vec<u8>> {
    let mut v: Vec<u8> = levels
        .iter()
        .copied()
        .filter(|l| (1..=100).contains(l))
        .collect();
    v.sort_unstable();
    v.dedup();
    (!v.is_empty()).then_some(v)
}

pub fn format_levels(levels: &[u8]) -> String {
    levels
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join("/")
}

/// Parses "80,95" (spaces allowed). Any bad token or out-of-range value → None.
pub fn parse_levels(s: &str) -> Option<Vec<u8>> {
    let parsed: Option<Vec<u8>> = s.split(',').map(|t| t.trim().parse::<u8>().ok()).collect();
    let v = parsed?;
    if v.iter().any(|l| !(1..=100).contains(l)) {
        return None;
    }
    normalize_levels(&v)
}

/// What the popover needs to draw overlays. A projection, never the whole file.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PopoverSettings {
    pub session_levels: Vec<u8>,
    pub weekly_levels: Vec<u8>,
    pub show_time_ticks: bool,
    pub show_elapsed_marker: bool,
    pub show_threshold_marks: bool,
    pub show_history: bool,
}

impl From<&Settings> for PopoverSettings {
    fn from(s: &Settings) -> Self {
        Self {
            session_levels: s.session_levels.clone(),
            weekly_levels: s.weekly_levels.clone(),
            show_time_ticks: s.show_time_ticks,
            show_elapsed_marker: s.show_elapsed_marker,
            show_threshold_marks: s.show_threshold_marks,
            show_history: s.show_history,
        }
    }
}

pub fn load(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .map(|mut s: Settings| {
            s.repair();
            s
        })
        .unwrap_or_default()
}

pub fn save(path: &Path, s: &Settings) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(s).map_err(std::io::Error::other)?;
    write_atomic(path, format!("{json}\n").as_bytes())
}

/// Write to a sibling temp file and rename over the target, so a crash mid-write leaves the
/// previous file intact instead of a truncated one that loads as defaults.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

pub fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("settings.json"))
}

#[cfg(test)]
mod tests {
    use super::write_atomic;
    use super::{
        format_levels, load, parse_levels, save, PopoverSettings, Settings, KEYS, MAX_POLL_SECS,
        MIN_POLL_SECS,
    };
    use std::path::PathBuf;

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cum-settings-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("nested").join("settings.json")
    }

    #[test]
    fn default_is_all_on() {
        let s = Settings::default();
        assert!(KEYS.iter().all(|k| s.get(k)));
    }

    #[test]
    fn toggle_flips_independent_keys() {
        let mut s = Settings::default();
        assert!(s.toggle("glyph"));
        assert!(!s.glyph);
        assert!(s.toggle("glyph"));
        assert!(s.glyph);
        assert!(s.toggle("weekly"));
        assert!(!s.weekly && s.session);
    }

    #[test]
    fn toggle_refuses_to_disable_last_of_each_pair() {
        let mut s = Settings::default();
        assert!(s.toggle("weekly"));
        assert!(!s.toggle("session"), "session is the last enabled quota");
        assert!(s.session);
        assert!(s.toggle("remaining"));
        assert!(!s.toggle("percent"), "percent is the last enabled part");
        assert!(s.percent);
        // Re-enabling the partner makes the other toggleable again.
        assert!(s.toggle("weekly"));
        assert!(s.toggle("session"));
        assert!(!s.session && s.weekly);
    }

    #[test]
    fn toggle_unknown_key_is_noop() {
        let mut s = Settings::default();
        assert!(!s.toggle("colour"));
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn load_missing_or_garbage_returns_default() {
        let p = temp_path("missing");
        assert_eq!(load(&p), Settings::default());
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        std::fs::write(&p, "not json").expect("write");
        assert_eq!(load(&p), Settings::default());
    }

    #[test]
    fn save_then_load_round_trips_and_creates_parents() {
        let p = temp_path("roundtrip");
        let mut s = Settings::default();
        s.toggle("glyph");
        s.toggle("weekly");
        save(&p, &s).expect("save");
        let raw = std::fs::read_to_string(&p).expect("read");
        assert!(raw.ends_with('\n'));
        assert!(raw.contains("\"glyph\": false"));
        assert_eq!(load(&p), s);
    }

    #[test]
    fn load_repairs_invariant_violations() {
        let p = temp_path("repair");
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        std::fs::write(
            &p,
            r#"{"session": false, "weekly": false, "percent": false, "remaining": false}"#,
        )
        .expect("write");
        let s = load(&p);
        assert!(s.session);
        assert!(!s.weekly);
        assert!(s.percent);
        assert!(!s.remaining);
        assert!(s.glyph);
    }

    #[test]
    fn load_tolerates_missing_and_unknown_fields() {
        let p = temp_path("partial");
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        std::fs::write(&p, r#"{"glyph": false, "theme": "dark"}"#).expect("write");
        let s = load(&p);
        assert!(!s.glyph);
        assert!(s.session && s.weekly && s.percent && s.remaining);
    }

    #[test]
    fn alert_keys_toggle_freely() {
        assert_eq!(KEYS.len(), 13);
        let mut s = Settings::default();
        assert!(s.alert_session && s.alert_weekly && s.alert_reset);
        assert!(s.auto_update_check);
        assert!(s.toggle("auto_update_check"));
        assert!(!s.get("auto_update_check"));
        assert!(s.toggle("auto_update_check"));
        assert!(s.show_history);
        assert!(s.toggle("show_history"));
        assert!(!s.get("show_history"));
        assert!(s.toggle("show_history"));
        assert!(PopoverSettings::from(&s).show_history);
        assert!(s.toggle("alert_reset"));
        assert!(!s.get("alert_reset"));
        assert!(s.toggle("alert_reset"));
        assert!(s.toggle("alert_session"));
        assert!(s.toggle("alert_weekly"));
        assert!(!s.get("alert_session") && !s.get("alert_weekly"));
        assert!(s.toggle("alert_weekly"));
        assert!(s.alert_weekly);
    }

    #[test]
    fn load_file_without_alert_fields_defaults_them_on() {
        let p = temp_path("old-shape");
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        std::fs::write(&p, r#"{"session": true, "weekly": false, "glyph": true, "percent": true, "remaining": true}"#)
            .expect("write");
        let s = load(&p);
        assert!(!s.weekly);
        assert!(s.alert_session && s.alert_weekly);
    }

    #[test]
    fn new_fields_default() {
        let s = Settings::default();
        assert_eq!(KEYS.len(), 13);
        assert!(s.show_time_ticks && s.show_elapsed_marker && s.show_threshold_marks);
        assert_eq!(s.session_levels, vec![80, 95]);
        assert_eq!(s.weekly_levels, vec![95]);
        assert_eq!(s.poll_interval_secs, 180);
        assert_eq!(s.levels("session"), &[80, 95]);
        assert_eq!(s.levels("weekly"), &[95]);
        assert!(s.levels("weekly:fable").is_empty());
    }

    #[test]
    fn repair_normalizes_levels_and_interval() {
        let mut s = Settings::default();
        s.session_levels = vec![95, 0, 80, 120, 80];
        s.weekly_levels = vec![];
        s.poll_interval_secs = 50;
        s.repair();
        assert_eq!(s.session_levels, vec![80, 95]);
        assert_eq!(s.weekly_levels, vec![95]);
        assert_eq!(s.poll_interval_secs, MIN_POLL_SECS);
        s.poll_interval_secs = 5000;
        s.repair();
        assert_eq!(s.poll_interval_secs, MAX_POLL_SECS);
    }

    #[test]
    fn set_levels_and_interval() {
        let mut s = Settings::default();
        assert!(s.set_levels("weekly", &[80, 95]));
        assert_eq!(s.weekly_levels, vec![80, 95]);
        assert!(!s.set_levels("glyph", &[50]));
        assert!(!s.set_levels("session", &[]));
        s.set_poll_interval(10);
        assert_eq!(s.poll_interval_secs, MIN_POLL_SECS);
        s.set_poll_interval(600);
        assert_eq!(s.poll_interval_secs, 600);
    }

    #[test]
    fn levels_parse_and_format() {
        assert_eq!(parse_levels("80,95"), Some(vec![80, 95]));
        assert_eq!(parse_levels("95, 80"), Some(vec![80, 95]));
        assert_eq!(parse_levels("x"), None);
        assert_eq!(parse_levels(""), None);
        assert_eq!(parse_levels("0,80"), None);
        assert_eq!(format_levels(&[80, 95]), "80/95");
        assert_eq!(format_levels(&[95]), "95");
    }

    #[test]
    fn overlay_keys_toggle_freely() {
        let mut s = Settings::default();
        for k in [
            "show_time_ticks",
            "show_elapsed_marker",
            "show_threshold_marks",
        ] {
            assert!(s.toggle(k));
            assert!(!s.get(k));
        }
    }

    #[test]
    fn popover_projection() {
        let mut s = Settings::default();
        s.toggle("show_elapsed_marker");
        let p = PopoverSettings::from(&s);
        assert_eq!(p.session_levels, vec![80, 95]);
        assert!(!p.show_elapsed_marker && p.show_time_ticks);
        let json = serde_json::to_value(&p).expect("serializes");
        assert_eq!(json["weekly_levels"], serde_json::json!([95]));
    }

    #[test]
    fn old_file_gets_new_defaults() {
        let p = temp_path("v12-shape");
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        std::fs::write(&p, r#"{"alert_session": false}"#).expect("write");
        let s = load(&p);
        assert!(!s.alert_session);
        assert_eq!(s.poll_interval_secs, 180);
        assert_eq!(s.session_levels, vec![80, 95]);
    }

    #[test]
    fn write_atomic_replaces_the_file_and_leaves_no_temp() {
        let dir = std::env::temp_dir().join(format!("cum-atomic-{}", std::process::id()));
        let path = dir.join("settings.json");
        write_atomic(&path, b"one").expect("first write");
        write_atomic(&path, b"two").expect("second write");
        assert_eq!(std::fs::read(&path).expect("read"), b"two");
        assert!(!path.with_extension("tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
