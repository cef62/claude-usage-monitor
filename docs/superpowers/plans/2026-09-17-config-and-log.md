# Configuration, Bar Overlays and Log (v1.3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tray-menu presets for alert thresholds, poll interval and popover overlays (persisted), threshold marks on the popover bars, a "Send test notification" item, and a capped local log revealed from the menu.

**Architecture:** `Settings` grows (levels, interval, three overlay bools) and becomes `Clone` behind an `Arc<Mutex<_>>` shared with the poll thread. `alerts.rs` reads levels from settings with `top = max(levels)`. New `log.rs` appends RFC3339-prefixed lines with 1 MB rotation; the poll loop and alert delivery log through it. `tray.rs` gets radio submenus and action items via a parsed `MenuAction`. The popover fetches a `PopoverSettings` projection (command + `settings` event) to draw overlays and threshold marks.

**Tech Stack:** Rust, Tauri 2.11 (`menu::SubmenuBuilder`, `CheckMenuItem`, `tauri_plugin_opener::reveal_item_in_dir`), React/TS, Vitest. No new crates.

**Spec:** `docs/superpowers/specs/2026-09-17-config-and-log-design.md`

## Global Constraints

- Branch `feat/config-and-log` (exists, off `main` at `44daa86`). Never commit to `main`.
- Presets: session levels `[80,95]` (default) · `[50,80,95]` · `[90,95]`; weekly `[95]` (default) · `[80,95]` · `[90]`; interval `180` (default) · `300` · `600` · `900`; `MIN_POLL_SECS` 120, `MAX_POLL_SECS` 900.
- Alert rule: `top = max(levels)`; `⚠` and unconditional firing at `top`; lower levels time-aware (`percent > elapsed_pct`); once per `(key, resets_at ± 60 s, level)`.
- `KEYS` (10): `session, weekly, glyph, percent, remaining, alert_session, alert_weekly, show_time_ticks, show_elapsed_marker, show_threshold_marks`.
- Log: `app_data_dir/claude-usage-monitor.log`, `MAX_BYTES` 1_048_576, rotate to `<name>.1`; line = `<RFC3339 UTC> <text>`; never the token, headers or URLs with secrets.
- No new capabilities; no new dependencies. No `unwrap`/`expect` in non-test code (except `main()`'s final `.expect`). `cargo clippy -- -D warnings` clean, no `#[allow]`. Biome/tsc clean.
- Commit: `git -c commit.gpgsign=false commit -m "<subject>" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"`. Every user-visible change ships with a changeset.

---

### Task 1: Settings — levels, interval, overlays; `Copy` → `Clone`

**Files:**
- Modify: `src-tauri/src/settings.rs`, `src-tauri/src/tray.rs` (three `*guard` copies → `.clone()`), `src-tauri/src/main.rs` (one copy → `.clone()`)

**Interfaces:**
- Produces: fields `show_time_ticks`, `show_elapsed_marker`, `show_threshold_marks: bool`, `session_levels: Vec<u8>`, `weekly_levels: Vec<u8>`, `poll_interval_secs: u64`; consts `KEYS: [&str; 10]`, `MIN_POLL_SECS`, `MAX_POLL_SECS`, `SESSION_LEVEL_PRESETS: [&[u8]; 3]`, `WEEKLY_LEVEL_PRESETS: [&[u8]; 3]`, `INTERVAL_PRESETS: [u64; 4]`; methods `levels(&self, key) -> &[u8]`, `set_levels(&mut self, key, &[u8]) -> bool`, `set_poll_interval(&mut self, u64)`, `repair` extended; free fns `format_levels(&[u8]) -> String`, `parse_levels(&str) -> Option<Vec<u8>>`; `#[derive(Serialize)] pub struct PopoverSettings { session_levels, weekly_levels, show_time_ticks, show_elapsed_marker, show_threshold_marks }` + `PopoverSettings::from(&Settings)`.

- [ ] **Step 1: Failing tests** (append inside `mod tests`)

```rust
    #[test]
    fn new_fields_default() {
        let s = Settings::default();
        assert_eq!(KEYS.len(), 10);
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
        for k in ["show_time_ticks", "show_elapsed_marker", "show_threshold_marks"] {
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
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml settings 2>&1 | grep -E 'error\[|no field|cannot find' | head -3`
Expected: `no field `show_time_ticks``, `cannot find function `parse_levels``.

- [ ] **Step 3: Implement in `settings.rs`**

Change the derive to `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]` (drop `Copy`). Add fields after `alert_weekly`:

```rust
    pub show_time_ticks: bool,
    pub show_elapsed_marker: bool,
    pub show_threshold_marks: bool,
    pub session_levels: Vec<u8>,
    pub weekly_levels: Vec<u8>,
    pub poll_interval_secs: u64,
```

`Default` adds `show_time_ticks: true, show_elapsed_marker: true, show_threshold_marks: true, session_levels: vec![80, 95], weekly_levels: vec![95], poll_interval_secs: 180,`.

Constants:

```rust
pub const KEYS: [&str; 10] = [
    "session",
    "weekly",
    "glyph",
    "percent",
    "remaining",
    "alert_session",
    "alert_weekly",
    "show_time_ticks",
    "show_elapsed_marker",
    "show_threshold_marks",
];
pub const MIN_POLL_SECS: u64 = 120;
pub const MAX_POLL_SECS: u64 = 900;
pub const SESSION_LEVEL_PRESETS: [&[u8]; 3] = [&[80, 95], &[50, 80, 95], &[90, 95]];
pub const WEEKLY_LEVEL_PRESETS: [&[u8]; 3] = [&[95], &[80, 95], &[90]];
pub const INTERVAL_PRESETS: [u64; 4] = [180, 300, 600, 900];
```

`get` gains arms for the three `show_*` keys; `toggle`'s `partner_on` match gains `"show_time_ticks" | "show_elapsed_marker" | "show_threshold_marks" => true,` and the flip match gains the three arms. Add methods to `impl Settings`:

```rust
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
```

Extend `repair`:

```rust
        self.session_levels = normalize_levels(&self.session_levels).unwrap_or_else(|| vec![80, 95]);
        self.weekly_levels = normalize_levels(&self.weekly_levels).unwrap_or_else(|| vec![95]);
        self.poll_interval_secs = self.poll_interval_secs.clamp(MIN_POLL_SECS, MAX_POLL_SECS);
```

Free functions:

```rust
/// Sorted, deduplicated, 1..=100 only. None when nothing valid remains.
fn normalize_levels(levels: &[u8]) -> Option<Vec<u8>> {
    let mut v: Vec<u8> = levels.iter().copied().filter(|l| (1..=100).contains(l)).collect();
    v.sort_unstable();
    v.dedup();
    (!v.is_empty()).then_some(v)
}

pub fn format_levels(levels: &[u8]) -> String {
    levels.iter().map(u8::to_string).collect::<Vec<_>>().join("/")
}

/// Parses "80,95" (spaces allowed). Any bad token or out-of-range value → None.
pub fn parse_levels(s: &str) -> Option<Vec<u8>> {
    let parsed: Option<Vec<u8>> = s.split(',').map(|t| t.trim().parse::<u8>().ok()).collect();
    normalize_levels(&parsed?)
}

/// What the popover needs to draw overlays. A projection, never the whole file.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PopoverSettings {
    pub session_levels: Vec<u8>,
    pub weekly_levels: Vec<u8>,
    pub show_time_ticks: bool,
    pub show_elapsed_marker: bool,
    pub show_threshold_marks: bool,
}

impl From<&Settings> for PopoverSettings {
    fn from(s: &Settings) -> Self {
        Self {
            session_levels: s.session_levels.clone(),
            weekly_levels: s.weekly_levels.clone(),
            show_time_ticks: s.show_time_ticks,
            show_elapsed_marker: s.show_elapsed_marker,
            show_threshold_marks: s.show_threshold_marks,
        }
    }
}
```

Note: `parse_levels("0,80")` → `[0, 80]` parses, then `normalize_levels` drops 0 → `[80]`, not `None`. The test expects `None` for `"0,80"`: make `parse_levels` strict — after parsing, if any value is outside `1..=100` return `None`: replace the last line with `let v = parsed?; if v.iter().any(|l| !(1..=100).contains(l)) { return None; } normalize_levels(&v)`.

- [ ] **Step 4: Fix the `Copy` users**

`src-tauri/src/tray.rs`: `let current = *lock_settings(app);` → `let current = lock_settings(app).clone();`; in `on_setting_toggled` `*s` → `s.clone()`; in `refresh_title` `let settings = *lock_settings(app);` → `let settings = lock_settings(app).clone();`.
`src-tauri/src/main.rs` in `notify_thresholds`: `let settings = *app.state::<Mutex<settings::Settings>>().lock()...;` → `.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();`.

- [ ] **Step 5: Run tests**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings && cargo test --manifest-path src-tauri/Cargo.toml`
Expected: clippy clean; `57 passed` (50 + 7). If clippy flags `PopoverSettings` as unused, it is used by Task 4; add nothing — the `pub` item in a `pub mod` is not dead code.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/settings.rs src-tauri/src/tray.rs src-tauri/src/main.rs
git -c commit.gpgsign=false commit -m "feat: configurable alert levels, poll interval and popover overlays in settings" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 2: Alerts read levels from settings; `top` replaces `MARKER_LEVEL`

**Files:**
- Modify: `src-tauri/src/alerts.rs`

**Interfaces:**
- Consumes: `Settings::levels(key)`.
- Produces: `pub fn top(levels: &[u8]) -> Option<u8>`; `SESSION_LEVELS`/`WEEKLY_LEVELS`/`MARKER_LEVEL` consts removed; `evaluate`/`marker` signatures unchanged.

- [ ] **Step 1: Failing tests** (append inside `mod tests`)

```rust
    fn with_levels(session: &[u8], weekly: &[u8]) -> Settings {
        let mut s = Settings::default();
        assert!(s.set_levels("session", session));
        assert!(s.set_levels("weekly", weekly));
        s
    }

    #[test]
    fn custom_levels_top_is_unconditional_and_lower_is_time_aware() {
        let s = with_levels(&[90, 95], &[95]);
        let mut st = AlertState::default();
        // 92% with 30 min left (elapsed 90%): 90 is not ahead of the clock → silent.
        assert!(evaluate(&mut st, &[session(92.0, 1800)], &s, NOW).is_empty());
        // 96%: top level fires regardless of the clock.
        let a = evaluate(&mut st, &[session(96.0, 1800)], &s, NOW);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].level, 95);
    }

    #[test]
    fn single_level_is_the_top() {
        let s = with_levels(&[95], &[95]);
        let mut st = AlertState::default();
        assert!(evaluate(&mut st, &[session(85.0, 7200)], &s, NOW).is_empty());
        assert_eq!(evaluate(&mut st, &[session(95.0, 60)], &s, NOW).len(), 1);
    }

    #[test]
    fn three_levels_fire_the_lowest_first() {
        let s = with_levels(&[50, 80, 95], &[95]);
        let mut st = AlertState::default();
        let a = evaluate(&mut st, &[session(55.0, 3 * 3600)], &s, NOW);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].level, 50);
    }

    #[test]
    fn marker_uses_top_level() {
        let s = with_levels(&[90, 95], &[95]);
        assert!(!marker(&[session(92.0, 7200)], &s));
        assert!(marker(&[session(96.0, 7200)], &s));
        assert_eq!(top(&[90, 95]), Some(95));
        assert_eq!(top(&[]), None);
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml alerts 2>&1 | grep -E 'cannot find|error\[' | head -3`
Expected: `cannot find function `top``.

- [ ] **Step 3: Implement**

Remove `SESSION_LEVELS`, `WEEKLY_LEVELS`, `MARKER_LEVEL` and the `levels(key)` free function. Add:

```rust
/// The highest configured level: `⚠` shows at or above it and it fires regardless of the clock.
pub fn top(levels: &[u8]) -> Option<u8> {
    levels.iter().copied().max()
}
```

In `evaluate`, replace the level pipeline with:

```rust
        let levels = settings.levels(&q.key);
        let Some(top_level) = top(levels) else {
            continue;
        };
        let elapsed = elapsed_pct(q, now);
        let highest = levels
            .iter()
            .copied()
            .filter(|&level| q.percent >= f64::from(level))
            .filter(|&level| level >= top_level || q.percent > elapsed)
            .filter(|&level| !already_fired(state, &q.key, q.resets_at, level))
            .max();
        let Some(highest) = highest else {
            continue;
        };
        for &level in levels.iter().filter(|&&l| l <= highest) {
            state.fired.insert((q.key.clone(), q.resets_at, level));
        }
```

`marker`:

```rust
pub fn marker(quotas: &[Quota], settings: &Settings) -> bool {
    quotas.iter().any(|q| {
        enabled(settings, &q.key)
            && top(settings.levels(&q.key)).is_some_and(|t| q.percent >= f64::from(t))
    })
}
```

Update the doc comment on `elapsed`/`marker` accordingly. Existing tests keep passing (defaults are `[80,95]`/`[95]`).

- [ ] **Step 4: Run tests, lint, commit**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings && cargo test --manifest-path src-tauri/Cargo.toml`
Expected: `61 passed`.

```bash
git add src-tauri/src/alerts.rs
git -c commit.gpgsign=false commit -m "feat: alert levels come from settings with the top level as marker" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 3: `log.rs`, poll interval from settings, poll/alert logging

**Files:**
- Create: `src-tauri/src/log.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/poll.rs`, `src-tauri/src/main.rs`

**Interfaces:**
- Produces: `log::MAX_BYTES`, `log::rfc3339(secs: i64) -> String`, `log::path(&AppHandle) -> Option<PathBuf>`, `log::append_at(path, now, line) -> io::Result<()>`, `log::append(path, line) -> io::Result<()>`, `log::write(&AppHandle, line)`; `poll::run(shared, settings: Arc<Mutex<Settings>>, log_path: Option<PathBuf>, on_update)`; `poll::next_delay(outcome, error_count, nearest_reset, now, base)`; `poll::log_line(outcome: &Outcome, snapshot: &Snapshot, delay: u64) -> String`; managed state type `Arc<Mutex<Settings>>`.

- [ ] **Step 1: Declare the module** — `lib.rs`: add `pub mod log;` (alphabetical: after `alerts`).

- [ ] **Step 2: Failing tests** — create `src-tauri/src/log.rs` with only:

```rust
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
        assert_eq!(rfc3339(1_789_588_800), "2026-09-17T20:00:00Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn append_prefixes_timestamp_and_creates_parents() {
        let p = temp_log("append");
        append_at(&p, 1_789_588_800, "startup v0.4.0").expect("append");
        append_at(&p, 1_789_588_860, "poll ok").expect("append");
        let raw = std::fs::read_to_string(&p).expect("read");
        assert_eq!(raw, "2026-09-17T20:00:00Z startup v0.4.0\n2026-09-17T20:01:00Z poll ok\n");
    }

    #[test]
    fn rotates_when_over_max_bytes() {
        let p = temp_log("rotate");
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        std::fs::write(&p, vec![b'x'; MAX_BYTES as usize + 1]).expect("fill");
        append_at(&p, 0, "after rotate").expect("append");
        let rotated = p.with_extension("log.1");
        assert!(rotated.exists());
        assert_eq!(std::fs::metadata(&rotated).expect("meta").len(), MAX_BYTES + 1);
        assert_eq!(std::fs::read_to_string(&p).expect("read"), "1970-01-01T00:00:00Z after rotate\n");
    }
}
```

Also add to `poll.rs` `mod tests`:

```rust
    #[test]
    fn success_uses_the_configured_base() {
        assert_eq!(next_delay(&Outcome::Success, 0, None, NOW, 300), 300);
        assert_eq!(next_delay(&Outcome::Success, 0, Some(NOW + 60), NOW, 300), COOLDOWN);
        assert_eq!(next_delay(&Outcome::Success, 0, Some(NOW + 250), NOW, 300), 255);
    }

    #[test]
    fn rate_limit_backoff_starts_from_base() {
        let rl = Outcome::RateLimited { retry_after: None };
        assert_eq!(next_delay(&rl, 1, None, NOW, 600), 600);
        assert_eq!(next_delay(&rl, 3, None, NOW, 600), MAX_BACKOFF);
        let ra = Outcome::RateLimited { retry_after: Some(200) };
        assert_eq!(next_delay(&ra, 1, None, NOW, 300), 300);
    }

    #[test]
    fn log_line_summarizes_the_cycle() {
        let snap = Snapshot {
            quotas: vec![
                Quota { key: "session".into(), label: "Session".into(), percent: 48.4, resets_at: NOW + 100, period_secs: 18000 },
                Quota { key: "weekly".into(), label: "Weekly".into(), percent: 64.0, resets_at: NOW + 100, period_secs: 604800 },
            ],
            fetched_at: Some(NOW),
            next_poll_at: NOW + 180,
            status: Status::Ok,
        };
        assert_eq!(log_line(&Outcome::Success, &snap, 180), "poll ok session=48% weekly=64% next=180s");
        let mut rl = snap.clone();
        rl.status = Status::RateLimited { until: NOW + 360 };
        assert_eq!(log_line(&Outcome::RateLimited { retry_after: None }, &rl, 360), "poll rate_limited retry=360s");
        let mut err = snap.clone();
        err.status = Status::Error { message: "HTTP 500".into() };
        assert_eq!(log_line(&Outcome::Failed, &err, 30), "poll error HTTP 500");
        let mut nt = snap.clone();
        nt.status = Status::NoToken;
        assert_eq!(log_line(&Outcome::Failed, &nt, 30), "poll no_token");
        let mut ae = snap;
        ae.status = Status::AuthExpired;
        assert_eq!(log_line(&Outcome::Unauthorized, &ae, 30), "poll auth_expired");
    }
