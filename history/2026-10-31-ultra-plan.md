---
type: "feature"
status: "complete"
files:
  - src-tauri/src/tray.rs
  - src-tauri/src/icon.rs
  - src-tauri/src/usage.rs
  - src-tauri/src/poll.rs
  - src/lib/ipc.ts
  - README.md
  - site/src/pages/index.astro
  - CLAUDE.md
areas:
  - tray
  - usage
  - poll
components:
  - menu-bar-title
  - windows-icon
  - plan-label
tags:
  - ultra
  - profile
related-to:
  - history/2026-09-24-colored-percentage.md
---

# feat: support the Ultra plan (no 5-hour session)

| Field       | Value             |
| ----------- | ----------------- |
| **Status**  | complete          |
| **Branch**  | `feat/ultra-plan` |
| **Ticket**  | none              |
| **Created** | 2026-10-31        |
| **Updated** | 2026-10-31        |

## Summary

On 2026-10-30 Anthropic renamed Max 20x to Ultra and Max 5x to Max. Verified Ultra accounts
have no 5-hour session quota, so the menu bar showed a permanent `◷ —` and the Windows icon an
empty top bar. The popover was already data-driven.

- `tray::shown_keys`: enabled halves whose quota exists; none → whichever of session/weekly
  exists; no data yet → the enabled placeholders. `title`, `percent_tints` and `icon::render`
  all use it, so title order, tint ranges and bars agree.
- `usage::plan_label`: `max_5x` → Max, `max_20x` → Ultra, any tier containing `ultra` → Ultra
  (a `shortcut:` guess). `usage::unknown_tier` feeds a one-off `profile tier <x>` log line.
- Poll: `profile_for` is set after any non-network profile outcome, so a 403/429/500 profile
  endpoint is not re-hit every poll; an attempted profile always replaces the plan, so a
  switched account never keeps the previous plan name.
- Removed the stray `mod.rs.orig` from the vendored tray-icon.

## Decisions

- No plan check: hide the session wherever the API omits it. Idle Max/Pro accounts without a
  session window also see weekly-only until a session starts.
- Session-only data with both halves enabled now draws one centred bar, same as weekly-only.

## Follow-ups

- Add new tier strings to `known_tier` when a user's log shows `profile tier …`.
- Recheck the Ultra `limits[]` fixture in `usage.rs` against a real response.
