# Claude Usage Monitor

## What This Is

A small, minimal desktop app that shows the current Claude plan usage: percentage consumed in
the 5-hour session window and the weekly window, plus when each resets. It lives in the macOS
menu bar or the Windows system tray and opens a small floating window with the same numbers,
better presentation, and links to the Claude usage/settings pages online.

Stack: Tauri 2 (Rust shell) + React 19 + TypeScript + Vite. No third-party UI libraries.

**Status:** v1.4 (Windows system tray) implemented; released via the Changesets pipeline.
Specs: `docs/superpowers/specs/2026-09-16-usage-monitor-v1-design.md`,
`docs/superpowers/specs/2026-09-17-ci-release-design.md`,
`docs/superpowers/specs/2026-09-17-menu-bar-settings-design.md`,
`docs/superpowers/specs/2026-09-17-threshold-alerts-design.md`,
`docs/superpowers/specs/2026-09-17-config-and-log-design.md`,
`docs/superpowers/specs/2026-09-18-windows-tray-design.md`.

## Reference Material (read before writing code)

- `docs/research-usage-monitors.md` — findings from two prior-art repos: the usage endpoint,
  headers, response shape, rate-limit discipline, and UI ideas. This is the data-layer spec.
- Prior art (both MIT): `jens-duttke/usage-monitor-for-claude` (Python tray app; `api.py`,
  `cache.py`, `formatting.py` are the parts worth porting) and
  `claude-monitor/claude-monitor-browser-extension` (`background.js` limits normalization,
  `popup.js` sparkline).
- Tauri conventions borrowed from `WorkWave/continuous-flow-sdk` `packages/desktop`.

## Tech Stack

- Tauri 2 (`tauri`, `tauri-build`, `@tauri-apps/api`). Plugins only when a native feature needs
  them: `tauri-plugin-opener` for external links (see Tauri Rules), `tray-icon` feature for the
  menu bar / tray.
- Rust stable, edition 2021. `reqwest` (rustls, native certs so corporate proxies work) +
  `serde`/`serde_json` for the usage API. Keep the usage JSON as `serde_json::Value` at the
  edge and normalize into a small typed struct — the API adds code-named fields without notice.
- React 19 + TypeScript (strict, `noUncheckedIndexedAccess`, `isolatedModules`,
  `jsx: react-jsx`, `module: ESNext`, `moduleResolution: Bundler`). Vite 8.
- Package manager: pnpm (pinned via `packageManager`). Not Bun, not npm.
- Linting/formatting: Biome v2 (NOT ESLint/Prettier). 2-space indent, single quotes, trailing
  commas, semicolons always, `lineWidth` 100, `noUnusedImports: error`, `useConst: error`.
  Biome ignores `src-tauri/`.
- Tests: Vitest for TypeScript, `cargo test` for Rust. No Playwright, no Gherkin.
- Styling: plain CSS (one `src/app.css` or CSS modules), system font stack,
  `prefers-color-scheme` for light/dark, native `<dialog>`/`<details>` before custom widgets.

## Layout

```
CLAUDE.md, AGENTS.md
docs/                      research notes, design specs
history/                   history-driven-workflow records (see skill)
src/                       React frontend (Vite root)
  main.tsx, App.tsx, app.css
  lib/                     pure TS: formatting, countdown, quota normalization
  lib/ipc.ts               the ONLY file that calls invoke()/listen()
src-tauri/
  Cargo.toml, tauri.conf.json, build.rs
  capabilities/default.json
  src/main.rs              Tauri builder, managed state, commands
  src/lib.rs               modules live here so integration tests can import them
  src/usage.rs             credentials + HTTP + normalization
  src/poll.rs              poll loop, backoff, cooldown state machine
  src/tray.rs              tray icon, menu, title text, popover positioning
  src/settings.rs          menu bar display settings, JSON in app_data_dir
  src/alerts.rs            threshold alert state machine (pure)
  src/icon.rs              Windows tray icon renderer (pure RGBA)
  src/log.rs               capped local log (never the token)
  icons/
test/                      Vitest specs for src/lib
```

Ownership: **Rust owns credentials, network, polling, and OS integration (tray, notifications,
keychain). React owns rendering only.** The frontend never sees the OAuth token.