```

Update the existing `next_delay` tests to pass `BASE_INTERVAL` as the fifth argument.

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E 'cannot find|error\[E0061' | head -3`
Expected: `cannot find function `rfc3339`` / `E0061` on `next_delay`.

- [ ] **Step 4: Implement `log.rs`** (above the tests)

```rust
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
    let oversized = std::fs::metadata(path).map(|m| m.len() > MAX_BYTES).unwrap_or(false);
    if oversized {
        std::fs::rename(path, path.with_extension("log.1"))?;
    }
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
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
```

- [ ] **Step 5: Implement in `poll.rs`**

Add `use crate::settings::{Settings, MAX_POLL_SECS, MIN_POLL_SECS}; use std::path::PathBuf;` and `use crate::log;`. Change `next_delay`:

```rust
pub fn next_delay(
    outcome: &Outcome,
    error_count: u32,
    nearest_reset: Option<i64>,
    now: i64,
    base: u64,
) -> u64 {
    match outcome {
        Outcome::Success => match nearest_reset {
            Some(reset) if reset > now && (reset - now) as u64 + RESET_GRACE < base => {
                ((reset - now) as u64 + RESET_GRACE).max(COOLDOWN)
            }
            _ => base,
        },
        Outcome::Unauthorized | Outcome::Failed => ERROR_RETRY,
        Outcome::RateLimited {
            retry_after: Some(secs),
        } => (*secs).clamp(base, MAX_BACKOFF),
        Outcome::RateLimited { retry_after: None } => {
            let shift = error_count.saturating_sub(1).min(4);
            (base << shift).min(MAX_BACKOFF)
        }
    }
}
```

