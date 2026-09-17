//! Menu bar display settings: which quota halves and which parts of each half to show.
//! Persisted as a small JSON file; every field defaults to true so older or hand-edited files
//! still load.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub session: bool,
    pub weekly: bool,
    pub glyph: bool,
    pub percent: bool,
    pub remaining: bool,
    pub alert_session: bool,
    pub alert_weekly: bool,
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
        }
    }
}

pub const KEYS: [&str; 7] = [
    "session",
    "weekly",
    "glyph",
    "percent",
    "remaining",
    "alert_session",
    "alert_weekly",
];

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
            _ => false,
        }
    }

    /// Flips `key`. Returns false and changes nothing when the flip would disable the last
    /// enabled member of a pair (session/weekly, percent/remaining) or the key is unknown.
    /// `glyph`, `alert_session`, and `alert_weekly` have no partner and can be freely toggled.
    pub fn toggle(&mut self, key: &str) -> bool {
        let partner_on = match key {
            "session" => self.weekly,
            "weekly" => self.session,
            "percent" => self.remaining,
            "remaining" => self.percent,
            "glyph" => true,
            "alert_session" | "alert_weekly" => true,
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
            _ => return false,
        }
        true
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
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(s).map_err(std::io::Error::other)?;
    std::fs::write(path, format!("{json}\n"))
}

pub fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("settings.json"))
}

#[cfg(test)]
mod tests {
    use super::{load, save, Settings, KEYS};
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
        assert_eq!(KEYS.len(), 7);
        let mut s = Settings::default();
        assert!(s.alert_session && s.alert_weekly);
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
}
