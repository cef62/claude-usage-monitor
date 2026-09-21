# Contributing

Thanks for taking a look. This is a small personal project, so the bar is "keep it small and
keep it working": bug fixes, platform fixes, and modest features that fit the scope in the README
are welcome. Open an issue first for anything bigger than a screen of code — it saves both of us
time if the idea does not fit.

## Prerequisites

- Node 26+ and pnpm 12 (`corepack enable` picks the pinned version from `package.json`).
- Rust stable via `rustup` (`src-tauri/rust-toolchain.toml` pins the channel and adds `rustfmt`
  and `clippy`).
- macOS 13+ with Xcode command line tools, or Windows 10/11 with the MSVC build tools and
  WebView2 (both preinstalled on recent Windows).
- [Claude Code](https://docs.claude.com/en/docs/claude-code) installed and logged in, so the app
  has a token to read while you develop.

## Setup and everyday commands

```bash
pnpm install
pnpm tauri dev      # the full app: menu bar / tray item + popover, hot reload for the UI
pnpm dev            # popover UI alone in a browser, with fixture data (no Rust, no token)
pnpm verify         # everything CI runs: biome, tsc, vitest, cargo fmt/clippy/test
pnpm tauri build    # release bundle under src-tauri/target/release/bundle/
```

`pnpm verify` must be green before you push. It is what the CI `verify-and-build` (macOS) and
`verify-windows` jobs run, followed by a full `tauri build`.

## Project layout

```
src/                 React popover (rendering only)
  lib/               pure TypeScript: formatting, countdown, quota types — tested in test/
  lib/ipc.ts         the ONLY file that talks to Rust (invoke/listen)
src-tauri/src/
  usage.rs           credentials, HTTP, response normalization
  poll.rs            poll loop, cooldown, backoff, 401 latch
  tray.rs            tray item, menu, title text, popover placement
  icon.rs            Windows tray icon renderer (pure RGBA)
  alerts.rs          threshold alert state machine (pure)
  settings.rs        settings.json in app_data_dir
  log.rs             capped local log
  main.rs            Tauri builder, commands, managed state
docs/                research notes and design specs
```

Rust owns credentials, network, polling, and OS integration. React renders. The frontend never
sees the token and never makes HTTP requests.

## Conventions

- **Formatting/linting**: Biome for TypeScript (`pnpm check:fix`), `cargo fmt` for Rust. Clippy runs
  with `-D warnings`.
- **Tests**: pure logic gets a test — Vitest in `test/` for `src/lib`, inline `#[cfg(test)]` for
  Rust. No snapshot tests, no mocks of the API; feed real JSON shapes.
- **No new dependencies** without a reason you would defend in the PR: no UI libraries, no icon
  packs, no state libraries. Plain CSS, native elements first.
- **Rate limits are sacred**: never add a code path that fetches more often than the cooldown
  allows, never add a "refresh now" that bypasses it, never send the token anywhere but
  `api.anthropic.com`, never log it.
- **Platform code** goes behind `#[cfg(target_os = "...")]`. macOS is the primary target; Windows
  must keep compiling — the Windows CI job is the gate, since most contributors develop on a Mac.
- Comments explain *why*, not what. Keep functions small enough to test.

## Making a change

1. Branch from `main` (`git checkout -b feat/short-name` or `fix/…`, `docs/…`).
2. Write the failing test first when the change is logic, then the code.
3. Run `pnpm verify`.
4. **Add a changeset** for anything a user can notice (behaviour, settings, docs that change what
   the user is told to do): `pnpm changeset`, pick `patch` or `minor`, write one sentence in the
   past tense from the user's point of view. Commit the generated `.changeset/*.md` with your change.
5. Open a pull request against `main`. Describe what changed, why, and how you tested it (which
   OS, which menu items you clicked). CI must be green.

Pull requests are squash-merged, so keep the PR title in conventional-commit form
(`feat: …`, `fix: …`, `docs: …`, `ci: …`); it becomes the commit on `main`.

## Releases

Versions are managed with [Changesets](https://github.com/changesets/changesets):

- Merging a PR with a changeset makes the bot open (or update) a **Version Packages** PR.
- Merging that PR bumps `package.json` and `src-tauri/tauri.conf.json` (kept in sync by
  `scripts/sync-version.mjs`), writes `CHANGELOG.md`, and pushes a `vX.Y.Z` tag.
- The tag runs `build-release.yml`: an Apple Silicon `.dmg` + `.app.tar.gz` and a Windows x64
  NSIS installer are attached to a GitHub Release.

Never edit versions by hand and never create tags by hand. Builds are unsigned (ad-hoc on
macOS); the release notes tell users how to open them.

The release is created as a draft, both platforms upload into it, and a final job publishes it
(GitHub makes published releases immutable). The platform jobs run with `fail-fast: false`, so
if one platform fails the other still uploads — but the failed platform gets no entry in
`latest.json` and its installed apps log a failed update check daily until the next release.
Re-run the failed job from the Actions tab; or, once the workflow itself has changed, rebuild
a tag with `gh workflow run build-release.yml --ref main -f tag=vX.Y.Z` (delete the empty
release first if the publish step already ran).

Release builds also produce updater artifacts (`.sig` files and `latest.json`) signed with a
minisign key stored in the repository secrets `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; the matching public key lives in
`src-tauri/tauri.conf.json` under `plugins.updater.pubkey`. Forks need their own keypair
(`pnpm tauri signer generate`) — and the maintainer's private key must never be lost: installed
apps only accept updates signed with it.

## Reporting a bug

Include the OS and version, the app version (Releases page name or `CHANGELOG.md`), what you
clicked, and the log: **Help → Open log** in the tray menu. The log never contains your token.
