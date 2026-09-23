---
type: "feat"
status: "complete"
files:
  - src-tauri/src/forecast.rs
  - src-tauri/src/lib.rs
  - src-tauri/src/history.rs
  - src-tauri/src/poll.rs
  - src-tauri/src/alerts.rs
  - src-tauri/src/settings.rs
  - src-tauri/src/tray.rs
  - src-tauri/src/icon.rs
  - src-tauri/src/main.rs
  - src/lib/quota.ts
  - src/lib/format.ts
  - src/lib/ipc.ts
  - src/App.tsx
  - src/app.css
  - test/format.test.ts
  - README.md
  - CLAUDE.md
  - .changeset/pace-forecast.md
  - docs/superpowers/specs/2026-09-22-pace-forecast-design.md
  - docs/superpowers/plans/2026-09-22-pace-forecast.md
areas:
  - core
  - tests
  - docs
components:
  - forecast
  - alerts
  - settings
  - tray-menu
  - popover
tags:
  - pace
  - notifications
  - history
  - vitest
related-to:
  - docs/superpowers/specs/2026-09-22-pace-forecast-design.md
  - docs/superpowers/plans/2026-09-22-pace-forecast.md
  - docs/superpowers/specs/2026-09-21-history-sparkline-design.md
  - history/2026-09-17-threshold-alerts.md
  - history/2026-09-18-launch-at-login-and-reset-notification.md
---

# feat: pace forecast

| Field       | Value                 |
| ----------- | --------------------- |
| **Status**  | complete              |
| **Branch**  | `feat/pace-forecast`  |
| **Ticket**  | none                  |
| **Created** | 2026-09-22            |
| **Updated** | 2026-09-23            |

## Summary

