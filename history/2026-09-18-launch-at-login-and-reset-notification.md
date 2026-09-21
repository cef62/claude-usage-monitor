---
type: "feat"
status: "complete"
files:
  - src-tauri/src/tray.rs
  - src-tauri/src/alerts.rs
  - src-tauri/src/main.rs
  - src-tauri/src/settings.rs
  - src-tauri/Cargo.toml
  - src-tauri/Cargo.lock
  - .changeset/launch-at-login.md
  - .changeset/reset-notification.md
  - CLAUDE.md
  - README.md
areas:
  - tray
  - core
  - docs
components:
  - autostart
  - reset-notification
  - alert-state-machine
tags:
  - tauri
  - rust
  - macos
  - windows
  - notifications
related-to:
  - README.md#alerts
  - README.md#configure
  - history/2026-09-18-windows-tray.md
  - history/2026-09-17-threshold-alerts.md
---

# feat: Start at login + reset notification

| Field       | Value                                    |
| ----------- | ----------------------------------------- |
| **Status**  | complete                                   |
| **Branch**  | `feat/launch-at-login` (PR #15), `feat/reset-notification` (PR #18) |
| **Ticket**  | none                                       |
| **Created** | 2026-09-18                                 |
| **Updated** | 2026-09-18                                 |

## Summary

Two small, bounded features shipped same-day without a design spec (below the threshold that
requires one): "Start at login" and "reset" notifications. **Start at login** adds a check item
to the tray menu backed by `tauri-plugin-autostart` (macOS LaunchAgent / Windows `HKCU\…\Run`);
it stores nothing in `settings.json` — the OS login-item registry is the only source of truth,
so toggling it always shows what the OS actually reports, even if a previous call failed.
**Reset notification** extends the pure alert state machine (`alerts.rs`) with an `armed: HashMap<String,
i64>` that records the window in which a quota last hit its *top* alert level; when that quota's
`resets_at` rolls into a new window (a genuine reset, not `resets_at` jitter), a one-time "back
to 0%" notification fires, gated by a new `alert_reset` setting (default on, under the Alerts
submenu). Arming survives the per-quota alert toggles being off, so a notification that armed
while alerts were on still delivers if they're re-enabled before the reset.

## Initial Request

