---
type: "feat"
status: "complete"
files:
  - src-tauri/src/icon.rs
  - src-tauri/src/tray.rs
  - src-tauri/src/main.rs
  - src-tauri/src/usage.rs
  - src-tauri/src/lib.rs
  - .gitattributes
  - .github/workflows/ci.yml
  - .github/workflows/build-release.yml
  - .changeset/windows-tray.md
  - CLAUDE.md
  - README.md
  - docs/superpowers/specs/2026-09-18-windows-tray-design.md
  - docs/superpowers/plans/2026-09-18-windows-tray.md
areas:
  - tray
  - core
  - ci
  - docs
components:
  - windows-tray-icon
  - popover-placement
  - blur-guard
  - ci-matrix
tags:
  - tauri
  - rust
  - windows
  - macos
  - tray
related-to:
  - docs/superpowers/specs/2026-09-18-windows-tray-design.md
  - docs/superpowers/plans/2026-09-18-windows-tray.md
  - history/2026-09-17-config-and-log.md
---

# feat: Windows system tray

| Field       | Value                                    |
| ----------- | ----------------------------------------- |
| **Status**  | complete                                   |
| **Branch**  | `feat/windows-tray` (squash-merged as PR #10) |
| **Ticket**  | none                                       |
| **Created** | 2026-09-18                                 |
| **Updated** | 2026-09-18                                 |

## Summary

Shipped v1.4: the app runs on Windows. A new pure module `icon.rs` renders a 32×32 raw-RGBA
tray icon (two stacked bars for session/weekly, no text/font dependency), with the full status
string in the hover tooltip; `tray::refresh` (renamed from `refresh_title`) forks once on
`cfg(target_os)` — macOS keeps setting the title text byte-for-byte as before, everything else
sets the tooltip and icon. Popover placement now accounts for a bottom taskbar (opens above the
tray icon instead of below) and is clamped to the containing monitor using its logical global
origin (`Area { x, y, width, height }`), not just its size — a bug that also affected macOS
multi-monitor setups and was caught in final review. A 400 ms blur guard stops the Windows
focus-steal pattern from reopening a popover the same tray click just closed. CI gained a
Windows verify+bundle job producing an unsigned x64 NSIS installer, attached to every release
alongside the macOS bundle.

## Initial Request

Build v1.4 per the approved design spec
(`docs/superpowers/specs/2026-09-18-windows-tray-design.md`) and implementation plan
(`docs/superpowers/plans/2026-09-18-windows-tray.md`): add the pure icon renderer, fork
`tray::refresh` by platform, fix popover placement for a bottom taskbar with a blur guard,
extend CI/release to build and publish a Windows installer, keep macOS behavior unchanged.

## Acceptance Criteria

- [x] `icon.rs` renders a 32×32 RGBA tray icon from `Snapshot` + `Settings` (green/amber/red
      per threshold), pure and unit-tested; OS scales to 16×16 at 100% DPI.
- [x] `tray::refresh` forks on `cfg(target_os)`: macOS sets the title (unchanged), Windows/other
      sets tooltip text (= the macOS title string) + icon.
- [x] Popover opens above the tray icon when it sits in the lower half of the monitor (Windows
      taskbar), below otherwise (macOS menu bar); clamped to the containing monitor by its
      global logical origin, not assumed to start at `(0,0)`.
- [x] `resize_popover` re-places the window using the last known tray rect (`LastTrayRect`)
      after a size change, not just on open.
- [x] A show requested within 400 ms of a blur-hide is ignored (`BLUR_GUARD`), so a tray click
      that would otherwise re-trigger a show right after the blur-hide instead closes the
      popover on Windows.
- [x] `usage::user_agent` resolves the `claude` `.cmd` shim through `cmd /C` on Windows so the
      version probe succeeds.
- [x] CI: a Windows job verifies and bundles an NSIS x64 installer; `build-release.yml` attaches
      it to releases alongside the macOS `.dmg`/`.app.tar.gz`.
- [x] `.gitattributes` forces LF line endings repo-wide so a Windows checkout does not fail the
      Biome format check.
- [x] macOS title text and popover placement stay byte-for-byte unchanged.

## Plan

Full step-by-step plan lives at `docs/superpowers/plans/2026-09-18-windows-tray.md` (icon
renderer, `.cmd` shim fix, tray refresh fork, popover-above-taskbar + blur guard, CI/release
matrix, docs).

## Execution Log

Branch `feat/windows-tray`, squash-merged to `main` as PR #10 (`f678f61`).

- 2026-09-18 (`5d75e6d`): docs — added the Windows system tray design spec.
- 2026-09-18 (`7b45a2e`): docs — added the Windows system tray implementation plan.
- 2026-09-18 (`a779bde`): feat — pure RGBA tray icon renderer for Windows (`icon.rs`).
- 2026-09-18 (`a2250c7`): fix — resolved the `claude` `.cmd` shim through `cmd /C` on Windows.
- 2026-09-18 (`7679e60`): feat — draw the tray icon and tooltip on non-macOS platforms.
- 2026-09-18 (`8b15b5f`): feat — place the popover above a bottom taskbar and guard against
  blur re-open.
- 2026-09-18 (`e251099`): ci — verify, bundle and release the Windows x64 installer.
- 2026-09-18 (`b2ce4e1`): fix — forced LF line endings so the Biome format check passes on a
  Windows checkout (see Final Notes).
- 2026-09-18 (`3411d25`): docs — Windows tray, installer and settings paths.
- 2026-09-18 (`0dffb6e`): fix — place the popover on the monitor under the tray icon and
  re-place after resize (final-review catch — see Final Notes).
- 2026-09-18 (`0ff29d0`): docs — described popover monitor selection, resize re-placement, and
  the 400 ms blur guard.
- 2026-09-18 (`c0c6bd0`): docs — corrected the blur guard test description to 600 ms.
- 2026-09-18 (`f678f61`): squash-merged as PR #10.

## Files Changed

- `src-tauri/src/icon.rs` (new) — pure 32×32 RGBA tray icon renderer, threshold colours, inline
  tests.
- `src-tauri/src/tray.rs` — `refresh` forked by `cfg(target_os)`; `popover_origin` takes a
  containing `Area` (global logical origin, not just size) and places above/below by taskbar
  position; `LastTrayRect` managed state drives `resize_popover` re-placement; `BLUR_GUARD =
  400 ms` guard against Windows focus-steal reopen.
- `src-tauri/src/main.rs` — wires `LastTrayRect` into managed state; platform-specific tray
  setup calls.
- `src-tauri/src/usage.rs` — `user_agent` probe spawns `cmd /C claude --version` on Windows to
  resolve the `.cmd` shim.
- `src-tauri/src/lib.rs` — module wiring for `icon`.
- `.gitattributes` (new) — `* text=auto eol=lf` so Windows checkouts don't introduce CRLF that
  breaks Biome.
- `.github/workflows/ci.yml` — Windows verify + bundle job.
- `.github/workflows/build-release.yml` — Windows NSIS installer build attached to releases.
- `.changeset/windows-tray.md` — minor changeset.
- `CLAUDE.md` — status line updated; OS-specific code convention reaffirmed
  (`#[cfg(target_os)]`, macOS first, keep Windows compiling).
- `README.md` — Windows install/usage notes, unsigned-installer SmartScreen note.
- `docs/superpowers/specs/2026-09-18-windows-tray-design.md`,
  `docs/superpowers/plans/2026-09-18-windows-tray.md` — design and plan.

## Testing

- `pnpm verify` green on macOS throughout (biome/tsc/vitest untouched by this cycle; cargo
  fmt/clippy/test green), plus `cargo check --target x86_64-pc-windows-msvc` locally where
  possible; full Windows compilation proven by CI.
- Inline Rust tests added/exercised: `icon::` (renderer output per threshold), `tray::tests::
  display_menu_label`, `tray::tests::popover` (monitor containment/placement), `tray::tests::
  blur` (guard timing).
- CI: both `verify-and-build` (macOS) and the new Windows job green; `windows-installer`
  artifact present. First Windows CI build took ~24 minutes cold (uncached toolchain/crates);
  expected to drop sharply once the Windows runner's cache warms on subsequent runs.
- Deferred to a human on real Windows hardware: icon visible on dark/light taskbars; tooltip
  counts down; left click opens the popover above the icon; click elsewhere / click icon again
  closes it; right-click menu shows "Tray ▸"; test notification shows a toast; Help → Open log
  selects the file in Explorer; Quit removes the icon.
- Deferred to a human on macOS: menu bar title and popover placement confirmed unchanged.

## Final Notes

- `.gitattributes` (`* text=auto eol=lf`) was needed because a Windows `git checkout` converts
  LF to CRLF by default; Biome's format check then flags every file as unformatted purely from
  line endings, failing the Windows CI job on files nobody touched.
- Final review (before merge) found the popover clamp only accounted for the containing
  monitor's *size*, not its *global origin* — on a multi-monitor macOS setup where the tray's
  monitor isn't at `(0,0)`, the popover could clamp to the wrong region entirely. This is a
  regression risk that predates Windows and was only surfaced because Windows forced
  `popover_origin` to take a real `Area` argument. Fixed by selecting the monitor the tray rect
  is contained in and clamping against that monitor's own `Area { x, y, width, height }`
  (`0dffb6e`). `resize_popover` now re-places the window against `LastTrayRect` instead of only
  placing once at open time.
- The 400 ms blur guard (`BLUR_GUARD`) exists because Windows tray clicks fire a focus-loss
  (blur-hide) event immediately before the click's own show request arrives; without the guard
  the popover would hide then immediately reopen. One doc pass mistakenly wrote the guard's
  test description as 600 ms; corrected to 400 ms to match the constant.
- Launch at login, ARM64 Windows, code signing, Linux, theme-aware icon colours, per-model bars
  in the icon, and a portable `.exe` are explicitly out of scope for this cycle.
