# Claude Usage Monitor

A macOS menu bar app that shows your Claude plan usage: percentage consumed in the 5-hour
session and weekly windows, and the countdown to each reset. Click it for a popover with
usage bars, an elapsed-time marker, and links to the claude.ai usage and billing pages.

Built with Tauri 2, Rust, React and TypeScript. No third-party UI libraries.

## How it works

The app reads the Claude Code OAuth token from the macOS Keychain (service
`Claude Code-credentials`), calls `https://api.anthropic.com/api/oauth/usage` every 3 minutes,
and shows the percentages the API reports. It never stores or logs the token. If you are not
logged in to Claude Code, or the token has expired, the menu bar shows `⏱ ! login`; run
`claude auth login` and the app recovers on its own.

Menu bar format: `⏱ 48% ↻2h13m · 📅 64% ↻3d4h` (session · weekly). ` (429)` after the text
means the API is rate limiting us and the numbers may be a few minutes old.

## Development

Requires Node 24+, pnpm 12, and a Rust stable toolchain (`rustup`).

```bash
pnpm install
pnpm tauri dev      # run the app
pnpm dev            # popover UI alone in a browser, with fixture data
pnpm verify         # biome, tsc, vitest, cargo fmt/clippy/test
pnpm tauri build    # .app and .dmg under src-tauri/target/release/bundle/
```

The app is unsigned. On first launch, right-click the `.app` and choose Open.

## Not yet

Threshold notifications, settings, Windows tray icon, launch at login, code signing.
