---
type: "feat"
status: "complete"
files:
  - src-tauri/src/log.rs
  - src-tauri/src/settings.rs
  - src-tauri/src/alerts.rs
  - src-tauri/src/poll.rs
  - src-tauri/src/tray.rs
  - src-tauri/src/main.rs
  - src-tauri/src/lib.rs
  - src/App.tsx
  - src/app.css
  - src/lib/format.ts
  - src/lib/ipc.ts
  - src/lib/quota.ts
  - test/format.test.ts
  - .changeset/config-and-log.md
  - CLAUDE.md
  - README.md
  - docs/superpowers/specs/2026-09-17-config-and-log-design.md
  - docs/superpowers/plans/2026-09-17-config-and-log.md
areas:
  - core
  - tray
  - frontend
  - docs
components:
  - settings
  - local-log
  - popover-overlays
  - poll-loop
tags:
  - tauri
  - rust
  - react
  - typescript
  - macos
related-to:
  - docs/superpowers/specs/2026-09-17-config-and-log-design.md
  - docs/superpowers/plans/2026-09-17-config-and-log.md
  - history/2026-09-17-threshold-alerts.md
---

# feat: configurable levels, interval, popover overlays and local log

| Field       | Value                                    |
| ----------- | ----------------------------------------- |
| **Status**  | complete                                   |
| **Branch**  | `feat/config-and-log` (squash-merged as PR #8) |
| **Ticket**  | none                                       |
| **Created** | 2026-09-17                                 |
| **Updated** | 2026-09-17                                 |

## Summary

Shipped v1.3: configuration and a local log. Alert thresholds (session `80/95` default,
`50/80/95`, `90/95`; weekly `95` default, `80/95`, `90`) and the poll interval (3/5/10/15 min
presets, clamped 120–900s) are now radio-button choices under the tray menu, persisted in
`settings.json` and read by the poll thread each cycle. The `⚠` marker and the "always fires"
rule now key off `top = max(levels)` per quota rather than a fixed 95; levels below `top` stay
time-aware. Three popover overlay toggles (time ticks, elapsed marker, threshold marks) were
added, plus threshold marks drawn on the bars themselves. A "Send test notification" menu item
and a capped local log (`app_data_dir/claude-usage-monitor.log`, rotated at 1 MB to `.log.1`,
revealed via Help → Open log) round out the cycle.

## Initial Request

Build v1.3 per the approved design spec
(`docs/superpowers/specs/2026-09-17-config-and-log-design.md`) and implementation plan
(`docs/superpowers/plans/2026-09-17-config-and-log.md`): make thresholds/interval/overlays
configurable, generalize the alert marker to the configured top level, add the local log, draw
overlays in the popover, then dock in docs.

## Acceptance Criteria

- [x] Session/weekly threshold presets and poll-interval presets are tray radio submenus,
      persisted in `settings.json`; hand-editable values in the file still work.
- [x] `⚠` marker and "always-fires" level are `top = max(levels)` per quota; levels below `top`
      remain time-aware (`percent > elapsed_pct`), matching the v1.2 rule exactly at the edges.
- [x] Poll interval stored as `poll_interval_secs`, clamped `120..900`; cooldown and 429 backoff
      behavior unchanged.
- [x] Popover gains three independent overlay toggles (time ticks, elapsed marker, threshold
      marks) and renders configured threshold marks on the bars.
- [x] "Send test notification" tray item fires a real notification through the same delivery
      path as a threshold alert.
- [x] Local log at `app_data_dir/claude-usage-monitor.log`, rotates to `.log.1` at 1 MB, never
      contains the token; a single `write_all` per append (no partial-line interleaving); Help
      → Open log reveals it via `tauri_plugin_opener::reveal_item_in_dir` (no capability).
- [x] `pnpm verify` passes (13 vitest, 68 cargo).

## Plan

Full step-by-step plan lives at `docs/superpowers/plans/2026-09-17-config-and-log.md`
(settings + poll interval + overlays, alert-levels-from-settings, log module, tray menu
presets, popover overlays, docs).

## Execution Log

Branch `feat/config-and-log`, squash-merged to `main` as PR #8 (`ed428a5`).

- 2026-09-17 (`dc8c942`): docs — added the configuration, bar overlays and log design.
- 2026-09-17 (`7afe20c`): docs — added the configuration and log implementation plan.
- 2026-09-17 (`9a42267`): feat — configurable alert levels, poll interval and popover
  overlays in `Settings`.
- 2026-09-17 (`032346c`): feat — alert levels now come from settings, with the top level as
  the marker/always-fire threshold.
- 2026-09-17 (`4c4e747`): fix — kept the strict ahead-of-clock rule for lower alert levels
  (an implementer draft had loosened it — see Final Notes).
- 2026-09-17 (`8338355`): feat — local log with 1 MB rotation and the configurable poll
  interval wired into the poll loop.
- 2026-09-17 (`4b41fb3`): test — corrected RFC3339 fixture dates in the log tests (see Final
  Notes).
- 2026-09-17 (`80ed380`): feat — tray menu presets for levels and interval, "Send test
  notification", "Open log".
- 2026-09-17 (`eaa3595`): feat — popover overlay toggles and threshold marks on the bars.
- 2026-09-17 (`9711501`): docs — described the configuration menu, overlays and local log.
- 2026-09-17 (`1abb5da`): fix — single-write log append, skip logging refused toggles, pinned
  the poll interval max to the backoff cap.
- 2026-09-17 (`2cf8068`): docs — dropped shipped items from the README "not yet" list.
- 2026-09-17 (`ed428a5`): squash-merged as PR #8.

## Files Changed

- `src-tauri/src/log.rs` (new) — capped local log: single `write_all` append, 1 MB rotation to
  `.log.1`, never writes the token.
- `src-tauri/src/settings.rs` — threshold-level presets (session/weekly), `poll_interval_secs`
  (clamped 120..900), three overlay toggles; `Settings` clonable, not copyable.
- `src-tauri/src/alerts.rs` — marker/always-fire now `top = max(levels)` per quota instead of
  a fixed 95; time-aware rule kept strict for levels below `top`.
- `src-tauri/src/poll.rs` — reads `poll_interval_secs` from settings each cycle; interval
  clamp logic.
- `src-tauri/src/tray.rs` — radio submenus for level/interval presets, "Send test
  notification", "Open log" (Help submenu), overlay toggle menu items.
- `src-tauri/src/main.rs` — wires the log into managed state; startup + poll log lines.
- `src-tauri/src/lib.rs` — module wiring for `log`.
- `src/App.tsx`, `src/app.css` — renders the three popover overlays (time ticks, elapsed
  marker, threshold marks) from settings pushed over IPC.
- `src/lib/format.ts`, `src/lib/quota.ts`, `src/lib/ipc.ts` — types/formatting for the new
  settings fields.
- `test/format.test.ts` — coverage for the new formatting paths.
- `.changeset/config-and-log.md` — minor changeset.
- `CLAUDE.md` — status line updated to v1.3 (current); Layout/Commands unchanged beyond that.
- `README.md` — describes threshold presets, poll interval, overlays, test notification, and
  the log.
- `docs/superpowers/specs/2026-09-17-config-and-log-design.md`,
  `docs/superpowers/plans/2026-09-17-config-and-log.md` — design and plan.

## Testing

- `pnpm verify`: 13 Vitest, 68 cargo tests, all green.
- `tauri dev`: menus render; log file receives startup and poll lines.
- Deferred to a human: "Send test notification" shows a banner; Help → Open log reveals the
  file; presets persist across relaunch; overlays toggle live.

## Final Notes

- Rejected an implementer draft that loosened the ahead-of-clock rule to `percent - elapsed >
  2.0` (a fudge factor) for levels below the top threshold. The plan's own fixture,
  `session(92.0, 1800)`, was actually wrong — the correct elapsed value for that fixture is
  `1200`, not `1800`, which is what made the strict `percent > elapsed_pct` rule look like it
  was failing. Fixed the fixture, kept the strict rule (`fix: keep the strict ahead-of-clock
  rule for lower alert levels`, `4c4e747`) — no fudge factor shipped.
- A follow-up commit (`4b41fb3`) corrected RFC3339 fixture dates in the log rotation tests that
  had drifted from the fixed "now" the tests assume.
- Log append is a single `write_all` call per line (not a `write!` formatter followed by a
  separate newline write), so a concurrent poll-thread write can never interleave a partial
  line into the file.
- Numeric editors in the settings UI, Windows tray, notification click handling, per-model
  alerts, and log upload/sharing beyond "reveal in Finder" are explicitly out of scope.
