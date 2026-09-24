# Pace forecast Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A per-quota pace forecast ("At this pace: 100% at 16:40" / "~78% at reset") on the session and weekly popover cards, plus one notification per window when a run-out is projected well before the reset.

**Architecture:** New pure Rust module `forecast.rs` computes a trailing-rate forecast from the full-resolution `History` samples once per successful poll; the result travels in `Snapshot.forecast` to the popover (rendering only) and to a new pure `alerts::run_outs` decision function. `main.rs` delivers the notification, merged into a same-quota threshold alert when both are due. A new `alert_forecast` setting gates the notification from Alerts ▸ **Run-out forecast**.

**Tech Stack:** Rust (serde), Tauri 2.11 + tauri-plugin-notification, React 19, TypeScript, Vitest, Biome.

**Spec:** `docs/superpowers/specs/2026-09-22-pace-forecast-design.md`

## Global Constraints

- Forecast only for keys `session` and `weekly`; `weekly:*` scoped quotas never get one.
- `lookback(period_secs) = period_secs / 7` (session 2571 s, weekly 86 400 s); no forecast while the window is younger than `lookback / 2`, at `percent >= 100`, or at `now >= resets_at`.
- Base point = newest sample with `window_start <= t <= now − lookback`, else `(window_start, 0.0)`.
- `Forecast` serializes as `{"kind":"runs_out","at":<i64>}` or `{"kind":"at_reset","percent":<f64>}` (`#[serde(tag = "kind", rename_all = "snake_case")]`); TS mirrors it in `src/lib/quota.ts`.
- Notification: once per (quota key, window) with `SAME_WINDOW_SECS` jitter tolerance; only when `alert_forecast` and the quota's own alert toggle are on, the forecast is `RunsOut`, `resets_at − at >= lookback`, and `percent` is below the quota's top level.
- Notification texts (relative times via `tray::countdown`): alone → title `Claude usage: {label}`, body `At this pace: 100% in ~{countdown(at − now)} · resets in {countdown(resets_at − now)}`; merged → the threshold alert's body becomes `Resets in {countdown(resets_at − now)} · at this pace 100% in ~{countdown(at − now)}`. Log line `forecast {key} 100% in {countdown(at − now)}`.
- Setting `alert_forecast: bool` default `true`, `KEYS` → 14 (after `alert_reset`), no partner, not in `PopoverSettings`; menu label `Run-out forecast` after `Notify on reset`.
- Popover line text: `At this pace: 100% at {clock(at, now)}` / `At this pace: ~{round(percent)}% at reset`; class `reset`, plus `runs-out` (colour `var(--over)`) for `runs_out`.
- No network, no new files on disk, no cadence change. No `unwrap`/`expect` outside tests.
- Every commit: `pnpm verify` green first; message ends with a blank line and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Branch `feat/pace-forecast` (exists, holds the spec).
- Setup once before Task 1: `pnpm install` (`node_modules` is missing in this checkout), then invoke the `history-driven-workflow` skill to open `history/2026-09-22-pace-forecast.md` (status `in-progress`).
- Never touch or use `.secrets/` (leaked updater key; the maintainer has been told). Build checks use `pnpm tauri build --no-bundle`, which skips updater signing.

## Review Focus

Inputs the spec implies but its test list does not pin; each has a test in the owning task.

1. **Stale snapshot after sleep or a long rate-limit** — a stored `runs_out.at` is already in the past when the popover opens; a person expects no "100% at 16:40" when it is 17:10. → `forecastText` returns `null`, the card shows no line (Task 5).
2. **Usage percent drops inside a window** (API correction, or the stored `f32` sample reads a hair above the fresh `f64` percent) → no negative rate; `AtReset { percent: current }` (Task 1).
3. **Unchanged usage with `f32` rounding noise** (base sample `48.3_f32` vs current `48.3_f64` gives a microscopic positive rate) → no integer overflow, no bogus run-out; `AtReset` ≈ 48.3 (Task 1).
4. **Session idle / not running** — the session quota is absent from the response while `history.json` still holds last window's session samples → no session forecast at all (Task 1).
5. **Threshold alert for one quota and run-out for the other in the same poll** → both notifications arrive; only a same-quota pair merges (Task 4).

---

### Task 1: `forecast.rs` — the pure forecast

**Files:**
- Create: `src-tauri/src/forecast.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod forecast;` between `pub mod alerts;` and `pub mod history;`)

**Interfaces:**
- Consumes: `crate::usage::Quota { key: String, label: String, percent: f64, resets_at: i64, period_secs: u64 }`; `crate::history::{History { by_key: HashMap<String, Vec<Sample>> }, Sample { t: i64, pct: f32 }}`.
- Produces: `pub enum Forecast { RunsOut { at: i64 }, AtReset { percent: f64 } }` (derives `Debug, Clone, PartialEq, Serialize`); `pub fn lookback(period_secs: u64) -> i64`; `pub fn forecast(q: &Quota, samples: &[Sample], now: i64) -> Option<Forecast>`; `pub fn for_quotas(quotas: &[Quota], history: &History, now: i64) -> HashMap<String, Forecast>`.

- [ ] **Step 1: Write the failing tests**

Add `pub mod forecast;` to `src-tauri/src/lib.rs` so the file reads:

