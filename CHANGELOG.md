# claude-usage-monitor

## 0.8.0

### Minor Changes

- 0ce186e: In-app updates: a daily check of the GitHub Releases feed, one notification per new version, and Help → Install update… to download, install and relaunch. Help → Check for updates… checks on demand.

## 0.7.0

### Minor Changes

- 1c536f3: "Notify on reset": a notification when a quota that had reached its top alert level rolls into a new window, once per reset; toggle under Alerts, on by default.

## 0.6.1

### Patch Changes

- ecbd9b6: Release workflow creates the GitHub Release once and builds macOS and Windows in parallel, so one platform failing no longer blocks the other's assets. CI actions bumped off the deprecated Node 20 runtime.

## 0.6.0

### Minor Changes

- 530d91f: "Start at login" check item in the tray menu (macOS login item via LaunchAgent, Windows Run key); off by default, the OS state is the source of truth.

### Patch Changes

- 3bac3b2: README rewritten around install / use / configure with a Releases link and acknowledgements; added CONTRIBUTING.md and the MIT license.

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