Two independent, scoped feature requests handled without the full brainstorm/spec/plan
machinery (small enough to be "bounded path" per the project's workflow): add a "Start at
login" toggle, and add a notification when a quota resets after having alerted at its top
level.

## Acceptance Criteria

Start at login (PR #15):
- [x] Tray menu check item "Start at login", reflecting `app.autolaunch().is_enabled()` at
      menu build time.
- [x] Toggling calls `enable()`/`disable()` on the plugin, then re-reads `is_enabled()` and
      sets the check mark from that result — a failed OS call never leaves a lying check mark.
- [x] No `Settings` field added; nothing persisted by this app — the OS registry is the sole
      source of truth.
- [x] Off by default (`tauri-plugin-autostart` does not register anything until the user
      toggles it on).
- [x] Errors are logged (`log::write`), never panic.

Reset notification (PR #18):
- [x] `AlertState` gains `armed: HashMap<String, i64>`, set when a quota's `evaluate()` call
      hits its top level (`highest == top_level`).
- [x] `alerts::resets()` returns quotas whose `resets_at` has moved past the armed window by
      more than the existing jitter tolerance (`SAME_WINDOW_SECS`), clearing the arm and firing
      once.
- [x] Gated by `settings.alert_reset` (default `true`) and the existing per-quota
      `alert_session`/`alert_weekly` toggles; arming itself is independent of those toggles so a
      later re-enable still delivers.
- [x] `resets_at` jitter never falsely triggers a reset (reuses the same tolerance as
      `already_fired`).
- [x] New tray menu item under **Alerts**: "Notify on reset", on by default.

## Plan

No written implementation plan — both were executed directly as single-commit bounded changes
per the project's workflow rules for small, well-scoped work (no spec/plan required below the
complexity threshold).

## Execution Log

- 2026-09-18 (`dfc4b0c` on `feat/launch-at-login`, squash-merged as PR #15 `530d91f`): feat —
  "Start at login" check item backed by `tauri-plugin-autostart`; OS is the source of truth, no
  settings field.
- 2026-09-18 (`9b42dd6` on `feat/reset-notification`, squash-merged as PR #18 `1c536f3`): feat
  — reset notification after a quota reached its top alert level; `AlertState.armed`,
  `alerts::resets`, `alert_reset` setting and menu item.

## Files Changed

Start at login (PR #15, `530d91f`):
- `src-tauri/src/tray.rs` — `MenuAction::Autostart`, `on_autostart_toggled` (flips the OS login
  item then re-reads it), "Start at login" `CheckMenuItem` wired into the tray menu.
- `src-tauri/src/main.rs` — registers `tauri-plugin-autostart`.
- `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` — `tauri-plugin-autostart` dependency.
- `.changeset/launch-at-login.md` — minor changeset.
- `CLAUDE.md` — mentions `tauri-plugin-autostart` for "Start at login", OS as source of truth.
- `README.md` — "Start at login" row in the Configure table.

Reset notification (PR #18, `1c536f3`):
- `src-tauri/src/alerts.rs` — `AlertState.armed: HashMap<String, i64>`; `pub struct Reset`;
  `pub fn resets(...)`, armed when a quota hits its top level in `evaluate()`.
- `src-tauri/src/settings.rs` — `alert_reset: bool` (default `true`).
- `src-tauri/src/tray.rs` — "Notify on reset" check item under **Alerts**.
- `src-tauri/src/main.rs` — calls `alerts::resets` from the poll callback, delivers via
  `tauri-plugin-notification`.
- `.changeset/reset-notification.md` — minor changeset.
- `README.md` — Alerts section describes the reset notification and "Notify on reset" toggle.

## Testing

- `pnpm verify` green for both PRs (existing suite; reset-notification added three new inline
  `alerts.rs` tests: `reset_fires_once_after_the_top_level_was_reached`,
  `reset_needs_the_top_level_not_just_any_alert`, `reset_ignores_jitter_and_respects_toggles`).
- Manual: toggled "Start at login" in the installed app and confirmed the OS registers/removes
  the login item (macOS System Settings → Login Items; Windows `HKCU\…\Run`); confirmed the
  check mark matches the OS state across a relaunch.
- Manual: forced a quota through its top level, waited for a window rollover, confirmed the
  reset notification fires exactly once and respects the `alert_reset` toggle.

## Final Notes

- These two features shipped alongside two sibling PRs from the same days, not otherwise
  recorded: PR #12 (`3bac3b2`, `docs: README, CONTRIBUTING and MIT license`) rewrote the README
  around install/use/configure, added `CONTRIBUTING.md`, the MIT `LICENSE`, and app screenshots
  under `docs/screenshots/`; PR #16 (`ecbd9b6`, `ci: parallel release builds, action bumps,
  tray hardening`) restructured `build-release.yml` so the GitHub Release is created once and
  macOS/Windows build in parallel (one platform's failure no longer blocks the other's
  assets), bumped CI actions off the deprecated Node 20 runtime, and made small tray hardening
  fixes alongside. Neither of those two needed its own history record: #12 is docs-only
  reorganization and #16 is CI-only housekeeping with no user-facing behavior change beyond the
  changeset text already captured in `CHANGELOG.md`.
- "Start at login" deliberately stores no state of its own — a `Settings` boolean would drift
  from reality the moment the OS-level registration fails or the user removes it outside the
  app (e.g., via System Settings directly). Reading `is_enabled()` fresh at menu-build and
  post-toggle time avoids that class of bug entirely.
- Custom reset-notification thresholds, snoozing, and persisting the armed state across
  restarts are out of scope; `armed` is in-memory only, matching `AlertState.fired`.
