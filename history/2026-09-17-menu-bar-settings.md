---
type: "feat"
status: "complete"
files:
  - src-tauri/src/settings.rs
  - src-tauri/src/tray.rs
  - src-tauri/src/main.rs
  - src-tauri/src/lib.rs
  - src-tauri/tauri.conf.json
  - .github/workflows/build-release.yml
  - .changeset/menu-bar-settings.md
  - CLAUDE.md
  - README.md
  - docs/superpowers/specs/2026-09-17-menu-bar-settings-design.md
  - docs/superpowers/plans/2026-09-17-menu-bar-settings.md
areas:
  - tray
  - core
  - docs
components:
  - menu-bar-settings
  - tray-title
  - ad-hoc-signing
tags:
  - tauri
  - rust
  - macos
  - settings
related-to:
  - docs/superpowers/specs/2026-09-17-menu-bar-settings-design.md
  - docs/superpowers/plans/2026-09-17-menu-bar-settings.md
  - history/2026-09-16-v1-app.md
  - history/2026-09-17-ci-release-pipeline.md
---

# feat: menu bar settings, monochrome glyphs, ad-hoc signing

| Field       | Value                                    |
| ----------- | ----------------------------------------- |
| **Status**  | complete                                   |
| **Branch**  | `feat/menu-bar-settings` (squash-merged as PR #4) |
| **Ticket**  | none                                       |
| **Created** | 2026-09-17                                 |
| **Updated** | 2026-09-17                                 |

## Summary

Shipped v1.1: a configurable menu bar. A new tray right-click submenu "Menu bar" with five
persisted check items (Session, Weekly, Glyphs, Percent, Remaining time) lets the user choose
what the title shows, backed by a Rust-owned `Settings` struct serialized to
`app_data_dir/settings.json` behind a `Mutex` in Tauri managed state. The colour 📅 emoji was
replaced with monochrome glyphs (session `◷` U+25F7, weekly `▦` U+25A6) and the title spacing was
widened so it reads cleanly next to the template icon. The macOS bundle is now ad-hoc signed
(`bundle.macOS.signingIdentity: "-"`) so downloaded builds open via "Open Anyway" instead of
Gatekeeper reporting them "damaged".

## Initial Request

Build v1.1 per the approved design spec
(`docs/superpowers/specs/2026-09-17-menu-bar-settings-design.md`) and implementation plan
(`docs/superpowers/plans/2026-09-17-menu-bar-settings.md`): add the persisted settings model,
render the title from settings with monochrome glyphs, wire the tray submenu, ad-hoc sign the
bundle, then update docs and ship.

## Acceptance Criteria

- [x] `Settings` (five booleans: show_session, show_weekly, show_glyphs, show_percent,
      show_remaining) persisted as JSON at `app_data_dir/settings.json`, held in
      `Mutex<Settings>` managed state.
- [x] Invariant enforced: at least one of session/weekly and one of percent/remaining stays
      enabled — the menu handler refuses a toggle that would empty either pair.
- [x] Tray right-click menu gains a "Menu bar" submenu of five `CheckMenuItem`s that toggle,
      re-sync check marks, persist, and refresh the title live.
- [x] Title renders only the enabled parts, monochrome glyphs (no emoji), leading space, single
      spaces inside a half, `"  ·  "` between halves.
- [x] `tauri.conf.json` sets `bundle.macOS.signingIdentity: "-"` (ad-hoc); no notarization.
- [x] README and CLAUDE.md describe the menu bar settings and the ad-hoc signing status.
- [x] `pnpm verify` passes (12 vitest, 32 cargo).

## Plan

Full step-by-step plan lives at `docs/superpowers/plans/2026-09-17-menu-bar-settings.md` (docs,
`settings.rs`, title rendering, submenu + signing, docs).

## Execution Log

Branch `feat/menu-bar-settings`, squash-merged to `main` as PR #4 (`2ba62fa`).

- 2026-09-17 (`4a5e068`): docs — added the menu bar settings and title polish design.
- 2026-09-17 (`c565ab1`): docs — added the menu bar settings implementation plan.
- 2026-09-17 (`722a7b3`): feat — added the persisted menu bar `Settings` model
  (`src-tauri/src/settings.rs`), JSON in `app_data_dir`.
- 2026-09-17 (`3be3499`): feat — render the menu bar title from settings with monochrome
  glyphs, replacing the colour emoji and widening spacing.
- 2026-09-17 (`b9b6848`): feat — menu bar settings submenu (five `CheckMenuItem`s) with
  persistence, and the ad-hoc signed bundle (`signingIdentity: "-"`).
- 2026-09-17 (`9824a7b`): docs — described menu bar settings and ad-hoc signing in
  CLAUDE.md/README.
- 2026-09-17 (`caaf025`): docs — aligned status glyphs and release notes with ad-hoc signing.
- 2026-09-17 (`2ba62fa`): squash-merged as PR #4.

## Files Changed

- `src-tauri/src/settings.rs` (new) — `Settings` struct, JSON load/save in `app_data_dir`,
  toggle-with-invariant logic.
- `src-tauri/src/tray.rs` — title rendering reads `Settings` and emits only enabled parts with
  monochrome glyphs; "Menu bar" submenu of five check items wired to the toggle handler.
- `src-tauri/src/main.rs` — wires `Settings` into managed state at startup.
- `src-tauri/src/lib.rs` — module wiring for `settings`.
- `src-tauri/tauri.conf.json` — `bundle.macOS.signingIdentity: "-"`.
- `.github/workflows/build-release.yml` — no functional change beyond what the ad-hoc-signed
  bundle needs from the existing `tauri-action` build.
- `.changeset/menu-bar-settings.md` — minor changeset.
- `CLAUDE.md` — Layout gains `src/settings.rs`; Tauri Rules note ad-hoc signing instead of
  "unsigned"; status line updated to v1.1.
- `README.md` — menu bar configuration section, "Open Anyway" note for the ad-hoc signed
  build.
- `docs/superpowers/specs/2026-09-17-menu-bar-settings-design.md`,
  `docs/superpowers/plans/2026-09-17-menu-bar-settings.md` — design and plan.

## Testing

- `pnpm verify`: 12 Vitest, 32 cargo tests, all green (fmt/clippy clean).
- Settings toggle/persist verified interactively in `tauri dev` (checks survive a relaunch).
- Deferred to the next release asset: `codesign -dv` shows a full ad-hoc signature, and the
  build opens on another Mac after "Open Anyway" — not verifiable until a tagged build exists.

## Final Notes

- The prior v1 release shipped fully unsigned; downloaded builds on a second Mac reported
  "damaged" because an unsigned/linker-signed `.app` fails Gatekeeper's quarantine check
  outright (no "Open Anyway" escape hatch without at least an ad-hoc signature). Fix:
  `bundle.macOS.signingIdentity: "-"` so `tauri-action`/`codesign` ad-hoc-signs the bundle —
  Gatekeeper then reports an unverified developer (survivable via "Open Anyway") instead of
  "damaged". Full notarization is still blocked on getting an Apple Developer account.
- No popover changes in this cycle — the settings surface is the tray right-click menu only.