Keep `BASE_INTERVAL` as the documented default (`pub const BASE_INTERVAL: u64 = 180;` stays, used by tests and as a fallback). Add:

```rust
/// One line per cycle for the local log. Percentages only; never anything from the request.
pub fn log_line(outcome: &Outcome, snapshot: &Snapshot, delay: u64) -> String {
    match &snapshot.status {
        Status::Ok if matches!(outcome, Outcome::Success) => {
            let pct = |key: &str| {
                snapshot
                    .quotas
                    .iter()
                    .find(|q| q.key == key)
                    .map(|q| format!("{}%", q.percent.round() as i64))
                    .unwrap_or_else(|| "-".to_string())
            };
            format!("poll ok session={} weekly={} next={delay}s", pct("session"), pct("weekly"))
        }
        Status::RateLimited { .. } => format!("poll rate_limited retry={delay}s"),
        Status::AuthExpired => "poll auth_expired".to_string(),
        Status::NoToken => "poll no_token".to_string(),
        Status::Error { message } => format!("poll error {message}"),
        Status::Ok => format!("poll ok next={delay}s"),
    }
}
```

Change `run`'s signature and body:

```rust
pub fn run<F: Fn(&Snapshot) + Send + 'static>(
    shared: Shared,
    settings: Arc<Mutex<Settings>>,
    log_path: Option<PathBuf>,
    on_update: F,
) {
```

