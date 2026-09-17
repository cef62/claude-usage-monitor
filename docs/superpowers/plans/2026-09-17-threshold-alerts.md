# Threshold Alerts (v1.2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** macOS notifications when session usage crosses 80%/95% or weekly crosses 95% (once per reset window, 80% only when ahead of the clock), a `⚠` title marker at ≥95%, and per-quota on/off in the tray menu.

**Architecture:** `settings.rs` gains `alert_session`/`alert_weekly`. New pure module `alerts.rs` decides which alerts are due from a `Snapshot`'s quotas and an in-memory `AlertState`. `tray.rs` adds an "Alerts" check submenu and the `⚠` prefix. `main.rs` registers `tauri-plugin-notification`, manages `AlertState`, and shows notifications from the poll callback.

**Tech Stack:** Rust, Tauri 2.11, `tauri-plugin-notification` 2.4. No frontend changes.

**Spec:** `docs/superpowers/specs/2026-09-17-threshold-alerts-design.md`

## Global Constraints

- Branch `feat/alerts` (exists, off `main` at `b4afa7d`). Never commit to `main`.
- Levels: session `[80, 95]`, weekly `[95]`, marker level `95`. Alert once per `(key, resets_at, level)`; a level below 95 fires only when `percent > elapsed_pct`. One alert per quota per evaluation (highest due level); lower levels are marked fired with it. Per-model quotas never alert.
- `Settings` new fields `alert_session`, `alert_weekly` default `true`, no pair invariant; `KEYS` = `["session", "weekly", "glyph", "percent", "remaining", "alert_session", "alert_weekly"]`.
- Title marker: ` ⚠ ` after the leading space for `Ok`/`RateLimited` when any alert-enabled quota is ≥ 95; status strings unchanged.
- Notification title `Claude usage: {Session|Weekly} {NN}%`, body `Resets in {countdown}`. Capability adds exactly `notification:default`.
- No `unwrap`/`expect` in `settings.rs`, `alerts.rs`, `tray.rs`, setup code. `cargo clippy -- -D warnings` clean, no `#[allow]`. Only new dependency: `tauri-plugin-notification = "2.4"`.
- `pnpm verify` green; every user-visible change ships with a changeset.
- Commit messages: conventional prefix, trailer on its own paragraph: `git -c commit.gpgsign=false commit -m "<subject>" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"`.

---

### Task 1: Settings — alert flags

**Files:**
- Modify: `src-tauri/src/settings.rs`

**Interfaces:**
- Produces: `Settings { …, pub alert_session: bool, pub alert_weekly: bool }`; `KEYS: [&str; 7]`; `get`/`toggle` accept `"alert_session"` and `"alert_weekly"` (no invariant).

- [ ] **Step 1: Add the failing tests** (inside `mod tests`)

```rust
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
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml settings 2>&1 | grep -E 'no field|error\[' | head -3`
Expected: `no field `alert_session``.

- [ ] **Step 3: Implement**

In `Settings`, add after `remaining: bool,`:

```rust
    pub alert_session: bool,
    pub alert_weekly: bool,
```

In `Default::default`, add `alert_session: true, alert_weekly: true,`.

Replace `KEYS`:

```rust
pub const KEYS: [&str; 7] = [
    "session",
    "weekly",
    "glyph",
    "percent",
    "remaining",
    "alert_session",
    "alert_weekly",
];
```

In `get`, add arms `"alert_session" => self.alert_session,` and `"alert_weekly" => self.alert_weekly,` before `_ => false`.

In `toggle`, the `partner_on` match gains `"alert_session" | "alert_weekly" => true,` (next to `"glyph" => true,`), and the flip match gains:

```rust
            "alert_session" => self.alert_session = !self.alert_session,
            "alert_weekly" => self.alert_weekly = !self.alert_weekly,
```

Update the `toggle` doc comment to mention that `glyph` and the alert keys have no partner.