A pace forecast for the session and weekly quotas: one line on each popover card ("At this
pace: 100% at 16:40" or "~78% at reset") and one notification per window when a quota is
projected to run out well before it resets. Computed in Rust from the usage history the app
already records, over a trailing lookback of a seventh of the window (~43 min session, 24 h
weekly). No new network traffic.

## Initial Request

"Brainstorm possible new features" → the user picked #1, the pace forecast, scoped as
"Popover + notification".

## Acceptance Criteria

- [x] Session and weekly cards show `At this pace: 100% at HH:MM` (red) or `At this pace: ~N% at reset`
- [x] No forecast early in a window (< half a lookback), at ≥ 100 %, or for per-model quotas
- [x] One notification per quota per window when the run-out is at least one lookback before the reset and usage is below the top alert level
- [x] A same-poll threshold alert for the same quota carries the forecast instead of a second notification
- [x] Alerts ▸ Run-out forecast toggles it (default on), persisted in settings.json
- [x] `pnpm verify` green; release binary compiles
- [x] README, CLAUDE.md, changeset updated

## Plan

Detailed steps: `docs/superpowers/plans/2026-09-22-pace-forecast.md`.

### Step 1: `forecast.rs` — the pure forecast
<!-- Status: complete -->

### Step 2: `Snapshot.forecast` and poll wiring
<!-- Status: complete -->

### Step 3: `alert_forecast` setting and menu item
<!-- Status: complete -->

### Step 4: `alerts::run_outs` and notification delivery
<!-- Status: complete -->

### Step 5: Forecast line in the popover
<!-- Status: complete -->

### Step 6: Docs, changeset, history record, quality gate
<!-- Status: complete -->

## Execution Log

### Setup - 2026-09-22
- The pinned pnpm 12.3.4 in `~/Library/pnpm/.tools` had an unlinked native binary (its
  `install.js` never ran); used the native binary directly via a PATH shim instead of editing
  the tool cache. Local Node is 25.9 (repo asks for ≥ 26, not enforced); CI runs 26.
- The pre-existing `money()` test fails under an Italian system locale (`Intl` default locale
  gives `12,34 €`); all checks ran with `LC_ALL=en_US.UTF-8`, as CI does.

### Step 1 - Complete ✓ (`ba07b48`)
- `forecast.rs` with 13 tests, written first and watched failing to compile.

### Step 2 - Complete ✓ (`863298d`)
- `Snapshot.forecast`, computed after `history.record` in the poll success branch.

### Step 3 - Complete ✓ (`3ebe7f7`)
- `alert_forecast` setting (`KEYS` 14) and the Alerts ▸ Run-out forecast item; it
  shares its commit with Step 4, since the setting ships with the notification it controls.

### Step 4 - Complete ✓ (`3ebe7f7`)
- `alerts::run_outs` / `partition_run_outs` (7 tests) and delivery in `main.rs`.

### Step 5 - Complete ✓ (`59e368b`)
- `forecastText` (3 tests), card line, CSS, dev fixture; checked in a 340 px browser window
  against the fixture.

### Step 6 - Complete ✓ (`67d24da`)
- README, CLAUDE.md, spec (null rule for past run-outs), changeset; quality gate passed.
- Whole-branch review by a fresh reviewer: ready to merge, no Critical or Important
  findings; six Minor items deferred (see Final Notes).

### Review round 1 - 2026-09-23 (PR #43, maintainer review)
- `d5cfe3c` refactor: `history::tracked(key)` shared by history and forecast (Nit).
- `7ef107c` fix: reaching 100 % exactly at the reset is `RunsOut`, not `AtReset { 100 }` (Bug).
- `220f3ba` fix: `RunsOut.from_average` (not serialized); a window-average forecast never alerts
  nor uses up the window (Bug: early-window false alarm).
- `eeec7ea` fix: `run_outs` runs once per `fetched_at` instead of every title tick (Bug: stale
  run-out alert; Nit: per-tick check).
- `165d20e` fix: `forecastText` takes `resets_at` and hides past-window lines, "at reset" caps at
  99 %, the card hides the line while stale (Bugs: `at_reset` outliving its window, stale UX).
- `03a5603` feat: Popover ▸ Pace forecast toggle, `show_forecast` (UX).
- docs (this commit): README states that every alert follows the quota's Session/Weekly switch
  (Docs), mentions the toggle and the early-window wait; spec Decisions and a revisions
  section; changeset mentions the toggle.

## Files Changed

- `src-tauri/src/forecast.rs` - new: trailing-rate forecast, `Forecast` enum, `lookback`, `for_quotas`
- `src-tauri/src/lib.rs` - registers `forecast`
- `src-tauri/src/history.rs` - `tracked(key)`, the one session/weekly filter (review round 1)
- `src-tauri/src/poll.rs` - `Snapshot.forecast`, computed once per successful poll
- `src-tauri/src/alerts.rs` - `RunOut`, `run_outs`, `partition_run_outs`, `forecast_fired` state
- `src-tauri/src/main.rs` - run-out notifications, merged into same-quota threshold alerts
- `src-tauri/src/settings.rs` - `alert_forecast` (default on)
- `src-tauri/src/tray.rs` - Alerts ▸ Run-out forecast; test fixture field
- `src-tauri/src/icon.rs` - test fixture field
- `src/lib/quota.ts` - `Forecast` type, `Snapshot.forecast`
- `src/lib/format.ts` - `forecastText`
- `src/lib/ipc.ts` - dev fixture forecasts
- `src/App.tsx` - forecast line on the quota card
- `src/app.css` - `.reset.runs-out`
- `test/format.test.ts` - `forecastText` tests
- `README.md`, `CLAUDE.md` - feature docs, layout, spec list
- `.changeset/pace-forecast.md` - minor release note
- `docs/superpowers/specs/2026-09-22-pace-forecast-design.md`, `docs/superpowers/plans/2026-09-22-pace-forecast.md` - spec and plan

## Testing

- `pnpm verify` green: Biome, tsc, Vitest 31/31, `cargo fmt --check`, `cargo clippy -D warnings`,
  `cargo test` 133/133 (29 new: 15 forecast, 9 alerts, 2 settings, 2 tray, 1 history; one
  poll test extended); Vitest 5 new. Review round 1 added 7 Rust and 2 Vitest tests, each
  watched failing first.
- `pnpm tauri build --no-bundle` builds the release binary (bundling skipped: updater signing
  needs the maintainer's key).
- Popover checked in a browser with the dev fixture: red run-out line on Session, muted
  "~78% at reset" on Weekly, none on the per-model card.
- Not exercised by hand: a real notification (needs a run-out in live data).

## Final Notes

- Review Focus decisions: a `runs_out` time already in the past is hidden (stale snapshot after
  sleep); falling usage and `f32` rounding noise give `AtReset` at the current percent; the
  run-out comparison is done in floating point before any cast, so a microscopic rate cannot
  overflow; an absent quota gets no forecast even with old samples in `history.json`; only a
  same-quota alert/run-out pair merges.
- Deferred minors from the pre-PR review: the shared `tracked(key)` filter and the README
  wording were done in review round 1, and the un-ceiled comparison is settled by `<=`. Still
  open: weekly-gate and `alert_weekly` tests for `run_outs`; the optional chain in `App.tsx`.
- The notification state is in memory, like the threshold alerts: a restart can repeat a
  run-out notification in the same window.