Inside the loop, right before `let delay = next_delay(...)`, read the base:

```rust
            let base = settings
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .poll_interval_secs
                .clamp(MIN_POLL_SECS, MAX_POLL_SECS);
            let delay = next_delay(&outcome, error_count, nearest_reset, now, base);
```

After `on_update(&read(&shared));` (the one before `sleep_ticking(delay, …)`), add:

```rust
            if let Some(path) = &log_path {
                let _ = log::append(path, &log_line(&outcome, &read(&shared), delay));
            }
```

- [ ] **Step 6: Wire `main.rs`**

- Manage `Arc<Mutex<Settings>>`: `let settings = Arc::new(Mutex::new(initial)); app.manage(settings.clone());` and change every `app.state::<Mutex<settings::Settings>>()` to `app.state::<Arc<Mutex<settings::Settings>>>()` (also in `tray.rs` `lock_settings`: `app.state::<Arc<Mutex<Settings>>>()`, add `use std::sync::Arc;` there).
- Startup line: after managing state, `log::write(app.handle(), &format!("startup v{}", app.package_info().version));` (import `claude_usage_monitor::log`).
- Call `poll::run(shared, settings, log::path(app.handle()), move |snapshot| { … })`.
- In `notify_thresholds`, after each `.show()`: `log::write(app, &format!("alert {} {} at {}%", alert.key, alert.level, alert.percent.round() as i64));`.