- [ ] **Step 4: Run tests**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml && cargo test --manifest-path src-tauri/Cargo.toml`
Expected: `35 passed` (33 + 2). The existing `default_is_all_on` test iterates `KEYS`, so it now covers the new fields too.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/settings.rs
git -c commit.gpgsign=false commit -m "feat: add per-quota alert settings" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 2: `alerts.rs` — pure threshold state machine

**Files:**
- Create: `src-tauri/src/alerts.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `settings::Settings` (with `alert_session`, `alert_weekly`), `usage::Quota { key, label, percent, resets_at, period_secs }`.
- Produces: `SESSION_LEVELS`, `WEEKLY_LEVELS`, `MARKER_LEVEL`, `AlertState` (`Default`), `Alert { key, label, level, percent, resets_at }`, `elapsed_pct(&Quota, i64) -> f64`, `enabled(&Settings, &str) -> bool`, `evaluate(&mut AlertState, &[Quota], &Settings, i64) -> Vec<Alert>`, `marker(&[Quota], &Settings) -> bool`.

- [ ] **Step 1: Declare the module**

`src-tauri/src/lib.rs`:

```rust
//! Library target so integration tests and the binary share the same modules.
pub mod alerts;
pub mod poll;
pub mod settings;
pub mod tray;
pub mod usage;
```

- [ ] **Step 2: Write the failing tests**

Create `src-tauri/src/alerts.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{SESSION_SECS, WEEKLY_SECS};

    const NOW: i64 = 1_789_588_800;

    fn quota(key: &str, percent: f64, secs_left: i64, period: u64) -> Quota {
        Quota {
            key: key.to_string(),
            label: key.to_string(),
            percent,
            resets_at: NOW + secs_left,
            period_secs: period,
        }
    }

    fn session(percent: f64, secs_left: i64) -> Quota {
        quota("session", percent, secs_left, SESSION_SECS)
    }

    fn on() -> Settings {
        Settings::default()
    }

    #[test]
    fn elapsed_pct_tracks_the_window() {
        assert!((elapsed_pct(&session(0.0, 7200), NOW) - 60.0).abs() < 0.01);
        assert_eq!(elapsed_pct(&session(0.0, 2 * SESSION_SECS as i64), NOW), 0.0);
        assert_eq!(elapsed_pct(&session(0.0, -10), NOW), 100.0);
    }

    #[test]
    fn fires_80_when_ahead_of_clock() {
        let mut st = AlertState::default();
        let alerts = evaluate(&mut st, &[session(85.0, 7200)], &on(), NOW);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].level, 80);
        assert_eq!(alerts[0].label, "Session");
        assert_eq!(alerts[0].percent, 85.0);
    }

    #[test]
    fn silent_below_every_level() {
        let mut st = AlertState::default();
        assert!(evaluate(&mut st, &[session(30.0, 4 * 3600)], &on(), NOW).is_empty());
    }

    #[test]
    fn time_aware_suppresses_80_behind_the_clock() {
        let mut st = AlertState::default();
        // 30 minutes left → 90% elapsed; 82% used is behind the clock.
        assert!(evaluate(&mut st, &[session(82.0, 1800)], &on(), NOW).is_empty());
    }

    #[test]
    fn fires_95_once_regardless_of_clock() {
        let mut st = AlertState::default();
        let first = evaluate(&mut st, &[session(96.0, 1800)], &on(), NOW);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].level, 95);
        // Same window (same resets_at), later poll.
        assert!(evaluate(&mut st, &[session(97.0, 1800)], &on(), NOW + 100).is_empty());
    }

    #[test]
    fn highest_due_level_marks_lower_levels_fired() {
        let mut st = AlertState::default();
        let first = evaluate(&mut st, &[session(96.0, 7200)], &on(), NOW);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].level, 95);
        // Usage drops back into the 80 band inside the same window: nothing new.
        assert!(evaluate(&mut st, &[session(85.0, 7200)], &on(), NOW + 200).is_empty());
    }

    #[test]
    fn new_window_rearms() {
        let mut st = AlertState::default();
        assert_eq!(evaluate(&mut st, &[session(85.0, 7200)], &on(), NOW).len(), 1);
        let later = NOW + 8000;
        let next = Quota {
            resets_at: later + 7200,
            ..session(85.0, 0)
        };
        assert_eq!(evaluate(&mut st, &[next], &on(), later).len(), 1);
    }

    #[test]
    fn disabled_quota_is_silent() {
        let mut st = AlertState::default();
        let mut s = on();
        assert!(s.toggle("alert_session"));
        assert!(evaluate(&mut st, &[session(96.0, 7200)], &s, NOW).is_empty());
    }

    #[test]
    fn weekly_uses_its_own_levels() {
        let mut st = AlertState::default();
        let weekly = |p: f64| quota("weekly", p, 3 * 86400, WEEKLY_SECS);
        assert!(evaluate(&mut st, &[weekly(90.0)], &on(), NOW).is_empty());
        let alerts = evaluate(&mut st, &[weekly(96.0)], &on(), NOW);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].level, 95);
        assert_eq!(alerts[0].label, "Weekly");
    }

    #[test]
    fn scoped_quotas_are_ignored() {
        let mut st = AlertState::default();
        let scoped = quota("weekly:fable", 99.0, 3 * 86400, WEEKLY_SECS);
        assert!(evaluate(&mut st, &[scoped], &on(), NOW).is_empty());
    }

    #[test]
    fn prunes_entries_from_old_windows() {
        let mut st = AlertState::default();
        st.fired.insert(("session".to_string(), NOW - 2 * 86400, 80));
        evaluate(&mut st, &[], &on(), NOW);
        assert!(st.fired.is_empty());
    }

    #[test]
    fn marker_follows_alert_settings_not_display_settings() {
        let quotas = [session(95.0, 7200), quota("weekly", 40.0, 86400, WEEKLY_SECS)];
        assert!(marker(&quotas, &on()));
        let mut off = on();
        assert!(off.toggle("alert_session"));
        assert!(!marker(&quotas, &off));
        let mut hidden = on();
        assert!(hidden.toggle("weekly"));
        let weekly_high = [quota("weekly", 96.0, 86400, WEEKLY_SECS)];
        assert!(marker(&weekly_high, &hidden));
    }
}
```

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml alerts 2>&1 | grep -E 'cannot find' | head -3`
Expected: `cannot find function `evaluate``, `cannot find type `AlertState``.

- [ ] **Step 4: Implement** (above the tests)

```rust
//! Threshold alerts: pure decision logic. Delivery (notifications) lives in main.rs.

