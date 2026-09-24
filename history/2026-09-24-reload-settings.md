---
type: "feature"
status: "complete"
files:
  - src-tauri/src/settings.rs
  - src-tauri/src/tray.rs
  - README.md
areas:
  - settings
  - tray
components:
  - help-menu
tags:
  - settings-json
related-to:
  - history/2026-09-24-colored-percentage.md
---

# feat: Help → Reload settings

| Field       | Value                  |
| ----------- | ---------------------- |
| **Status**  | complete               |
| **Branch**  | `feat/reload-settings` |
| **Ticket**  | none                   |
| **Created** | 2026-09-24             |
| **Updated** | 2026-09-24             |

## Summary

Hand edits to `settings.json` needed a restart; any menu change in between even wrote the
in-memory settings back over them. New **Help → Reload settings** re-reads the file.

- `settings::try_load` returns why a file is unusable; `load` (startup) keeps defaulting.
- Success: replace the managed `Settings`, then `after_settings_change` (check marks, save the
  repaired file, popover event, title, log line).
- Failure (missing file, bad JSON): nothing changes and nothing is written, so a typo is never
  replaced by defaults. Notification + log line carry the serde error.

## Decisions

- Menu item, not a file watcher: no polling, no new dependency, no half-saved edits applied.
- No "Open settings file" item; the README lists the path.