- [ ] **Step 7: Run tests, lint, commit**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings && cargo test --manifest-path src-tauri/Cargo.toml`
Expected: `67 passed` (61 + 3 log + 3 poll).

```bash
git add src-tauri/src/lib.rs src-tauri/src/log.rs src-tauri/src/poll.rs src-tauri/src/main.rs src-tauri/src/tray.rs
git -c commit.gpgsign=false commit -m "feat: local log with rotation and configurable poll interval" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 4: Menu — Popover toggles, level/interval radios, test notification, open log, `settings` event

**Files:**
- Modify: `src-tauri/src/tray.rs`, `src-tauri/src/main.rs`

**Interfaces:**
- Consumes: `settings::{KEYS, SESSION_LEVEL_PRESETS, WEEKLY_LEVEL_PRESETS, INTERVAL_PRESETS, format_levels, parse_levels, PopoverSettings}`, `log::{write, path, append}`, `tauri_plugin_opener::reveal_item_in_dir`, `NotificationExt`.
- Produces: `pub enum MenuAction { Open, Quit, Toggle(String), Levels { key: String, levels: Vec<u8> }, Interval(u64), TestNotification, OpenLog }`, `pub fn parse_menu_id(id: &str) -> Option<MenuAction>`, `pub fn radio_id_levels(key, levels) -> String`, `pub fn radio_id_interval(secs) -> String`; command `get_settings() -> PopoverSettings`; event `settings` (payload `PopoverSettings`) emitted after every settings change.

- [ ] **Step 1: Failing tests** (in `tray.rs` `mod tests`)

```rust
    #[test]
    fn menu_ids_round_trip() {
        assert_eq!(radio_id_levels("session", &[80, 95]), "levels:session:80,95");
        assert_eq!(radio_id_interval(300), "interval:300");
        assert!(matches!(parse_menu_id("open"), Some(MenuAction::Open)));
        assert!(matches!(parse_menu_id("quit"), Some(MenuAction::Quit)));
        assert!(matches!(parse_menu_id("set:glyph"), Some(MenuAction::Toggle(k)) if k == "glyph"));
        assert!(matches!(
            parse_menu_id("levels:weekly:80,95"),
            Some(MenuAction::Levels { key, levels }) if key == "weekly" && levels == vec![80, 95]
        ));
        assert!(matches!(parse_menu_id("interval:600"), Some(MenuAction::Interval(600))));
        assert!(matches!(parse_menu_id("test-notification"), Some(MenuAction::TestNotification)));
        assert!(matches!(parse_menu_id("open-log"), Some(MenuAction::OpenLog)));
        assert!(parse_menu_id("levels:weekly:x").is_none());
        assert!(parse_menu_id("interval:abc").is_none());
        assert!(parse_menu_id("bogus").is_none());
    }
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml tray 2>&1 | grep -E 'cannot find' | head -2`
Expected: `cannot find function `parse_menu_id``.

- [ ] **Step 2: Implement in `tray.rs`**

Imports: add `use crate::log;`, `use crate::settings::{format_levels, parse_levels, PopoverSettings, INTERVAL_PRESETS, SESSION_LEVEL_PRESETS, WEEKLY_LEVEL_PRESETS};`, `use tauri::Emitter;`, `use tauri_plugin_notification::NotificationExt;`.

Types and parsing:

```rust
#[derive(Debug, PartialEq)]
pub enum MenuAction {
    Open,
    Quit,
    Toggle(String),
    Levels { key: String, levels: Vec<u8> },
    Interval(u64),
    TestNotification,
    OpenLog,
}

pub fn radio_id_levels(key: &str, levels: &[u8]) -> String {
    let joined = levels.iter().map(u8::to_string).collect::<Vec<_>>().join(",");
    format!("levels:{key}:{joined}")
}

pub fn radio_id_interval(secs: u64) -> String {
    format!("interval:{secs}")
}

pub fn parse_menu_id(id: &str) -> Option<MenuAction> {
    match id {
        "open" => return Some(MenuAction::Open),
        "quit" => return Some(MenuAction::Quit),
        "test-notification" => return Some(MenuAction::TestNotification),
        "open-log" => return Some(MenuAction::OpenLog),
        _ => {}
    }
    if let Some(key) = id.strip_prefix("set:") {
        return Some(MenuAction::Toggle(key.to_string()));
    }
    if let Some(rest) = id.strip_prefix("levels:") {
        let (key, raw) = rest.split_once(':')?;
        return parse_levels(raw).map(|levels| MenuAction::Levels {
            key: key.to_string(),
            levels,
        });
    }
    if let Some(raw) = id.strip_prefix("interval:") {
        return raw.parse().ok().map(MenuAction::Interval);
    }
    None
}
```

Labels for the new check groups:

```rust
const POPOVER_LABELS: [(&str, &str); 3] = [
    ("show_time_ticks", "Time ticks"),
    ("show_elapsed_marker", "Elapsed marker"),
    ("show_threshold_marks", "Threshold marks"),
];
```

A radio-submenu helper next to `check_submenu` (items keyed by full id in the same `MenuItems` map):

```rust
fn radio_submenu(
    app: &AppHandle,
    text: &str,
    entries: &[(String, String, bool)], // (id, label, checked)
    items: &mut HashMap<String, CheckMenuItem<Wry>>,
) -> tauri::Result<tauri::menu::Submenu<Wry>> {
    let mut submenu = SubmenuBuilder::new(app, text);
    for (id, label, checked) in entries {
        let item = CheckMenuItem::with_id(app, id.clone(), label.as_str(), true, *checked, None::<&str>)?;
        submenu = submenu.item(&item);
        items.insert(id.clone(), item);
    }
    submenu.build()
}

fn level_entries(key: &str, presets: &[&[u8]], current: &[u8]) -> Vec<(String, String, bool)> {
    presets
        .iter()
        .map(|p| (radio_id_levels(key, p), format_levels(p), *p == current))
        .collect()
}

fn interval_entries(current: u64) -> Vec<(String, String, bool)> {
    INTERVAL_PRESETS
        .iter()
        .map(|&s| (radio_id_interval(s), format!("{} min", s / 60), s == current))
        .collect()
}
```

