# claude-usage-monitor

## 0.11.5

### Patch Changes

- e46dfac: The "update available" notification shows the release's headline again: the update feed was published with empty notes since 0.11.3.

## 0.11.4

### Patch Changes

- 24b8753: Point Claude Desktop users to a one-time Claude Code login: the "no login" banner says how, and the popover links to the Claude Code install docs.

## 0.11.3

### Patch Changes

- 2aea38e: Rotate the update signing key after the old one was exposed in the repository. Install this version before the next one: 0.11.2 and older cannot install releases signed with the new key, so from 0.11.4 on they need a manual download from the Releases page.

## 0.11.2

### Patch Changes

- ce062ae: Project website at https://cef62.github.io/claude-usage-monitor/ with downloads, guide and changelog.

## 0.11.1

### Patch Changes

- 32fc070: settings.json and history.json are written atomically (temp file + rename), so a crash mid-write can no longer reset settings or the history. Dependencies refreshed (Tauri 2.11.6, updater plugin 2.12); Dependabot keeps actions and crates current.

## 0.11.0

### Minor Changes

- 0896f92: Sparkline under each quota bar showing how usage grew over the current reset window (persisted in history.json, cleared at reset); Popover → "History line" toggles it.

## 0.10.0

### Minor Changes

- 1ad8854: The popover names your plan (read once per login from the profile endpoint) and shows an "Extra usage" card with the overage credits used this month when your account has extra usage enabled.

## 0.9.1

### Patch Changes

- 2a395a2: macOS 27: left click opens the popover again (the menu no longer swallows the click). Ships tray-icon 0.24.2 with the upstream fix applied until Tauri picks up tray-icon 0.25.

## 0.9.0

### Minor Changes

- 62c4a1b: About view: Help → About… (or the version in the popover footer) shows the installed version with links to the repository, release notes, issue tracker and license.

## 0.8.2

### Patch Changes

- f083bf9: Update notifications show the first line of the release notes; Help → "Check for updates automatically" can turn the daily check off. Release pages now carry the CHANGELOG entry.

## 0.8.1

### Patch Changes

- 89787a7: Updater: the daily check keeps its schedule across system sleep, clicking Install during a check says "Update in progress…". CI actions are pinned to commit SHAs.
- 272fe1d: Release workflow creates the GitHub Release as a draft and publishes it after both platform builds upload, since GitHub now makes published releases immutable.

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