use crate::settings::Settings;
use crate::usage::Quota;
use std::collections::HashSet;

pub const SESSION_LEVELS: [u8; 2] = [80, 95];
pub const WEEKLY_LEVELS: [u8; 1] = [95];
/// At or above this level the title shows `⚠` and time-aware suppression no longer applies.
pub const MARKER_LEVEL: u8 = 95;
const PRUNE_AFTER_SECS: i64 = 86_400;

/// Levels already announced, keyed by (quota key, reset window, level). In-memory only.
#[derive(Default)]
pub struct AlertState {
    fired: HashSet<(String, i64, u8)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Alert {
    pub key: String,
    pub label: String,
    pub level: u8,
    pub percent: f64,
    pub resets_at: i64,
}

/// Fraction of the reset window already elapsed, 0..=100.
pub fn elapsed_pct(q: &Quota, now: i64) -> f64 {
    let period = q.period_secs as f64;
    if period <= 0.0 {
        return 100.0;
    }
    let elapsed = period - (q.resets_at - now) as f64;
    (elapsed / period * 100.0).clamp(0.0, 100.0)
}

pub fn enabled(settings: &Settings, key: &str) -> bool {
    match key {
        "session" => settings.alert_session,
        "weekly" => settings.alert_weekly,
        _ => false,
    }
}

fn levels(key: &str) -> &'static [u8] {
    match key {
        "session" => &SESSION_LEVELS,
        "weekly" => &WEEKLY_LEVELS,
        _ => &[],
    }
}

fn label(key: &str) -> &'static str {
    match key {
        "session" => "Session",
        "weekly" => "Weekly",
        _ => "Quota",
    }
}

