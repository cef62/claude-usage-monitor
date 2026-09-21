---
type: "feat"
status: "complete"
files:
  - src-tauri/src/alerts.rs
  - src-tauri/src/settings.rs
  - src-tauri/src/tray.rs
  - src-tauri/src/main.rs
  - src-tauri/src/lib.rs
  - src-tauri/Cargo.toml
  - src-tauri/Cargo.lock
  - .changeset/threshold-alerts.md
  - CLAUDE.md
  - README.md
  - docs/superpowers/specs/2026-09-17-threshold-alerts-design.md
  - docs/superpowers/plans/2026-09-17-threshold-alerts.md
areas:
  - core
  - tray
  - docs
components:
  - alert-state-machine
  - notifications
  - tray-title
tags:
  - tauri
  - rust
  - macos
  - notifications
related-to:
  - docs/superpowers/specs/2026-09-17-threshold-alerts-design.md
  - docs/superpowers/plans/2026-09-17-threshold-alerts.md
  - history/2026-09-17-menu-bar-settings.md
---

# feat: threshold alerts

| Field       | Value                                    |
| ----------- | ----------------------------------------- |
| **Status**  | complete                                   |
| **Branch**  | `feat/alerts` (squash-merged as PR #6)     |
| **Ticket**  | none                                       |
| **Created** | 2026-09-17                                 |
| **Updated** | 2026-09-17                                 |

## Summary

Shipped v1.2: threshold alerts. A macOS notification fires when the session quota crosses 80%
or 95%, and when the weekly quota crosses 95%, once per reset window, delivered via
`tauri-plugin-notification` from the Rust side (poll callback), with the 80% level suppressed
while usage is still behind the clock (`percent > elapsed_pct`). A `⚠` marker prefixes the menu
bar title while any alerting quota sits at or above 95%. Per-quota on/off (`Session`, `Weekly`)
lives under a new tray right-click **Alerts** submenu, backed by two new `Settings` fields
(`alert_session`, `alert_weekly`, default true). The alert state machine (`alerts.rs`) is pure
and keyed by `(quota key, resets_at, level)` with an in-memory `HashSet`.

## Initial Request

Build v1.2 per the approved design spec
(`docs/superpowers/specs/2026-09-17-threshold-alerts-design.md`) and implementation plan
(`docs/superpowers/plans/2026-09-17-threshold-alerts.md`): add per-quota alert settings, the
pure alert state machine, wire notification delivery and the title marker, then dock in docs.

## Acceptance Criteria

- [x] Notification fires at session 80/95 and weekly 95, once per `(quota, reset window,
      level)`, via `tauri-plugin-notification` called from Rust only.
- [x] 80% level suppressed while `percent <= elapsed_pct` (usage behind the clock); 95% always
      fires.
- [x] `⚠` prefixes the tray title while any alerting quota is at or above 95%.
- [x] `Settings` gains `alert_session`, `alert_weekly` (default `true`); tray **Alerts**
      submenu with `Session`/`Weekly` check items toggles and persists them.
- [x] `resets_at` sub-second jitter never causes a duplicate or missed fire for the same
      window — see Final Notes.
- [x] No capability entry added for notifications (Rust-side plugin call bypasses the webview
      capability system) — an initial `notification:default` entry added during
      implementation was found unused and removed before merge.
- [x] `pnpm verify` passes (12 vitest, 49 cargo).

## Plan

Full step-by-step plan lives at `docs/superpowers/plans/2026-09-17-threshold-alerts.md`
(settings, alerts state machine, title marker + menu, plugin/capability/delivery, docs).

## Execution Log

Branch `feat/alerts`, squash-merged to `main` as PR #6 (`a157fb5`).

- 2026-09-17 (`1b57088`): docs — added the threshold alerts design.
- 2026-09-17 (`59cdd78`): docs — added the threshold alerts implementation plan.
- 2026-09-17 (`7a410fb`): feat — added per-quota alert settings (`alert_session`,
  `alert_weekly`) to `Settings`.
- 2026-09-17 (`e212997`): feat — added the threshold alert state machine (`alerts.rs`): pure,
  keyed by `(quota key, resets_at, level)`.
- 2026-09-17 (`250df1d`): feat — threshold notifications with the `⚠` title marker and Alerts
  toggles wired into the tray menu.
- 2026-09-17 (`39ad050`): docs — described threshold alerts in CLAUDE.md/README.
- 2026-09-17 (`0f08e84`): fix — treat a jittered `resets_at` as the same alert window (see
  Final Notes).
- 2026-09-17 (`fbdd50a`): chore — dropped the unused `notification:default` capability entry
  and aligned docs (Rust-side plugin calls need no capability grant).
- 2026-09-17 (`cdd2dc5`): docs — dropped the stale capability mention from the alerts spec.
- 2026-09-17 (`a157fb5`): squash-merged as PR #6.

## Files Changed

- `src-tauri/src/alerts.rs` (new) — pure alert state machine: `AlertState` (`HashSet<(String,
  i64, u8)>`), `already_fired` with a jitter tolerance, `evaluate()` producing `Alert`s.
- `src-tauri/src/settings.rs` — `alert_session`, `alert_weekly` fields (default `true`).
- `src-tauri/src/tray.rs` — `⚠` title marker logic; **Alerts** submenu (`Session`, `Weekly`
  check items) wired to the settings toggle handler.
- `src-tauri/src/main.rs` — wires `AlertState` into managed state; calls `alerts::evaluate`
  from the poll callback and delivers via `tauri-plugin-notification`.
- `src-tauri/src/lib.rs` — module wiring for `alerts`.
- `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` — `tauri-plugin-notification` 2.4 dependency.
- `.changeset/threshold-alerts.md` — minor changeset.
- `CLAUDE.md` — Layout gains `src/alerts.rs`; Tauri Rules note notifications need no
  capability entry; status line updated to v1.2.
- `README.md` — describes the alert levels, once-per-window behaviour, and the Alerts menu.
- `docs/superpowers/specs/2026-09-17-threshold-alerts-design.md`,
  `docs/superpowers/plans/2026-09-17-threshold-alerts.md` — design and plan.

## Testing

- `pnpm verify`: 12 Vitest, 49 cargo tests, all green.
- `tauri dev`: menus render, no panic.
- Deferred to a human with a real session: cross 80% and confirm one banner; toggle Alerts →
  Session off and confirm no banner.

## Final Notes

- `resets_at` jitters sub-second between polls (per `docs/research-usage-monitors.md`), so a
  naive equality key on `(quota, resets_at, level)` could treat the same reset window as two
  different windows across polls and re-fire, or (worse) treat two genuinely different windows
  as the same and suppress a real alert. Fixed in `already_fired` with a ±60s tolerance: two
  `resets_at` values are the same window if `(r1 - r2).abs() <= SAME_WINDOW_SECS` (60).
- An initial `notification:default` capability entry was added while wiring the plugin, then
  found unnecessary and removed: `tauri-plugin-notification` is called from the Rust side only
  (poll callback), and Rust-side plugin calls bypass the webview capability system entirely —
  the macOS permission prompt is OS-level, not a Tauri capability. `CLAUDE.md`'s Tauri Rules
  section was updated to say so explicitly.
- Notification click handling, custom thresholds, popover in-app banners, and per-model quota
  alerts are explicitly out of scope for this cycle.