Rebuild the menu in `setup` (replace from `let mut items = HashMap::new();` through `.build()?;` of the main menu):

```rust
    let mut items = HashMap::new();
    let display = check_submenu(app, "Menu bar", &DISPLAY_LABELS, &current, &mut items)?;
    let popover = check_submenu(app, "Popover", &POPOVER_LABELS, &current, &mut items)?;

    let session_levels = radio_submenu(
        app,
        "Session levels",
        &level_entries("session", &SESSION_LEVEL_PRESETS, &current.session_levels),
        &mut items,
    )?;
    let weekly_levels = radio_submenu(
        app,
        "Weekly levels",
        &level_entries("weekly", &WEEKLY_LEVEL_PRESETS, &current.weekly_levels),
        &mut items,
    )?;
    let mut alerts_menu = SubmenuBuilder::new(app, "Alerts");
    for (key, label) in ALERT_LABELS {
        let item = CheckMenuItem::with_id(app, format!("set:{key}"), label, true, current.get(key), None::<&str>)?;
        alerts_menu = alerts_menu.item(&item);
        items.insert(key.to_string(), item);
    }
    let test_item = MenuItemBuilder::with_id("test-notification", "Send test notification").build(app)?;
    let alerts_menu = alerts_menu
        .separator()
        .item(&session_levels)
        .item(&weekly_levels)
        .separator()
        .item(&test_item)
        .build()?;

    let interval = radio_submenu(app, "Check every", &interval_entries(current.poll_interval_secs), &mut items)?;
    let open_log = MenuItemBuilder::with_id("open-log", "Open log").build(app)?;
    let help = SubmenuBuilder::new(app, "Help").item(&open_log).build()?;
    app.manage(MenuItems(items));

    let menu = MenuBuilder::new(app)
        .item(&open)
        .item(&display)
        .item(&popover)
        .item(&alerts_menu)
        .item(&interval)
        .item(&help)
        .separator()
        .item(&quit)
        .build()?;
```

Note `ALERT_LABELS` items are keyed by bare key (`alert_session`) in `MenuItems` as before, while radio items are keyed by their full id.

Event handler:

```rust
        .on_menu_event(|app, event| match parse_menu_id(event.id().as_ref()) {
            Some(MenuAction::Open) => {
                let _ = tauri_plugin_opener::open_url(USAGE_URL, None::<&str>);
            }
            Some(MenuAction::Quit) => app.exit(0),
            Some(MenuAction::Toggle(key)) => on_setting_toggled(app, &key),
            Some(MenuAction::Levels { key, levels }) => on_levels_chosen(app, &key, &levels),
            Some(MenuAction::Interval(secs)) => on_interval_chosen(app, secs),
            Some(MenuAction::TestNotification) => send_test_notification(app),
            Some(MenuAction::OpenLog) => open_log(app),
            None => {}
        })
```

