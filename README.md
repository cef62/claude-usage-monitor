# Claude Usage Monitor

A macOS menu bar / Windows system tray app that shows your Claude plan usage: percentage
consumed in the 5-hour session and weekly windows, and the countdown to each reset. Click it for
a popover with usage bars, an elapsed-time marker, and links to the claude.ai usage and billing
pages.

Built with Tauri 2, Rust, React and TypeScript. No third-party UI libraries.

## How it works

The app reads the Claude Code OAuth token from the macOS Keychain (service
`Claude Code-credentials`), calls `https://api.anthropic.com/api/oauth/usage` every 3 minutes,
and shows the percentages the API reports. It never stores or logs the token. If you are not
logged in to Claude Code the menu bar shows `◷ —`; if the token has expired it shows
`◷ ! login`. In both cases run `claude auth login` and the app recovers on its own.

Menu bar format: `◷ 48% ↻2h13m  ·  ▦ 64% ↻3d4h` (session · weekly). ` (429)` after the text
means the API is rate limiting us and the numbers may be a few minutes old.

On Windows the tray shows no text, so the icon carries the numbers: the top bar is the session
quota, the bottom bar the weekly quota, green while behind the clock, amber from 80 %, red when
ahead of the clock or full. A red frame means an alert level has been reached; a red square in
the middle means you need to run `claude auth login`. Hover the icon for the same text macOS
shows in the menu bar. The token is read from `%USERPROFILE%\.claude\.credentials.json`
(`%CLAUDE_CONFIG_DIR%` if set).

## Configuration

Everything lives in the tray right-click menu and persists in
`~/Library/Application Support/com.matteo.claude-usage-monitor/settings.json` (macOS) or
`%APPDATA%\com.matteo.claude-usage-monitor\settings.json` (Windows):

- **Menu bar** (**Tray** on Windows) — Session, Weekly, Glyphs (`◷` / `▦`), Percent, Remaining
  time. At least one quota and one of Percent/Remaining always stay on. On Windows Session/Weekly
  also hide the matching bar in the icon; the other three shape the tooltip.
- **Popover** — Time ticks (hour/day divisions), Elapsed marker (the white line: how far through
  the reset window you are), Threshold marks (coloured ticks under the bar at each alert level).
- **Alerts** — per-quota on/off; **Session levels** `80/95` · `50/80/95` · `90/95`; **Weekly
  levels** `95` · `80/95` · `90`; **Send test notification**. The highest level always fires and
  shows `⚠` in the menu bar; lower levels fire only while usage is ahead of the elapsed time. One
  notification per level per reset window.
- **Check every** — 3 / 5 / 10 / 15 minutes. Below 120 s the API rate-limits, so that is the floor.
- **Help → Open log** — reveals `claude-usage-monitor.log` (poll results, alerts, settings
  changes; never your token). Rotates at 1 MB to `.log.1`. Attach it when reporting a problem.

Hand-editing `settings.json` is fine: `session_levels`/`weekly_levels` accept any 1–100 values,
`poll_interval_secs` is clamped to 120–900.

## Development

Requires Node 24+, pnpm 12, and a Rust stable toolchain (`rustup`).

```bash
pnpm install
pnpm tauri dev      # run the app
pnpm dev            # popover UI alone in a browser, with fixture data
pnpm verify         # biome, tsc, vitest, cargo fmt/clippy/test
pnpm tauri build    # .app and .dmg under src-tauri/target/release/bundle/
```

The app is ad-hoc signed, not notarized. On first launch macOS says it cannot verify the
developer: open **System Settings → Privacy & Security** and click **Open Anyway**, or run
`xattr -cr "/Applications/Claude Usage Monitor.app"` once.

## Releases

Every user-visible change lands with a changeset (`pnpm changeset`). On `main`, the Changesets
bot keeps a "Version Packages" pull request up to date; merging it bumps `package.json` and
`src-tauri/tauri.conf.json`, writes `CHANGELOG.md`, and pushes a `vX.Y.Z` tag. The tag builds
the Apple Silicon app and the Windows x64 installer and publishes a GitHub
Release with the `.dmg`, `.app.tar.gz` and `-setup.exe`.

Download the latest build from the Releases page. The app is ad-hoc signed, not notarized. On
first launch macOS says it cannot verify the developer: open **System Settings → Privacy &
Security** and click **Open Anyway**, or run
`xattr -cr "/Applications/Claude Usage Monitor.app"` once.

On Windows the installer is unsigned: when SmartScreen appears, click **More info → Run anyway**.
It installs per user (no admin prompt) and needs the WebView2 runtime, which Windows 10/11 ship.

## Not yet

Launch at login, code signing, Windows ARM64, Linux.
