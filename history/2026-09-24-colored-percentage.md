---
type: "feature"
status: "complete"
files:
  - src/App.tsx
  - src/app.css
  - src/lib/format.ts
  - src/lib/quota.ts
  - src-tauri/src/settings.rs
  - src-tauri/src/tray.rs
  - src-tauri/Cargo.toml
areas:
  - popover
  - tray
  - settings
components:
  - quota-card
  - menu-bar-title
tags:
  - thresholds
  - nsattributedstring
---

# feat: colour the percentages by alert threshold

| Field       | Value                |
| ----------- | -------------------- |
| **Status**  | complete             |
| **Branch**  | `feat/color-percent` |
| **Ticket**  | none                 |
| **Created** | 2026-09-24           |
| **Updated** | 2026-09-24           |

## Summary

New `color_percent` setting (default on, **Popover → Colored percentage**). Session and weekly
percentages turn amber (`--warn` / `systemOrangeColor`) at the lowest alert level and red at
the highest; a single level goes straight to red, like its threshold mark. Rule lives twice,
kept in step: `levelColor` (`src/lib/format.ts`) and `level_tint` (`tray.rs`).

- Popover: class on `.pct`. Forecast line gets `margin-top: 4px` (it sat on the sparkline).
- macOS menu bar: `set_title` is a plain string, so `refresh` re-sets the button title as an
  `NSAttributedString` via `with_inner_tray_icon` → `ns_status_item()`. `percent_tints`
  (pure, tested) returns UTF-16 ranges. objc2 crates are direct deps now with
  `default-features = false`; same versions tray-icon already uses, no new crates in the lock.
- Windows tray icon unchanged on purpose: its bars already carry pace colours.

## Decisions

- Reuse the existing amber rather than a true yellow (legibility on light backgrounds).
- Per-model weekly cards and extra usage have no alert levels, so they stay uncoloured.