/// Returns at most one alert per enabled quota: the highest level that is newly due. Lower
/// levels are recorded as fired at the same time so they never announce late.
pub fn evaluate(state: &mut AlertState, quotas: &[Quota], settings: &Settings, now: i64) -> Vec<Alert> {
    state
        .fired
        .retain(|(_, resets_at, _)| *resets_at >= now - PRUNE_AFTER_SECS);

    let mut alerts = Vec::new();
    for q in quotas.iter().filter(|q| enabled(settings, &q.key)) {
        let elapsed = elapsed_pct(q, now);
        let highest = levels(&q.key)
            .iter()
            .copied()
            .filter(|&level| q.percent >= f64::from(level))
            .filter(|&level| level >= MARKER_LEVEL || q.percent > elapsed)
            .filter(|&level| !state.fired.contains(&(q.key.clone(), q.resets_at, level)))
            .max();
        let Some(highest) = highest else {
            continue;
        };
        for &level in levels(&q.key).iter().filter(|&&l| l <= highest) {
            state.fired.insert((q.key.clone(), q.resets_at, level));
        }
        alerts.push(Alert {
            key: q.key.clone(),
            label: label(&q.key).to_string(),
            level: highest,
            percent: q.percent,
            resets_at: q.resets_at,
        });
    }
    alerts
}

/// True when any alert-enabled quota is at or above the marker level.
pub fn marker(quotas: &[Quota], settings: &Settings) -> bool {
    quotas
        .iter()
        .any(|q| enabled(settings, &q.key) && q.percent >= f64::from(MARKER_LEVEL))
}
```

- [ ] **Step 5: Run tests**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml && cargo test --manifest-path src-tauri/Cargo.toml`
Expected: `47 passed` (35 + 12).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/alerts.rs
git -c commit.gpgsign=false commit -m "feat: add threshold alert state machine" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 3: Title marker, Alerts submenu, notification delivery

**Files:**
- Modify: `src-tauri/src/tray.rs`, `src-tauri/src/main.rs`, `src-tauri/Cargo.toml`, `src-tauri/capabilities/default.json`

**Interfaces:**
- Consumes: `alerts::{AlertState, evaluate, marker}`, `settings::Settings`, `tray::countdown`.
- Produces: `⚠` prefix in `title`; menu ids `set:alert_session`, `set:alert_weekly`; managed `Mutex<AlertState>`; notifications from the poll callback.

- [ ] **Step 1: Failing title tests** (in `tray.rs` `mod tests`)

```rust
    #[test]
    fn title_marks_alerting_quota_at_95() {
        let s = snapshot(
            Status::Ok,
            vec![
                quota("session", 96.0, 2 * 3600 + 13 * 60, SESSION_SECS),
                quota("weekly", 64.0, 3 * 86400 + 4 * 3600 + 20 * 60, WEEKLY_SECS),
            ],
        );
        assert_eq!(
            title(&s, NOW, &Settings::default()),
            " ⚠ ◷ 96% ↻2h13m  ·  ▦ 64% ↻3d4h"
        );
        assert_eq!(
            title(&snapshot(Status::RateLimited { until: NOW + 900 }, s.quotas.clone()), NOW, &Settings::default()),
            " ⚠ ◷ 96% ↻2h13m  ·  ▦ 64% ↻3d4h (429)"
        );
        assert_eq!(
            title(&s, NOW, &with(&["alert_session"])),
            " ◷ 96% ↻2h13m  ·  ▦ 64% ↻3d4h"
        );
    }

    #[test]
    fn title_marker_ignores_status_strings() {
        let s = snapshot(Status::AuthExpired, vec![quota("session", 99.0, 600, SESSION_SECS)]);
        assert_eq!(title(&s, NOW, &Settings::default()), " ◷ ! login");
    }
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml tray 2>&1 | grep -E 'panicked|left:|right:' | head -4`
Expected: the first new test fails on the missing `⚠ ` prefix (assertion `left`/`right` mismatch).

- [ ] **Step 2: Title marker**

In `tray.rs` add `use crate::alerts;` and change the `numbers` closure in `title` to:

```rust
    let numbers = || {
        let mut halves = Vec::with_capacity(2);
        if settings.session {
            halves.push(half(s, "session", SESSION_GLYPH, now, settings));
        }
        if settings.weekly {
            halves.push(half(s, "weekly", WEEKLY_GLYPH, now, settings));
        }
        let warn = if alerts::marker(&s.quotas, settings) { "⚠ " } else { "" };
        format!(" {warn}{}", halves.join(SEPARATOR))
    };
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml tray`
Expected: `10 passed`.

- [ ] **Step 3: Alerts submenu**

Replace `LABELS` with two tables and a helper:

```rust
const DISPLAY_LABELS: [(&str, &str); 5] = [
    ("session", "Session"),
    ("weekly", "Weekly"),
    ("glyph", "Glyphs"),
    ("percent", "Percent"),
    ("remaining", "Remaining time"),
];

const ALERT_LABELS: [(&str, &str); 2] = [("alert_session", "Session"), ("alert_weekly", "Weekly")];

fn check_submenu(
    app: &AppHandle,
    text: &str,
    labels: &[(&str, &str)],
    current: &Settings,
    items: &mut HashMap<String, CheckMenuItem<Wry>>,
) -> tauri::Result<tauri::menu::Submenu<Wry>> {
    let mut submenu = SubmenuBuilder::new(app, text);
    for (key, label) in labels {
        let item = CheckMenuItem::with_id(
            app,
            format!("set:{key}"),
            *label,
            true,
            current.get(key),
            None::<&str>,
        )?;
        submenu = submenu.item(&item);
        items.insert((*key).to_string(), item);
    }
    submenu.build()
}
```

In `setup`, replace the block from `let mut items = HashMap::new();` through `app.manage(MenuItems(items));` with:

```rust
    let mut items = HashMap::new();
    let display = check_submenu(app, "Menu bar", &DISPLAY_LABELS, &current, &mut items)?;
    let alerts_menu = check_submenu(app, "Alerts", &ALERT_LABELS, &current, &mut items)?;
    app.manage(MenuItems(items));
```

and the menu build to:

```rust
    let menu = MenuBuilder::new(app)
        .item(&open)
        .item(&display)
        .item(&alerts_menu)
        .separator()
        .item(&quit)
        .build()?;
```

`on_setting_toggled` already loops over `settings::KEYS` (now 7) for the re-sync, so no change there.

- [ ] **Step 4: Plugin, capability, delivery**

`src-tauri/Cargo.toml`, under `[dependencies]` after `tauri-plugin-opener`:

```toml
tauri-plugin-notification = "2.4"
```

`src-tauri/capabilities/default.json` `permissions` array — add `"notification:default"` after `"core:default"`.

`src-tauri/src/main.rs`: add imports

```rust
use claude_usage_monitor::{alerts, settings};
use tauri_plugin_notification::NotificationExt;
```

(replace the existing `use claude_usage_monitor::settings;` line), register the plugin right after the opener plugin:

```rust
        .plugin(tauri_plugin_notification::init())
```

in `setup`, after `app.manage(Mutex::new(initial));` add `app.manage(Mutex::new(alerts::AlertState::default()));`, and change the poll closure to:

```rust
            poll::run(shared, move |snapshot| {
                tray::refresh_title(&handle, snapshot);
                notify_thresholds(&handle, snapshot);
                let _ = handle.emit("usage", snapshot);
            });
```

Add this free function above `main`:

```rust
/// Shows one macOS notification per newly crossed threshold. Failures are ignored: the title
/// marker still tells the story if notifications are denied.
fn notify_thresholds(app: &AppHandle, snapshot: &Snapshot) {
    let settings = *app
        .state::<Mutex<settings::Settings>>()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = poll::now();
    let due = {
        let state = app.state::<Mutex<alerts::AlertState>>();
        let mut state = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        alerts::evaluate(&mut state, &snapshot.quotas, &settings, now)
    };
    for alert in due {
        let _ = app
            .notification()
            .builder()
            .title(format!(
                "Claude usage: {} {}%",
                alert.label,
                alert.percent.round() as i64
            ))
            .body(format!("Resets in {}", tray::countdown(alert.resets_at - now)))
            .show();
    }
}
```