Handlers (replace `on_setting_toggled`'s tail with the shared `after_settings_change`):

```rust
/// Re-syncs every check/radio mark, persists, logs, notifies the popover, refreshes the title.
fn after_settings_change(app: &AppHandle, updated: &Settings, log_text: &str) {
    if let Some(items) = app.try_state::<MenuItems>() {
        for k in KEYS {
            if let Some(item) = items.0.get(k) {
                let _ = item.set_checked(updated.get(k));
            }
        }
        for p in SESSION_LEVEL_PRESETS {
            if let Some(item) = items.0.get(&radio_id_levels("session", p)) {
                let _ = item.set_checked(p == updated.session_levels.as_slice());
            }
        }
        for p in WEEKLY_LEVEL_PRESETS {
            if let Some(item) = items.0.get(&radio_id_levels("weekly", p)) {
                let _ = item.set_checked(p == updated.weekly_levels.as_slice());
            }
        }
        for s in INTERVAL_PRESETS {
            if let Some(item) = items.0.get(&radio_id_interval(s)) {
                let _ = item.set_checked(s == updated.poll_interval_secs);
            }
        }
    }
    if let Some(path) = settings::path(app) {
        // The in-memory value already applies; a failed write only loses persistence.
        let _ = settings::save(&path, updated);
    }
    log::write(app, log_text);
    let _ = app.emit("settings", PopoverSettings::from(updated));
    refresh_title(app, &poll::read(&app.state::<poll::Shared>()));
}

fn on_setting_toggled(app: &AppHandle, key: &str) {
    let updated = {
        let mut s = lock_settings(app);
        s.toggle(key);
        s.clone()
    };
    after_settings_change(app, &updated, &format!("settings {key}={}", updated.get(key)));
}

fn on_levels_chosen(app: &AppHandle, key: &str, levels: &[u8]) {
    let updated = {
        let mut s = lock_settings(app);
        s.set_levels(key, levels);
        s.clone()
    };
    after_settings_change(app, &updated, &format!("settings {key}_levels={}", format_levels(updated.levels(key))));
}

fn on_interval_chosen(app: &AppHandle, secs: u64) {
    let updated = {
        let mut s = lock_settings(app);
        s.set_poll_interval(secs);
        s.clone()
    };
    after_settings_change(app, &updated, &format!("settings poll_interval_secs={}", updated.poll_interval_secs));
}

fn send_test_notification(app: &AppHandle) {
    let _ = app
        .notification()
        .builder()
        .title("Claude usage: test")
        .body("Notifications are working")
        .show();
    log::write(app, "test notification");
}

fn open_log(app: &AppHandle) {
    let Some(path) = log::path(app) else {
        return;
    };
    if !path.exists() {
        let _ = log::append(&path, "log created from Help → Open log");
    }
    let _ = tauri_plugin_opener::reveal_item_in_dir(&path);
}
```

- [ ] **Step 3: `main.rs` — `get_settings` command**

```rust
#[tauri::command]
fn get_settings(state: State<'_, Arc<Mutex<settings::Settings>>>) -> Result<settings::PopoverSettings, String> {
    let s = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    Ok(settings::PopoverSettings::from(&*s))
}
```

Add it to `generate_handler![...]`.

- [ ] **Step 4: Build, lint, test, manual**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings && cargo test --manifest-path src-tauri/Cargo.toml`
Expected: `68 passed`. If `reveal_item_in_dir` has a different signature in opener 2.5 (`reveal_item_in_dir<P: AsRef<Path>>(p: P) -> Result<()>` is expected), check `~/.cargo/registry/src/*/tauri-plugin-opener-2.5.*/src/` and adapt.

Run `pnpm tauri dev` (background, ~3 min): confirm no panic, title renders; via `osascript` list the right-click menu: `Open usage page`, `Menu bar`, `Popover`, `Alerts`, `Check every`, `Help`, `Quit`. Confirm `~/Library/Application Support/com.matteo.claude-usage-monitor/claude-usage-monitor.log` gains a `startup` line and a `poll …` line. Kill the process.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/tray.rs src-tauri/src/main.rs
git -c commit.gpgsign=false commit -m "feat: tray menu presets for levels and interval, test notification, open log" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 5: Popover overlays and threshold marks

**Files:**
- Modify: `src/lib/quota.ts`, `src/lib/ipc.ts`, `src/lib/format.ts`, `src/App.tsx`, `src/app.css`, `test/format.test.ts`

**Interfaces:**
- Consumes: command `get_settings` → `PopoverSettings`; event `settings`.
- Produces: `PopoverSettings` type, `getSettings()`, `onSettings(cb)`, `markClass(level, levels)`.

- [ ] **Step 1: Failing test** (`test/format.test.ts`)

```ts
import { barColor, clock, countdown, elapsedPct, markClass, relative } from '@/lib/format';
// …
describe('markClass', () => {
  it('colours the top level red and the rest amber', () => {
    expect(markClass(95, [80, 95])).toBe('over');
    expect(markClass(80, [80, 95])).toBe('warn');
    expect(markClass(95, [95])).toBe('over');
  });
});
```

Run: `pnpm test:run 2>&1 | tail -3` — Expected: fails, `markClass` not exported.

- [ ] **Step 2: `format.ts`**

```ts
export function markClass(level: number, levels: number[]): BarColor {
  return level >= Math.max(...levels) ? 'over' : 'warn';
}
```

- [ ] **Step 3: `quota.ts`** — add:

```ts
export type PopoverSettings = {
  session_levels: number[];
  weekly_levels: number[];
  show_time_ticks: boolean;
  show_elapsed_marker: boolean;
  show_threshold_marks: boolean;
};

export const DEFAULT_POPOVER_SETTINGS: PopoverSettings = {
  session_levels: [80, 95],
  weekly_levels: [95],
  show_time_ticks: true,
  show_elapsed_marker: true,
  show_threshold_marks: true,
};
```

- [ ] **Step 4: `ipc.ts`** — import `DEFAULT_POPOVER_SETTINGS, PopoverSettings` and add:

```ts
export async function getSettings(): Promise<PopoverSettings> {
  return inTauri ? invoke<PopoverSettings>('get_settings') : DEFAULT_POPOVER_SETTINGS;
}

export async function onSettings(cb: (s: PopoverSettings) => void): Promise<() => void> {
  if (!inTauri) return () => {};
  return listen<PopoverSettings>('settings', (event) => cb(event.payload));
}
```

- [ ] **Step 5: `App.tsx`**

Import `markClass`, `getSettings`, `onSettings`, `DEFAULT_POPOVER_SETTINGS`, `PopoverSettings`. Add state `const [settings, setSettings] = useState<PopoverSettings>(DEFAULT_POPOVER_SETTINGS);` and an effect mirroring the snapshot one (cancelled flag) that calls `getSettings().then(...)` and `onSettings(setSettings)`.

`QuotaCard` becomes:

```tsx
function levelsFor(q: Quota, s: PopoverSettings): number[] {
  if (q.key === 'session') return s.session_levels;
  if (q.key === 'weekly') return s.weekly_levels;
  return [];
}

function QuotaCard({ q, now, settings }: { q: Quota; now: number; settings: PopoverSettings }) {
  const elapsed = elapsedPct(q, now);
  const color = barColor(q.percent, elapsed);
  const tickCount = q.period_secs === SESSION_SECS ? 5 : 7;
  const ticks = Array.from({ length: tickCount - 1 }, (_, i) => ((i + 1) / tickCount) * 100);
  const levels = levelsFor(q, settings);
  return (
    <section className={`card ${color}`}>
      <header>
        <span className="label">{q.label}</span>
        <span className="pct">{Math.round(q.percent)}%</span>
      </header>
      <div className="bar">
        <div className="fill" style={{ width: `${Math.min(100, q.percent)}%` }} />
        {settings.show_time_ticks &&
          ticks.map((left) => <i key={left} className="tick" style={{ left: `${left}%` }} />)}
        {settings.show_elapsed_marker && <div className="marker" style={{ left: `${elapsed}%` }} />}
        {settings.show_threshold_marks &&
          levels.map((level) => (
            <i key={level} className={`mark ${markClass(level, levels)}`} style={{ left: `${level}%` }} />
          ))}
      </div>
      <p className="reset">
        Resets in {countdown(q.resets_at - now)} · {clock(q.resets_at, now)}
      </p>
    </section>
  );
}
```

and the render passes `settings={settings}`.

- [ ] **Step 6: `app.css`** — add:

```css
.mark {
  position: absolute;
  top: 100%;
  margin-top: 1px;
  width: 2px;
  height: 3px;
  transform: translateX(-50%);
}

.mark.warn {
  background: var(--warn);
}

.mark.over {
  background: var(--over);
}
```

- [ ] **Step 7: Verify**

Run: `pnpm check && pnpm typecheck && pnpm test:run` — Expected: clean, `13 passed`.
Run `pnpm dev`, open http://localhost:5173: session bar shows amber mark at 80% and red at 95% below the bar; weekly shows red at 95%; Fable bar shows none. Stop the server.

- [ ] **Step 8: Commit**

```bash
git add src/lib/quota.ts src/lib/ipc.ts src/lib/format.ts src/App.tsx src/app.css test/format.test.ts
git -c commit.gpgsign=false commit -m "feat: popover overlay toggles and threshold marks" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 6: Docs, changeset, PR

**Files:**
- Modify: `README.md`, `CLAUDE.md`
- Create: `.changeset/config-and-log.md`

- [ ] **Step 1: README** — replace the "Right-click the menu bar item → **Menu bar** …" paragraph and the "**Alerts.**" paragraph with a "## Configuration" section placed after "How it works":

```markdown
## Configuration

Everything lives in the tray right-click menu and persists in
`~/Library/Application Support/com.matteo.claude-usage-monitor/settings.json`:

- **Menu bar** — Session, Weekly, Glyphs (`◷` / `▦`), Percent, Remaining time. At least one quota
  and one of Percent/Remaining always stay on.
- **Popover** — Time ticks (hour/day divisions), Elapsed marker (the white line: how far through
  the reset window you are), Threshold marks (coloured ticks under the bar at each alert level).
- **Alerts** — per-quota on/off; **Session levels** `80/95` · `50/80/95` · `90/95`; **Weekly
  levels** `95` · `80/95` · `90`; **Send test notification**. The highest level always fires and
  shows `⚠` in the menu bar; lower levels fire only while usage is ahead of the elapsed time. One
  notification per level per reset window.
- **Check every** — 3 / 5 / 10 / 15 minutes. Below 120 s the API rate-limits, so that is the floor.
- **Help → Open log** — reveals `claude-usage-monitor.log` (poll results, alerts, settings
  changes; never your token). Rotates at 1 MB to `.log.1`. Attach it when reporting a problem.

Hand-editing `settings.json` is fine: `session_levels`/`weekly_levels` accept any 1–100 values,
`poll_interval_secs` is clamped to 120–900.
```

- [ ] **Step 2: CLAUDE.md** — Layout: add `  src/log.rs               capped local log (never the token)`. Data Source section: change "base poll 180s" to "base poll 180s (user-configurable 120–900s)". Status paragraph → "v1.3 (configuration, bar overlays, log) implemented; released via the Changesets pipeline." plus spec path `docs/superpowers/specs/2026-09-17-config-and-log-design.md`.

- [ ] **Step 3: Changeset** `.changeset/config-and-log.md`:

```markdown
---
"claude-usage-monitor": minor
---

Configurable alert levels (presets), poll interval (3–15 min), and popover overlays from the tray menu; threshold marks on the popover bars; "Send test notification"; a local log with Help → Open log.
```

- [ ] **Step 4: Verify, commit, push, PR**

Run: `pnpm verify 2>&1 | tail -3 && pnpm changeset status` — Expected: green, 13 vitest, 68 cargo; `minor` bump.

```bash
git add README.md CLAUDE.md .changeset/config-and-log.md
git -c commit.gpgsign=false commit -m "docs: describe configuration menu, overlays and the local log" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
git push -u origin feat/config-and-log
gh pr create --base main --head feat/config-and-log --title "feat: configurable levels, interval, popover overlays and local log" --body-file - <<'EOF'
## Summary

- Settings: `session_levels`, `weekly_levels`, `poll_interval_secs` (120–900), three popover overlay toggles; `Settings` is now `Clone` behind `Arc<Mutex<_>>` shared with the poll thread.
- Alerts read levels from settings; `top = max(levels)` is the unconditional level and the `⚠` marker.
- Tray menu: **Popover** toggles, **Alerts → Session/Weekly levels** presets, **Send test notification**, **Check every** presets, **Help → Open log**.
- `log.rs`: `app_data_dir/claude-usage-monitor.log`, RFC3339 lines, 1 MB rotation; poll cycles, alerts, settings changes and startup are logged (never the token).
- Popover draws threshold marks (amber/red) and honors the overlay toggles live via `get_settings` + `settings` event.

Spec: `docs/superpowers/specs/2026-09-17-config-and-log-design.md`

## Test plan

- [x] `pnpm verify` (13 vitest, 68 cargo)
- [x] `tauri dev`: menus render, log file gets startup + poll lines
- [ ] Human: Send test notification shows a banner; Help → Open log reveals the file; presets persist across relaunch; overlays toggle live

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
gh pr checks --watch --interval 30
```

Expected: CI `verify-and-build` passes.
