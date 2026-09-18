# claude-usage-monitor

## 0.5.0

### Minor Changes

- f678f61: Windows support: system tray icon with session/weekly bars and a tooltip, popover above the taskbar, and an unsigned x64 installer attached to every release.

## 0.4.0

### Minor Changes

- ed428a5: Configurable alert levels (presets), poll interval (3–15 min), and popover overlays from the tray menu; threshold marks on the popover bars; "Send test notification"; a local log with Help → Open log.

## 0.3.0

### Minor Changes

- a157fb5: Threshold alerts: macOS notifications at 80% and 95% session usage and 95% weekly usage, once per reset window (80% only when usage is ahead of the clock), a `⚠` menu bar marker at 95%, and per-quota on/off under right-click → Alerts.

## 0.2.0

### Minor Changes

- 2ba62fa: Configurable menu bar: choose Session/Weekly, Glyphs, Percent and Remaining time from the tray menu (persisted). Monochrome glyphs and wider spacing. The macOS bundle is now ad-hoc signed so downloaded builds open via "Open Anyway" instead of reporting "damaged".

## 0.1.0

### Minor Changes

- 5a40503: First release: macOS menu bar app showing Claude session and weekly usage with reset countdowns, and a popover with usage bars, links to the claude.ai usage and billing pages, and a status footer.