```rust
//! Library target so integration tests and the binary share the same modules.
pub mod alerts;
pub mod forecast;
pub mod history;
pub mod icon;
pub mod log;
pub mod poll;
pub mod settings;
pub mod tray;
pub mod update;
pub mod usage;
```

Create `src-tauri/src/forecast.rs` with only the tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{SESSION_SECS, WEEKLY_SECS};

    const NOW: i64 = 1_789_588_800;

    /// Session quota whose window started an hour ago: resets_at = NOW + 14400.
    fn session(percent: f64) -> Quota {
        Quota {
            key: "session".into(),
            label: "Session".into(),
            percent,
            resets_at: NOW + 14_400,
            period_secs: SESSION_SECS,
        }
    }

    fn at(t: i64, pct: f32) -> Sample {
        Sample { t, pct }
    }

    fn runs_out_at(f: Option<Forecast>) -> i64 {
        match f {
            Some(Forecast::RunsOut { at }) => at,
            other => panic!("expected RunsOut, got {other:?}"),
        }
    }

    fn at_reset(f: Option<Forecast>) -> f64 {
        match f {
            Some(Forecast::AtReset { percent }) => percent,
            other => panic!("expected AtReset, got {other:?}"),
        }
    }

    #[test]
    fn lookback_is_a_seventh_of_the_period() {
        assert_eq!(lookback(SESSION_SECS), 2571);
        assert_eq!(lookback(WEEKLY_SECS), 86_400);
    }

    #[test]
    fn steady_rate_runs_out_before_the_reset() {
        let f = forecast(&session(50.0), &[at(NOW - 3000, 20.0), at(NOW, 50.0)], NOW);
        assert!((runs_out_at(f) - (NOW + 5000)).abs() <= 1);
    }

    #[test]
    fn slow_rate_projects_the_percent_at_reset() {
        let f = forecast(&session(30.0), &[at(NOW - 3000, 20.0), at(NOW, 30.0)], NOW);
        assert!((at_reset(f) - 78.0).abs() < 0.01);
    }

    #[test]
    fn flat_usage_stays_where_it_is() {
        let f = forecast(&session(30.0), &[at(NOW - 3000, 30.0), at(NOW, 30.0)], NOW);
        assert!((at_reset(f) - 30.0).abs() < 0.01);
    }

    #[test]
    fn falling_usage_stays_at_the_current_percent() {
        let f = forecast(&session(50.0), &[at(NOW - 3000, 60.0), at(NOW, 50.0)], NOW);
        assert!((at_reset(f) - 50.0).abs() < 0.01);
    }

    #[test]
    fn f32_rounding_noise_is_not_a_run_out() {
        // 48.3 stored as f32 reads back a hair below the fresh f64 48.3: a microscopic rate.
        let f = forecast(&session(48.3), &[at(NOW - 3000, 48.3), at(NOW, 48.3)], NOW);
        assert!((at_reset(f) - 48.3).abs() < 0.01);
    }

    #[test]
    fn base_is_the_newest_sample_at_least_one_lookback_old() {
        // NOW − 1000 is inside the lookback and must be ignored; NOW − 3000 is the base.
        let samples = [
            at(NOW - 3500, 0.0),
            at(NOW - 3000, 20.0),
            at(NOW - 1000, 49.0),
            at(NOW, 50.0),
        ];
        let f = forecast(&session(50.0), &samples, NOW);
        assert!((runs_out_at(f) - (NOW + 5000)).abs() <= 1);
    }

    #[test]
    fn without_an_old_sample_the_window_start_is_the_base() {
        // Window started 3600 s ago at 0 %: 50 % in an hour → 100 % in another hour.
        let f = forecast(&session(50.0), &[at(NOW - 600, 45.0), at(NOW, 50.0)], NOW);
        assert!((runs_out_at(f) - (NOW + 3600)).abs() <= 1);
    }

    #[test]
    fn no_forecast_too_early_when_full_or_after_the_reset() {
        let young = Quota {
            resets_at: NOW + SESSION_SECS as i64 - 1000,
            ..session(10.0)
        };
        assert_eq!(forecast(&young, &[], NOW), None);
        assert_eq!(forecast(&session(100.0), &[], NOW), None);
        assert_eq!(forecast(&session(112.0), &[], NOW), None);
        let over = Quota {
            resets_at: NOW,
            ..session(50.0)
        };
        assert_eq!(forecast(&over, &[], NOW), None);
    }

    #[test]
    fn weekly_uses_a_one_day_lookback() {
        // Window 3 days old; 30 % a day ago, 50 % now → 20 %/day → 100 % in 2.5 days,
        // before the reset 4 days out.
        let q = Quota {
            key: "weekly".into(),
            label: "Weekly".into(),
            percent: 50.0,
            resets_at: NOW + 4 * 86_400,
            period_secs: WEEKLY_SECS,
        };
        let samples = [at(NOW - 86_400, 30.0), at(NOW - 3600, 49.0), at(NOW, 50.0)];
        let f = forecast(&q, &samples, NOW);
        assert!((runs_out_at(f) - (NOW + 216_000)).abs() <= 1);
    }

    #[test]
    fn for_quotas_covers_session_and_weekly_only() {
        let mut h = History::default();
        h.by_key
            .insert("session".into(), vec![at(NOW - 3000, 20.0), at(NOW, 50.0)]);
        let scoped = Quota {
            key: "weekly:fable".into(),
            ..session(50.0)
        };
        let out = for_quotas(&[session(50.0), scoped], &h, NOW);
        assert_eq!(out.len(), 1);
        assert!(matches!(out["session"], Forecast::RunsOut { .. }));
    }

    #[test]
    fn for_quotas_skips_an_absent_quota_even_with_old_samples() {
        // No session running: the response has no session quota, history.json still does.
        let mut h = History::default();
        h.by_key
            .insert("session".into(), vec![at(NOW - 3000, 20.0), at(NOW - 60, 50.0)]);
        assert!(for_quotas(&[], &h, NOW).is_empty());
    }

    #[test]
    fn serializes_with_a_kind_tag() {
        let v = serde_json::to_value(Forecast::RunsOut { at: 5 }).expect("serializes");
        assert_eq!(v, serde_json::json!({"kind": "runs_out", "at": 5}));
        let v = serde_json::to_value(Forecast::AtReset { percent: 78.0 }).expect("serializes");
        assert_eq!(v, serde_json::json!({"kind": "at_reset", "percent": 78.0}));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml forecast`
Expected: compile error — `cannot find function 'forecast'` / `cannot find type 'Forecast'`.

- [ ] **Step 3: Write the implementation**

Insert above the `#[cfg(test)]` block in `src-tauri/src/forecast.rs`:

```rust
//! Pace forecast: where a quota lands at the recent rate. Pure; computed once per successful
//! poll from the full-resolution history samples, shared by the popover and the alerts.

use crate::history::{History, Sample};
use crate::usage::Quota;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Forecast {
    /// 100 % is reached before the reset, at this unix second.
    RunsOut { at: i64 },
    /// Projected utilization when the window resets; always < 100.
    AtReset { percent: f64 },
}

/// `period_secs / 7`: ~43 min for the session, exactly one day for the weekly quota, so the
/// weekly rate always spans a full day-night cycle.
pub fn lookback(period_secs: u64) -> i64 {
    period_secs as i64 / 7
}

pub fn forecast(q: &Quota, samples: &[Sample], now: i64) -> Option<Forecast> {
    let lookback = lookback(q.period_secs);
    let window_start = q.resets_at - q.period_secs as i64;
    if q.percent >= 100.0 || now >= q.resets_at || now - window_start < lookback / 2 {
        return None;
    }
    // Windows start at 0 %, so without an old-enough sample this is the window average.
    let (t0, p0) = samples
        .iter()
        .rev()
        .find(|s| s.t >= window_start && s.t <= now - lookback)
        .map_or((window_start, 0.0), |s| (s.t, f64::from(s.pct)));
    let dt = (now - t0) as f64;
    if dt <= 0.0 {
        return None;
    }
    let rate = (q.percent - p0) / dt;
    if rate <= 0.0 {
        return Some(Forecast::AtReset { percent: q.percent });
    }
    // Compared as floats before any cast: a microscopic rate (f32 noise) must not overflow i64.
    let secs_to_full = (100.0 - q.percent) / rate;
    let secs_to_reset = (q.resets_at - now) as f64;
    if secs_to_full < secs_to_reset {
        Some(Forecast::RunsOut {
            at: now + secs_to_full.ceil() as i64,
        })
    } else {
        Some(Forecast::AtReset {
            percent: q.percent + rate * secs_to_reset,
        })
    }
}

/// Forecasts for the quotas in this response only: a quota the API no longer reports (no
/// session running) gets none, even while its old samples are still in the history.
pub fn for_quotas(quotas: &[Quota], history: &History, now: i64) -> HashMap<String, Forecast> {
    quotas
        .iter()
        .filter(|q| q.key == "session" || q.key == "weekly")
        .filter_map(|q| {
            let samples = history.by_key.get(&q.key).map_or(&[][..], Vec::as_slice);
            forecast(q, samples, now).map(|f| (q.key.clone(), f))
        })
        .collect()
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml forecast`
Expected: 13 tests in `forecast::tests` PASS.

- [ ] **Step 5: Verify and commit**

Run: `pnpm verify` — expected green (clippy may flag dead code only if something is unused; `forecast` is `pub` in a lib, so it is not).

```bash
git add src-tauri/src/forecast.rs src-tauri/src/lib.rs
git commit -m "feat: pace forecast from the usage history" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: `Snapshot.forecast` and poll wiring

**Files:**
- Modify: `src-tauri/src/poll.rs` (imports, `Snapshot`, `Default`, success branch ~lines 241–258, tests)
- Modify: `src-tauri/src/tray.rs` (test helper `snapshot()` ~line 707)
- Modify: `src-tauri/src/icon.rs` (test helper `snapshot()` ~line 128)

**Interfaces:**
- Consumes: `forecast::{Forecast, for_quotas}` from Task 1.
- Produces: `Snapshot.forecast: HashMap<String, forecast::Forecast>` (serialized field `forecast`), read by Task 4 (`main.rs`) and Task 5 (frontend).

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/poll.rs`, extend `default_snapshot_has_no_token_and_no_quotas`:

```rust
    #[test]
    fn default_snapshot_has_no_token_and_no_quotas() {
        let s = Snapshot::default();
        assert_eq!(s.status, Status::NoToken);
        assert!(s.quotas.is_empty());
        assert_eq!(s.fetched_at, None);
        assert!(s.forecast.is_empty());
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml default_snapshot`
Expected: compile error — `no field 'forecast' on type 'Snapshot'`.

- [ ] **Step 3: Add the field and wire it**

In `src-tauri/src/poll.rs`:

Add one import directly above `use crate::history;`:

```rust
use crate::forecast::{self, Forecast};
```

`Snapshot` gains, directly after `history`:

```rust
    /// Pace forecast per session/weekly quota, computed from the full-resolution history.
    pub forecast: HashMap<String, Forecast>,
```

`Default for Snapshot` gains `forecast: HashMap::new(),` after `history: HashMap::new(),`.

In `run`, the success branch after the history block becomes:

```rust
                                let popover_history = history.for_popover();
                                let forecasts = forecast::for_quotas(&quotas, &history, now);
                                let mut s = lock(&shared);
                                s.quotas = quotas;
                                s.history = popover_history;
                                s.forecast = forecasts;
                                s.extra = extra;
```

(the remaining lines — `if plan.is_some()`, `fetched_at`, `status` — stay as they are).

Fix the struct literals that now miss a field:
- `poll.rs` test `log_line_summarizes_the_cycle`: add `forecast: HashMap::new(),` after `history: HashMap::new(),`.
- `tray.rs` test helper `snapshot()`: add `forecast: HashMap::new(),` after `history: HashMap::new(),`.
- `icon.rs` test helper `snapshot()`: add `forecast: Default::default(),` after `history: Default::default(),`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: all PASS, including `default_snapshot_has_no_token_and_no_quotas`.

- [ ] **Step 5: Verify and commit**

Run: `pnpm verify` — expected green.

```bash
git add src-tauri/src/poll.rs src-tauri/src/tray.rs src-tauri/src/icon.rs
git commit -m "feat: ship the pace forecast in the snapshot" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: `alert_forecast` setting and menu item

**Files:**
- Modify: `src-tauri/src/settings.rs` (struct, `Default`, `KEYS`, `get`, `toggle` + its doc comment, tests)
- Modify: `src-tauri/src/tray.rs` (`ALERT_LABELS` ~line 48, tests)

**Interfaces:**
- Consumes: nothing new.
- Produces: `Settings.alert_forecast: bool` (default `true`), toggle key `"alert_forecast"`, menu id `set:alert_forecast` — read by Task 4.

- [ ] **Step 1: Write the failing tests**

In `src-tauri/src/settings.rs` tests, add:

```rust
    #[test]
    fn alert_forecast_defaults_on_and_toggles_freely() {
        let mut s = Settings::default();
        assert!(s.alert_forecast);
        assert!(s.toggle("alert_forecast"));
        assert!(!s.get("alert_forecast"));
        assert!(s.toggle("alert_forecast"));
        assert!(s.get("alert_forecast"));
    }
```

Change both `assert_eq!(KEYS.len(), 13);` (in `alert_keys_toggle_freely` and `new_fields_default`) to `assert_eq!(KEYS.len(), 14);`, and add `assert!(s.alert_forecast);` at the end of `old_file_gets_new_defaults`.

In `src-tauri/src/tray.rs` tests, add:

```rust
    #[test]
    fn alert_menu_items_are_known_settings() {
        assert!(ALERT_LABELS
            .iter()
            .any(|(k, l)| *k == "alert_forecast" && *l == "Run-out forecast"));
        assert!(ALERT_LABELS.iter().all(|(k, _)| KEYS.contains(k)));
        assert_eq!(
            parse_menu_id("set:alert_forecast"),
            Some(MenuAction::Toggle("alert_forecast".into()))
        );
    }
```

(If the tray test module does not already `use super::*`, import `ALERT_LABELS`, `KEYS`, `parse_menu_id` and `MenuAction` explicitly.)

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml alert`
Expected: compile error — `no field 'alert_forecast' on type 'Settings'`.

- [ ] **Step 3: Implement**

`src-tauri/src/settings.rs`:
- struct: `pub alert_forecast: bool,` directly after `pub alert_reset: bool,`.
- `Default`: `alert_forecast: true,` directly after `alert_reset: true,`.
- `KEYS`: type `[&str; 14]`, `"alert_forecast",` directly after `"alert_reset",`.
- `get`: `"alert_forecast" => self.alert_forecast,` after the `alert_reset` arm.
- `toggle` partner arm: `"alert_session" | "alert_weekly" | "alert_reset" | "alert_forecast" | "auto_update_check" => true,`
- `toggle` flip arm: `"alert_forecast" => self.alert_forecast = !self.alert_forecast,` after the `alert_reset` arm.
- `toggle` doc comment's last line becomes: `` /// `glyph`, `alert_session`, `alert_weekly`, `alert_reset`, and `alert_forecast` have no partner and can be freely toggled. ``

`src-tauri/src/tray.rs`:

```rust
const ALERT_LABELS: [(&str, &str); 4] = [
    ("alert_session", "Session"),
    ("alert_weekly", "Weekly"),
    ("alert_reset", "Notify on reset"),
    ("alert_forecast", "Run-out forecast"),
];
```

The menu loop and `on_setting_toggled` are generic over these keys; nothing else changes.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: all PASS.

- [ ] **Step 5: Verify and commit**

Run: `pnpm verify` — expected green.

```bash
git add src-tauri/src/settings.rs src-tauri/src/tray.rs
git commit -m "feat: Run-out forecast alert setting" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: `alerts::run_outs` and notification delivery

**Files:**
- Modify: `src-tauri/src/alerts.rs` (imports, `AlertState`, new `RunOut`, `run_outs`, `partition_run_outs`, tests)
- Modify: `src-tauri/src/main.rs` (`notify_thresholds`)

**Interfaces:**
- Consumes: `forecast::{Forecast, lookback}` (Task 1), `Snapshot.forecast` (Task 2), `Settings.alert_forecast` (Task 3); existing `enabled`, `top`, `label`, `PRUNE_AFTER_SECS`, `SAME_WINDOW_SECS`, `Alert`.
- Produces: `pub struct RunOut { pub key: String, pub label: String, pub at: i64, pub resets_at: i64 }`; `pub fn run_outs(state: &mut AlertState, quotas: &[Quota], forecasts: &HashMap<String, Forecast>, settings: &Settings, now: i64) -> Vec<RunOut>`; `pub fn partition_run_outs(due: &[Alert], run_outs: Vec<RunOut>) -> (Vec<RunOut>, Vec<RunOut>)` (merged, alone).

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests` in `src-tauri/src/alerts.rs`:

```rust
    fn runs_out(key: &str, at: i64) -> HashMap<String, Forecast> {
        HashMap::from([(key.to_string(), Forecast::RunsOut { at })])
    }

    #[test]
    fn run_out_fires_once_per_window() {
        let mut st = AlertState::default();
        let q = session(40.0, 4 * 3600);
        let f = runs_out("session", NOW + 3600);
        let out = run_outs(&mut st, &[q.clone()], &f, &on(), NOW);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, "session");
        assert_eq!(out[0].label, "Session");
        assert_eq!(out[0].at, NOW + 3600);
        assert_eq!(out[0].resets_at, q.resets_at);
        assert!(run_outs(&mut st, &[q.clone()], &f, &on(), NOW + 180).is_empty());
        let jittered = Quota {
            resets_at: q.resets_at + 1,
            ..q.clone()
        };
        assert!(run_outs(&mut st, &[jittered], &f, &on(), NOW + 360).is_empty());
        let next = next_window(&q);
        let later = runs_out("session", next.resets_at - 3 * 3600);
        assert_eq!(
            run_outs(&mut st, &[next], &later, &on(), NOW + 4 * 3600).len(),
            1
        );
    }

    #[test]
    fn run_out_needs_both_toggles() {
        let q = session(40.0, 4 * 3600);
        let f = runs_out("session", NOW + 3600);
        let mut off = on();
        assert!(off.toggle("alert_forecast"));
        assert!(run_outs(&mut AlertState::default(), &[q.clone()], &f, &off, NOW).is_empty());
        let mut no_session = on();
        assert!(no_session.toggle("alert_session"));
        assert!(run_outs(&mut AlertState::default(), &[q], &f, &no_session, NOW).is_empty());
    }

    #[test]
    fn run_out_close_to_the_reset_is_silent() {
        // Resets in 4 h; running out 30 min before is less than the ~43 min lookback.
        let q = session(40.0, 4 * 3600);
        let f = runs_out("session", q.resets_at - 1800);
        assert!(run_outs(&mut AlertState::default(), &[q], &f, &on(), NOW).is_empty());
    }

    #[test]
    fn run_out_is_silent_at_the_top_level() {
        let q = session(95.0, 4 * 3600);
        let f = runs_out("session", NOW + 600);
        assert!(run_outs(&mut AlertState::default(), &[q], &f, &on(), NOW).is_empty());
    }

    #[test]
    fn at_reset_forecasts_never_fire() {
        let q = session(40.0, 4 * 3600);
        let f = HashMap::from([("session".to_string(), Forecast::AtReset { percent: 70.0 })]);
        assert!(run_outs(&mut AlertState::default(), &[q], &f, &on(), NOW).is_empty());
    }

    #[test]
    fn run_out_entries_from_old_windows_are_pruned() {
        let mut st = AlertState::default();
        st.forecast_fired
            .insert(("session".to_string(), NOW - 2 * 86400));
        run_outs(&mut st, &[], &HashMap::new(), &on(), NOW);
        assert!(st.forecast_fired.is_empty());
    }

    #[test]
    fn run_outs_merge_only_into_a_same_quota_alert() {
        let alert = Alert {
            key: "session".into(),
            label: "Session".into(),
            level: 80,
            percent: 85.0,
            resets_at: NOW + 7200,
        };
        let r = |key: &str| RunOut {
            key: key.into(),
            label: key.into(),
            at: NOW + 3600,
            resets_at: NOW + 7200,
        };
        let (merged, alone) = partition_run_outs(&[alert], vec![r("session"), r("weekly")]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].key, "session");
        assert_eq!(alone.len(), 1);
        assert_eq!(alone[0].key, "weekly");
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml alerts`
Expected: compile error — `cannot find function 'run_outs'`, `cannot find type 'RunOut'`, `no field 'forecast_fired'`.

- [ ] **Step 3: Implement in `alerts.rs`**

Imports at the top become:

```rust
use crate::forecast::{self, Forecast};
use crate::settings::Settings;
use crate::usage::Quota;
use std::collections::{HashMap, HashSet};
```

`AlertState` becomes (doc comment extended):

```rust
/// Levels already announced, keyed by (quota key, reset window, level), plus the window in
/// which each quota last hit its top level — that arms the reset notification — and the
/// windows that already had a run-out notification. In-memory only.
#[derive(Default)]
pub struct AlertState {
    fired: HashSet<(String, i64, u8)>,
    armed: HashMap<String, i64>,
    forecast_fired: HashSet<(String, i64)>,
}
```

Add after `marker` (before `#[cfg(test)]`):

```rust
/// A quota the pace forecast says hits 100 % well before its reset.
#[derive(Debug, Clone, PartialEq)]
pub struct RunOut {
    pub key: String,
    pub label: String,
    pub at: i64,
    pub resets_at: i64,
}

/// At most one per quota per window, only when running out costs at least one lookback of the
/// window, and only below the top level, where the threshold alert already speaks. Returned
/// entries count as fired whether the caller sends them alone or merges them into an alert.
pub fn run_outs(
    state: &mut AlertState,
    quotas: &[Quota],
    forecasts: &HashMap<String, Forecast>,
    settings: &Settings,
    now: i64,
) -> Vec<RunOut> {
    state
        .forecast_fired
        .retain(|(_, resets_at)| *resets_at >= now - PRUNE_AFTER_SECS);
    if !settings.alert_forecast {
        return Vec::new();
    }
    let mut out = Vec::new();
    for q in quotas.iter().filter(|q| enabled(settings, &q.key)) {
        let Some(Forecast::RunsOut { at }) = forecasts.get(&q.key) else {
            continue;
        };
        let early_enough = q.resets_at - at >= forecast::lookback(q.period_secs);
        let below_top = top(settings.levels(&q.key)).is_none_or(|t| q.percent < f64::from(t));
        let fired = state
            .forecast_fired
            .iter()
            .any(|(k, r)| *k == q.key && (r - q.resets_at).abs() <= SAME_WINDOW_SECS);
        if !early_enough || !below_top || fired {
            continue;
        }
        state.forecast_fired.insert((q.key.clone(), q.resets_at));
        out.push(RunOut {
            key: q.key.clone(),
            label: label(&q.key).to_string(),
            at: *at,
            resets_at: q.resets_at,
        });
    }
    out
}

/// Splits run-outs into those that ride along with a threshold alert for the same quota in
/// this cycle (one notification instead of two) and those sent on their own.
pub fn partition_run_outs(due: &[Alert], run_outs: Vec<RunOut>) -> (Vec<RunOut>, Vec<RunOut>) {
    run_outs
        .into_iter()
        .partition(|r| due.iter().any(|a| a.key == r.key))
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml alerts`
Expected: all `alerts::tests` PASS (existing 20 + 7 new).

- [ ] **Step 5: Deliver from `main.rs`**

Replace `notify_thresholds` in `src-tauri/src/main.rs` with:

```rust
/// Shows one notification per newly crossed threshold, reset, or projected run-out; a run-out
/// due together with a threshold alert for the same quota rides along in that alert's body.
/// Failures are ignored: the title marker still tells the story if notifications are denied.
fn notify_thresholds(app: &AppHandle, snapshot: &Snapshot) {
    let settings = app
        .state::<Arc<Mutex<settings::Settings>>>()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let now = poll::now();
    let (resets, due, run_outs) = {
        let state = app.state::<Mutex<alerts::AlertState>>();
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Resets first: they belong to the window that just ended, thresholds to the new one.
        let resets = alerts::resets(&mut state, &snapshot.quotas, &settings);
        let due = alerts::evaluate(&mut state, &snapshot.quotas, &settings, now);
        let run_outs =
            alerts::run_outs(&mut state, &snapshot.quotas, &snapshot.forecast, &settings, now);
        (resets, due, run_outs)
    };
    for r in &run_outs {
        log::write(
            app,
            &format!("forecast {} 100% in {}", r.key, tray::countdown(r.at - now)),
        );
    }
    let (merged, alone) = alerts::partition_run_outs(&due, run_outs);
    for reset in resets {
        let _ = app
            .notification()
            .builder()
            .title(format!("Claude usage: {} reset", reset.label))
            .body(format!(
                "Back to 0% · next reset in {}",
                tray::countdown(reset.resets_at - now)
            ))
            .show();
        log::write(app, &format!("reset {}", reset.key));
    }
    for alert in due {
        let resets_in = tray::countdown(alert.resets_at - now);
        let body = match merged.iter().find(|r| r.key == alert.key) {
            Some(r) => format!(
                "Resets in {resets_in} · at this pace 100% in ~{}",
                tray::countdown(r.at - now)
            ),
            None => format!("Resets in {resets_in}"),
        };
        let _ = app
            .notification()
            .builder()
            .title(format!(
                "Claude usage: {} {}%",
                alert.label,
                alert.percent.round() as i64
            ))
            .body(body)
            .show();
        log::write(
            app,
            &format!(
                "alert {} {} at {}%",
                alert.key,
                alert.level,
                alert.percent.round() as i64
            ),
        );
    }
    for r in alone {
        let _ = app
            .notification()
            .builder()
            .title(format!("Claude usage: {}", r.label))
            .body(format!(
                "At this pace: 100% in ~{} · resets in {}",
                tray::countdown(r.at - now),
                tray::countdown(r.resets_at - now)
            ))
            .show();
    }
}
```

- [ ] **Step 6: Verify and commit**

Run: `pnpm verify` — expected green (`cargo fmt --check` may ask to reflow; run `cargo fmt --manifest-path src-tauri/Cargo.toml` and re-run).

```bash
git add src-tauri/src/alerts.rs src-tauri/src/main.rs
git commit -m "feat: run-out forecast notification" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Forecast line in the popover

**Files:**
- Modify: `src/lib/quota.ts` (new `Forecast` type, `Snapshot.forecast`)
- Modify: `src/lib/format.ts` (new `forecastText`)
- Modify: `src/lib/ipc.ts` (`FIXTURE.forecast`)
- Modify: `src/App.tsx` (`QuotaCard` prop + line, `App` passes it)
- Modify: `src/app.css` (`.reset.runs-out`)
- Test: `test/format.test.ts`

**Interfaces:**
- Consumes: the serialized `Snapshot.forecast` from Task 2: `Record<string, {kind:'runs_out', at:number} | {kind:'at_reset', percent:number}>`.
- Produces: `export type Forecast`, `Snapshot.forecast: Record<string, Forecast>`, `export function forecastText(f: Forecast, now: number): string | null`.

- [ ] **Step 1: Write the failing tests**

In `test/format.test.ts`, add `forecastText` to the `@/lib/format` import list (alphabetical: after `elapsedPct`), then append:

```ts
describe('forecastText', () => {
  it('names the run-out time, with the weekday when not today', () => {
    expect(forecastText({ kind: 'runs_out', at: NOW + 2 * 3600 + 13 * 60 }, NOW)).toBe(
      'At this pace: 100% at 16:45',
    );
    expect(
      forecastText({ kind: 'runs_out', at: NOW + 3 * 86400 + 6 * 3600 + 28 * 60 }, NOW),
    ).toBe('At this pace: 100% at Sat 21:00');
  });

  it('rounds the projected percent at reset', () => {
    expect(forecastText({ kind: 'at_reset', percent: 77.6 }, NOW)).toBe(
      'At this pace: ~78% at reset',
    );
  });

  it('hides a run-out time that is already in the past', () => {
    // A snapshot from before a sleep: the popover must not claim "100% at 14:02" at 14:32.
    expect(forecastText({ kind: 'runs_out', at: NOW - 30 * 60 }, NOW)).toBeNull();
    expect(forecastText({ kind: 'runs_out', at: NOW }, NOW)).toBeNull();
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `pnpm test:run`
Expected: FAIL — `forecastText is not a function` (or a TS import error).

- [ ] **Step 3: Implement the types and the formatter**

`src/lib/quota.ts` — after the `Sample` type:

```ts
// Mirrors src-tauri/src/forecast.rs: where usage lands at the recent pace.
export type Forecast = { kind: 'runs_out'; at: number } | { kind: 'at_reset'; percent: number };
```

and in `Snapshot`, after `history`:

```ts
  forecast: Record<string, Forecast>;
```

`src/lib/format.ts` — the import becomes
`import type { ExtraUsage, Forecast, Quota, Sample } from './quota';` and append:

```ts
// Null for a run-out already in the past: the snapshot predates a sleep or a long backoff.
export function forecastText(f: Forecast, now: number): string | null {
  if (f.kind === 'at_reset') return `At this pace: ~${Math.round(f.percent)}% at reset`;
  return f.at > now ? `At this pace: 100% at ${clock(f.at, now)}` : null;
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `pnpm test:run`
Expected: PASS, including the three new `forecastText` tests.

- [ ] **Step 5: Render the line**

`src/lib/ipc.ts` — in `FIXTURE`, after `status: { kind: 'ok' },`:

```ts
  forecast: {
    session: { kind: 'runs_out', at: nowSecs + 80 * 60 },
    weekly: { kind: 'at_reset', percent: 78 },
  },
```

`src/App.tsx`:
- add `forecastText` to the `@/lib/format` import (alphabetical: after `extraPct`);
- the `@/lib/quota` type import becomes
  `import type { ExtraUsage, Forecast, PopoverSettings, Quota, Sample, Snapshot, Status } from '@/lib/quota';`
- `QuotaCard` takes a `forecast` prop:

```tsx
function QuotaCard({
  q,
  now,
  settings,
  history,
  forecast,
}: {
  q: Quota;
  now: number;
  settings: PopoverSettings;
  history: Sample[];
  forecast: Forecast | undefined;
}) {
```

- inside `QuotaCard`, after `const levels = levelsFor(q, settings);`:

```tsx
  const pace = forecast ? forecastText(forecast, now) : null;
```

- after the `<p className="reset">…</p>` element:

```tsx
      {pace && (
        <p className={forecast?.kind === 'runs_out' ? 'reset runs-out' : 'reset'}>{pace}</p>
      )}
```

- in `App`, the `QuotaCard` element gains `forecast={snap.forecast[q.key]}` after the `history` prop.

`src/app.css` — directly after the `.reset { … }` rule:

```css
.reset.runs-out {
  color: var(--over);
}
```

- [ ] **Step 6: Verify, look at it, commit**

Run: `pnpm check:fix && pnpm verify` — expected green (Biome may reflow the long lines above).
Run: `pnpm dev`, open http://localhost:5173 — the Session card shows a red `At this pace: 100% at HH:MM` about 80 minutes from now, the Weekly card a muted `At this pace: ~78% at reset`, the Fable card no line. Stop the dev server.

```bash
git add src/lib/quota.ts src/lib/format.ts src/lib/ipc.ts src/App.tsx src/app.css test/format.test.ts
git commit -m "feat: pace forecast line in the popover" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Docs, changeset, history record, quality gate

**Files:**
- Modify: `README.md` (Popover "Also in the popover" list, Alerts list, Configure table Alerts row)
- Modify: `CLAUDE.md` (status line, spec list, layout)
- Modify: `docs/superpowers/specs/2026-09-22-pace-forecast-design.md` (status line; the `forecastText` null rule from Review Focus 1)
- Create: `.changeset/pace-forecast.md`
- Modify: `history/2026-09-22-pace-forecast.md` (created by `history-driven-workflow` at execution start)

**Interfaces:**
- Consumes: the finished feature (Tasks 1–5).
- Produces: user-facing docs and the release note.

- [ ] **Step 1: README**

In `### Popover`, the "Also in the popover:" list gains as its first bullet:

```markdown
- **Pace forecast** — a line under the session and weekly cards: when you reach 100 % at the
  pace of the last ~43 minutes (session) or 24 hours (weekly), in red, or where you land at the
  reset if you will not run out first.
```

In `### Alerts`, after the **Notify on reset** bullet:

```markdown
- **Run-out forecast** (on by default) — one notification per window when the pace forecast
  says a quota runs out at least ~43 minutes (session) or a day (weekly) before it resets,
  while it is still below its highest level. If a threshold alert fires at the same moment, the
  forecast is added to that notification instead.
```

In the Configure table, the Alerts row's options start with
`Session · Weekly (on/off) · Notify on reset · Run-out forecast · **Session levels** …` (rest unchanged).

- [ ] **Step 2: CLAUDE.md and spec**

- Status line → `**Status:** v0.12 (pace forecast) implemented; released via the Changesets pipeline.`
- Spec list: the last entry `` `docs/superpowers/specs/2026-09-21-history-sparkline-design.md`. `` becomes
  `` `docs/superpowers/specs/2026-09-21-history-sparkline-design.md`, `` followed by a new line
  `` `docs/superpowers/specs/2026-09-22-pace-forecast-design.md`. ``
- Layout: after the `src/history.rs` line add
  `  src/forecast.rs          pace forecast from the history samples (pure)`
- Spec file: status line → `Date: 2026-09-22. Status: approved.`; in the Frontend section's
  `format.ts` bullet append: `Returns null for a runs_out whose at is not in the future (a
  snapshot from before a sleep or a long backoff); the card then shows no line.` and change the
  signature there to `forecastText(f: Forecast, now: number): string | null`.

- [ ] **Step 3: Changeset**

Create `.changeset/pace-forecast.md`:

```markdown
---
"claude-usage-monitor": minor
---

Pace forecast: the session and weekly cards in the popover say when you reach 100% at your recent pace, or where you land at the reset; Alerts → "Run-out forecast" sends one notification per window when a quota would run out well before it resets.
```

- [ ] **Step 4: History record**

Update `history/2026-09-22-pace-forecast.md` per the `history-driven-workflow` skill: `status: "complete"`, the `files:` list (every file in Tasks 1–6 plus the spec and this plan), Execution Log entries with the commit SHAs from Tasks 1–5, Testing (test counts from the last `pnpm verify`), Final Notes (the Review Focus decisions).

- [ ] **Step 5: Quality gate**

Run the `quality-gate` skill. Its checks for this branch:
- `pnpm verify` — green.
- `pnpm tauri build --no-bundle` — compiles the release binary (src-tauri changed); `--no-bundle` skips updater signing, which needs the maintainer's key.
- README/CLAUDE.md reflect the feature (Steps 1–2).
- No new dependencies in `package.json` / `Cargo.toml`; no orphaned files.
- `src-tauri/capabilities/default.json` unchanged (notifications are sent from Rust and need no capability).

- [ ] **Step 6: Commit**

```bash
git add README.md CLAUDE.md docs/superpowers/specs/2026-09-22-pace-forecast-design.md .changeset/pace-forecast.md history/2026-09-22-pace-forecast.md
git commit -m "docs: pace forecast in README, CLAUDE.md, changeset and history" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 7: Hand off**

Invoke `superpowers:finishing-a-development-branch`. The user is not a maintainer of `cef62/claude-usage-monitor`: a PR needs their own fork as the push remote.