## Data Source (the part that is easy to get wrong)

- Endpoint: `GET https://api.anthropic.com/api/oauth/usage` (+ `/api/oauth/profile` once per
  token for plan name). Returns **percentages only**; there are no token counts anywhere.
- Headers: `Authorization: Bearer <accessToken>`, `anthropic-beta: oauth-2025-04-20`,
  `User-Agent: claude-code/<version>`. Any other User-Agent got permanently 429'd upstream.
- Token: macOS Keychain, service `Claude Code-credentials`, value is JSON
  `{"claudeAiOauth":{"accessToken",...}}`. Fallback `$CLAUDE_CONFIG_DIR/.credentials.json`
  (defaults to `~/.claude/.credentials.json`; this is the Windows/Linux path). Re-read on every
  poll; a missing/partial blob means "no token", never a crash.
- Quotas: prefer `limits[]` (`kind: session | weekly_all | weekly_scoped`, per-model via
  `scope.model.display_name`), fall back to flat `five_hour` / `seven_day` / `seven_day_*`.
  Drop entries with null utilization or no `resets_at`. Utilization can exceed 100; show the
  raw number, clamp only the bar fill. `resets_at` has sub-second jitter — never compare for
  equality.
- Rate limits (learned by the prior art through pain): base poll 180s (user-configurable
  120–900s), never two successful
  fetches closer than 120s, one in-flight request at a time, 429 → honor `Retry-After` else
  exponential backoff capped at 15 min, 401 → stop using that token until the credential store
  changes and tell the user to run `claude auth login`. No "refresh now" that bypasses the
  cooldown. Never spawn `claude update` to refresh a token.
- Never log or persist the token. Never send it anywhere but `api.anthropic.com`.

## Tauri Rules

- `tauri.conf.json` is the version anchor together with `package.json`; keep them equal.
  `Cargo.toml` version is inert.
- Capabilities are an allowlist in `src-tauri/capabilities/default.json`. Grant the minimum.
  A missing capability fails silently in the webview — check there first when an `invoke`
  rejects for no reason. `opener:allow-open-url` needs an explicit `allow: [{url: "https://*"}]`
  scope or it opens nothing. Notifications are sent from Rust (`tauri-plugin-notification`),
  which needs no capability entry; the macOS permission prompt is OS-level.
- External links (Claude settings/usage pages) open via `plugin:opener|open_url`, never by
  navigating the webview.
- Call plugin commands directly with `invoke('plugin:<name>|<cmd>')` from `src/lib/ipc.ts`
  rather than adding the plugin's JS package — one version to keep in step, not two.
- `#[tauri::command]` handlers return `Result<T, String>`; no `unwrap`/`expect` in command or
  poll code. Serde field names are `snake_case` on both sides (`#[serde(rename_all)]` only if
  the frontend needs camelCase, and then everywhere).
- Push state to the frontend with events (`app.emit`) from the poll loop; the frontend does not
  poll the backend on a timer.
- OS-specific code behind `#[cfg(target_os = "...")]`. The platform forks in the tray are
  `tray::refresh` (macOS: title; elsewhere: tooltip + `icon::render`), `DISPLAY_MENU_LABEL`, and
  the startup icon seed in `tray::setup`. Spawn `claude` through
  `cmd /C` on Windows (it is a `.cmd` shim) with `CREATE_NO_WINDOW`.
- Release profile: `opt-level = "s"`, `lto = true`, `codegen-units = 1`, `strip = true`,
  `panic = "abort"`. `main.rs` starts with
  `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`.
- The app is ad-hoc signed (`signingIdentity: "-"`), not notarized, until an Apple Developer
  account exists. No auto-updater until asked.

## Commands

```bash
pnpm install
pnpm tauri dev          # full app
pnpm dev                # frontend only in a browser (IPC stubbed)
pnpm check              # biome check .
pnpm check:fix          # biome check --write .
pnpm typecheck          # tsc --noEmit
pnpm test:run           # vitest run
pnpm verify             # check + typecheck + test:run + cargo fmt --check + cargo clippy -D warnings + cargo test
pnpm tauri build
pnpm changeset          # add a changeset for a user-visible change (required in the PR)
```

## Versioning and Releases

