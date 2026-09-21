---
type: "feat"
status: "complete"
files:
  - src-tauri/src/update.rs
  - src-tauri/src/tray.rs
  - src-tauri/src/main.rs
  - src-tauri/src/lib.rs
  - src-tauri/Cargo.toml
  - src-tauri/Cargo.lock
  - src-tauri/tauri.conf.json
  - .github/workflows/build-release.yml
  - .github/workflows/ci.yml
  - .changeset/auto-update.md
  - CLAUDE.md
  - CONTRIBUTING.md
  - README.md
  - docs/superpowers/specs/2026-09-19-auto-update-design.md
  - docs/superpowers/plans/2026-09-19-auto-update.md
areas:
  - core
  - tray
  - ci
  - docs
components:
  - updater
  - update-state-machine
  - release-signing
tags:
  - tauri
  - rust
  - macos
  - windows
  - updater
  - minisign
related-to:
  - docs/superpowers/specs/2026-09-19-auto-update-design.md
  - docs/superpowers/plans/2026-09-19-auto-update.md
  - history/2026-09-17-ci-release-pipeline.md
  - history/2026-09-18-launch-at-login-and-reset-notification.md
---

# feat: auto-update

| Field       | Value                                    |
| ----------- | ----------------------------------------- |
| **Status**  | complete                                   |
| **Branch**  | `feat/auto-update` (squash-merged as PR #20) |
| **Ticket**  | none                                       |
| **Created** | 2026-09-19                                 |
| **Updated** | 2026-09-21                                 |

## Summary

Shipped v1.7: in-app updates. `tauri-plugin-updater` (Rust side only, no npm package, no
capability) checks `https://github.com/cef62/claude-usage-monitor/releases/latest/download/
latest.json` — produced by `tauri-action`'s `includeUpdaterJson: true` and merged across the
macOS/Windows release matrix — 30 seconds after startup and then every 24 hours. A new
`update.rs` owns `UpdateState`, the background checker thread, the manual check, and the
install; it notifies once per available version and exposes **Help → Check for updates…** and
**Help → Install update x.y.z…** in the tray menu. Update artifacts are minisign-signed;
installed builds only accept packages signed by the public key committed in
`tauri.conf.json`, verified against the private key + password held in repo secrets. Debug
builds never auto-check (`cfg!(debug_assertions)`); the manual item still works in debug so it
can be exercised against the real feed, and answers `Updates are disabled in debug builds` in
`tauri dev`. Since the repo is private until flipped public, 404s from the feed are expected for
a while and are swallowed silently in the background (logged once) while surfaced by name on
manual checks.

## Initial Request

Build v1.7 per the approved design spec
(`docs/superpowers/specs/2026-09-19-auto-update-design.md`) and implementation plan
(`docs/superpowers/plans/2026-09-19-auto-update.md`): wire the updater plugin with a committed
public key, add the Help menu items, implement the daily background check with one notification
per version and one-click install, sign CI's updater artifacts, then dock in docs.

## Acceptance Criteria

- [x] `tauri-plugin-updater` 2.x wired Rust-side only; reads `latest.json` from the GitHub
      Releases feed; verifies minisign signatures against the public key in `tauri.conf.json`.
- [x] Background check 30s after startup, then every 24h; notifies once per newly-seen version;
      never surfaces a background failure beyond a single log line.
- [x] **Help → Check for updates…** checks on demand and reports the result (up to date /
      available / error) via notification.
- [x] **Help → Install update x.y.z…** appears only when an update is available; downloads,
      installs, and restarts the app (macOS: swaps the `.app` bundle then `app.restart()`;
      Windows: NSIS `installMode: "passive"`, installer relaunches the app).
- [x] Debug builds (`cfg!(debug_assertions)`) never run the background check; the manual item
      still hits the real feed and reports the disabled state in `tauri dev` specifically.
- [x] Network requests carry an idle read timeout so a stalled socket can never pin the checker
      "busy" state until a restart.
- [x] CI signs updater artifacts with repo secrets (`TAURI_SIGNING_PRIVATE_KEY`,
      `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`); the private key itself is stored only in the
      repo's encrypted secrets and the controller's local keychain, never in the repo.
- [x] `pnpm verify` green; `pnpm tauri dev` logs no update check (debug guard confirmed).

## Plan

Full step-by-step plan lives at `docs/superpowers/plans/2026-09-19-auto-update.md` (updater
plugin + public key + state skeleton, Help menu items, daily check + notification + install,
docs/signing/changeset, timeout + debug guard fix, CI secrets).

## Execution Log

Branch `feat/auto-update`, squash-merged to `main` as PR #20 (`0ce186e`).

- 2026-09-19 (`09b09af`): docs — added the auto-update design spec.
- 2026-09-19 (`6d75f34`): docs — added the auto-update implementation plan.
- 2026-09-19 (`5b04c98`): feat — updater plugin, public key and `UpdateState` skeleton.
- 2026-09-19 (`13e4909`): feat — Help menu items for checking and installing updates.
- 2026-09-19 (`532eb10`): feat — daily update check, notification, and one-click install.
- 2026-09-19 (`9552e3d`): docs — updater documentation, release signing, and changeset.
- 2026-09-19 (`ee5c5bf`): fix — read timeout for update requests, and no updates in debug
  builds (closed a gap where the manual check path lacked both).
- 2026-09-19 (`7e2055d`): docs — updated spec notes for the timeout, debug guard, and download
  hosts.
- 2026-09-19 (`a3b61d8`): ci — gave the CI build steps the updater signing key
  (`createUpdaterArtifacts: true` makes every `tauri build` require the private key).
- 2026-09-19 (`f3f6580`): fix — regenerated the updater signing key with its password (see
  Final Notes).
- 2026-09-21 (`0ce186e`): squash-merged as PR #20.

## Files Changed

- `src-tauri/src/update.rs` (new) — `UpdateState`, background checker thread (30s startup
  delay, 24h interval, `READ_TIMEOUT = 60s` idle read timeout), manual check, install flow,
  debug-build guard.
- `src-tauri/src/tray.rs` — Help submenu gains "Check for updates…" and a conditional "Install
  update x.y.z…" item.
- `src-tauri/src/main.rs` — registers `tauri-plugin-updater`, wires `UpdateState` into managed
  state, starts the background checker.
- `src-tauri/src/lib.rs` — module wiring for `update`.
- `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` — `tauri-plugin-updater` dependency.
- `src-tauri/tauri.conf.json` — `plugins.updater` config: feed URL, committed minisign public
  key.
- `.github/workflows/build-release.yml` — `createUpdaterArtifacts: true`; build steps receive
  `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secrets; `latest.json`
  merged and published across the macOS/Windows matrix.
- `.github/workflows/ci.yml` — no functional change beyond keeping the plugin config
  compiling on PR builds.
- `.changeset/auto-update.md` — minor changeset.
- `CLAUDE.md` — status line/stack notes updated for the updater plugin.
- `CONTRIBUTING.md` — note on the updater signing keypair being repo-secret-only.
- `README.md` — "Updates" section: check cadence, one notification per version, one-click
  install, signature verification, "builds older than 0.8.0 have no updater" note.
- `docs/superpowers/specs/2026-09-19-auto-update-design.md`,
  `docs/superpowers/plans/2026-09-19-auto-update.md` — design and plan.

## Testing

- `cargo test --manifest-path src-tauri/Cargo.toml update::` — 3 passed (state transitions,
  debug guard, timeout wiring) after each relevant task; green throughout.
- `cargo test --manifest-path src-tauri/Cargo.toml menu_ids` — green after the Help menu item
  changes.
- `pnpm verify` green after every commit; confirmed `pnpm tauri dev` logs show no update check
  attempt (debug guard holds).
- Deferred to a human: first release with the updater carries `latest.json` + `.sig` assets;
  install that build on macOS and Windows, confirm Help → Check for updates… reports "Up to
  date", ship the next patch, confirm the notification + Help → Install update…, confirm the
  app relaunches on the new version and the log records `update installed …, restarting`.
- Noted as expected until the repo goes public: the feed URL 404s, surfacing as `Update check
  failed` on manual checks and staying silent in the background.

## Final Notes

- The minisign keypair was first generated with `tauri signer generate --ci` and no `-p` flag,
  which writes an **unencrypted** private key. CI then failed every signed build with "Wrong
  password for that key" once the password secret was set (the key had none to match). Fixed
  by regenerating the keypair with `-p` (password-protected), committing the new public key,
  and re-setting both `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repo
  secrets. The keypair itself lives only in repo secrets and the controller's local machine —
  never in the repository or in this record.
- `createUpdaterArtifacts: true` means every `tauri build` — not just tagged releases — needs
  the private key to produce a valid bundle, so the CI *build* steps (not only the *release*
  step) needed the signing secrets added to their environment.
- Final review added two things beyond the original plan: a 60-second idle read timeout on
  update HTTP requests (`READ_TIMEOUT`), so a stalled connection can't leave the checker
  permanently "busy" until the app restarts; and a debug-build guard specifically on the
  *manual* check path (the background checker was already guarded), so `tauri dev` reports
  "Updates are disabled in debug builds" instead of hitting the real feed from a developer's
  local run.
- Opt-out toggle, release notes in the notification, delta/differential updates, Windows
  ARM64, Linux, and updating from builds older than the first updater-carrying release are
  explicitly out of scope.