- [ ] **Step 5: Build, lint, test**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings && cargo test --manifest-path src-tauri/Cargo.toml`
Expected: first build downloads the notification plugin; clippy clean; `49 passed` (47 + 2). If `NotificationExt` or the builder method names differ in plugin 2.4, check `~/.cargo/registry/src/*/tauri-plugin-notification-2.4.*/src/` and use the documented names, noting it in the report.

- [ ] **Step 6: Manual check**

Run: `pnpm tauri dev` (background, ~3 min). Expected: compiles, no panic; right-click shows `Menu bar ▸` and `Alerts ▸` (two checked items). If the real session usage is ≥ 80% and ahead of the clock, one banner `Claude usage: Session NN%` appears (macOS asks for notification permission the first time). Otherwise no banner is expected — say so. Toggle checks are human-pending if clicks cannot be driven. Kill the dev process.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/tray.rs src-tauri/src/main.rs src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/capabilities/default.json
git -c commit.gpgsign=false commit -m "feat: threshold notifications with title marker and alert toggles" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 4: Docs, changeset, PR

**Files:**
- Modify: `README.md`, `CLAUDE.md`
- Create: `.changeset/threshold-alerts.md`

- [ ] **Step 1: README** — after the "Right-click the menu bar item → **Menu bar**…" paragraph add:

```markdown
**Alerts.** A macOS notification fires when the session quota crosses 80% or 95% and when the
weekly quota crosses 95%, once per reset window. The 80% alert only fires while usage is ahead of
the elapsed time (you would hit the cap before the reset). While any quota is at 95% or more the
menu bar shows `⚠`. Turn alerts off per quota under right-click → **Alerts**. macOS asks for
notification permission the first time.
```

- [ ] **Step 2: CLAUDE.md** — Layout block: add `  src/alerts.rs            threshold alert state machine (pure)` after `src/settings.rs`. Tauri Rules: change the capabilities bullet to mention `notification:default` alongside `opener:allow-open-url`. Status paragraph: "v1.2 (threshold alerts) implemented; released via the Changesets pipeline." plus the existing spec references and `docs/superpowers/specs/2026-09-17-threshold-alerts-design.md`.

- [ ] **Step 3: Changeset** `.changeset/threshold-alerts.md`:

```markdown
---
"claude-usage-monitor": minor
---

Threshold alerts: macOS notifications at 80% and 95% session usage and 95% weekly usage, once per reset window (80% only when usage is ahead of the clock), a `⚠` menu bar marker at 95%, and per-quota on/off under right-click → Alerts.
```

- [ ] **Step 4: Verify, commit, push, PR**

Run: `pnpm verify 2>&1 | tail -3 && pnpm changeset status`
Expected: green, 12 vitest, 49 cargo; changeset lists a `minor` bump.

```bash
git add README.md CLAUDE.md .changeset/threshold-alerts.md
git -c commit.gpgsign=false commit -m "docs: describe threshold alerts" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
git push -u origin feat/alerts
gh pr create --base main --head feat/alerts --title "feat: threshold alerts" --body-file - <<'EOF'
## Summary

- `alerts.rs`: pure threshold state machine — session 80%/95%, weekly 95%, once per `(quota, reset window, level)`, 80% only when usage is ahead of the elapsed time; highest due level wins.
- macOS notifications via `tauri-plugin-notification` from the poll callback; capability `notification:default`.
- `⚠` prefix in the menu bar title while any alert-enabled quota is ≥ 95%.
- Tray right-click → **Alerts** submenu with per-quota check items (`alert_session`, `alert_weekly` in `settings.json`).

Spec: `docs/superpowers/specs/2026-09-17-threshold-alerts-design.md`

## Test plan

- [x] `pnpm verify` (12 vitest, 49 cargo)
- [x] `tauri dev`: menus render, no panic
- [ ] Human: cross 80% on a real session → one banner; toggle Alerts → Session off → no banner

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
gh pr checks --watch --interval 30
```

Expected: CI `verify-and-build` passes.