- **Every user-visible change ships with a changeset** in the same PR (`pnpm changeset`,
  file under `.changeset/`). Docs-only changes that alter what a user is told to do count.
- `package.json` is the version source. `pnpm version-packages` runs `changeset version` and
  `scripts/sync-version.mjs`, which mirrors the version into `src-tauri/tauri.conf.json`.
  `Cargo.toml` stays `0.0.0`; nothing reads `CARGO_PKG_VERSION`.
- Workflows: `ci.yml` (verify + build on PRs and `main`), `release.yml` (Changesets action:
  opens the Version Packages PR; on its merge runs `changeset git-tag`, pushes the tag and
  dispatches `build-release.yml`), `build-release.yml` (dispatched by `release.yml`, or any
  manual `v*` tag push: `tauri-action` builds `aarch64-apple-darwin` and publishes the GitHub
  Release, then a second job adds the Windows x64 NSIS installer).
- Never edit versions by hand; never create tags by hand.

## Code Conventions

- Imports use the `@/*` alias for anything beyond `./`.
- Pure logic (countdown, elapsed fraction, quota normalization, formatting) lives in `src/lib/`
  as plain functions with a Vitest spec in `test/`. Components stay thin.
- Rust modules get inline `#[cfg(test)]` tests for parsing and state-machine logic.
- Tests go in `test/` (TS) and inline (Rust). No snapshot tests.
- Comments explain why, not what.

## What NOT to Do

- Don't add UI libraries, icon packs, CSS frameworks, or state libraries. React + CSS only.
- Don't use ESLint or Prettier — use Biome.
- Don't read `~/.claude/projects/*.jsonl` or shell out to `ccusage`; the API is the source.
- Don't fetch from the frontend. All HTTP goes through Rust.
- Don't add a "refresh now" that ignores the cooldown.
- Don't use `localStorage` for anything that must survive; settings live in the Rust side
  (`app_data_dir`).

## Workflow Standards

### Required Skills (mandatory in every task — never skip, never ask permission)
1. **superpowers:brainstorming** — before any creative/implementation work
2. **superpowers:writing-plans** — before multi-step implementation
3. **superpowers:executing-plans** — when implementing from a plan
4. **superpowers:test-driven-development** — before writing implementation code
5. **history-driven-workflow** — for planning, execution tracking, and PR prep
6. **quality-gate** — automatically before commit
7. **superpowers:finishing-a-development-branch** — when work is complete

### Always-On Modes
- **caveman:caveman** (level `full`) — terse replies in chat. Prose written to files (code,
  comments, commits, docs, PR/issue bodies) stays normal English.
- **ponytail:ponytail** (level `full`) — laziest solution that works. Climb the ladder: does it
  need to exist, is it already in the codebase, stdlib, native platform feature, installed
  dependency, one line — only then new code. Never lazy about understanding the problem,
  validation, security, or accessibility.

### Feature Branch Policy
**Hard block:** Never start implementation on `main`. Always create a feature branch first
(`superpowers:using-git-worktrees` or `git checkout -b <type>/<short-description>`). The only
exception is if the user explicitly says to work on main in the current conversation.

### History Context
Before starting any task, search for relevant past work:
- `claude-mem` MCP search tools for cross-session memory
- `history-search` skill for records in `./history/`
- Present findings to the user before proceeding

### Clarification Questions
Ask at least one clarifying question before starting non-trivial work. Prefer multiple-choice
(AskUserQuestion) over open-ended. Never assume scope, approach, or acceptance criteria.

### Quality Gate (automatic)
Runs at task completion, before commit/PR:
1. `pnpm verify` passes (biome, tsc, vitest, cargo fmt/clippy/test)
2. `pnpm tauri build` compiles when `src-tauri/` changed
3. README/docs reflect new behavior or settings
4. No unused dependencies in `package.json` or `Cargo.toml`, no orphaned files
5. Capabilities file grants nothing the code does not use

Do NOT skip these checks. Do NOT ask the user whether to run them — they are mandatory.

## History-Driven Workflow Configuration

| Setting                     | Value          |
| --------------------------- | -------------- |
| **History folder**          | `./history/`   |
| **JIRA project**            | none           |
| **Auto-search on planning** | `true`         |
| **Search depth**            | `summary`      |
| **Min relevance score**     | `2`            |
