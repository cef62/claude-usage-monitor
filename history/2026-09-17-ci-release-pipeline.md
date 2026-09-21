---
type: "chore"
status: "complete"
files:
  - .github/workflows/ci.yml
  - .github/workflows/release.yml
  - .github/workflows/build-release.yml
  - scripts/sync-version.mjs
  - test/sync-version.test.ts
  - package.json
  - src-tauri/tauri.conf.json
  - tsconfig.json
  - .changeset/README.md
  - .changeset/config.json
  - .changeset/v1-app.md
  - CLAUDE.md
  - README.md
  - docs/superpowers/specs/2026-09-17-ci-release-design.md
  - docs/superpowers/plans/2026-09-17-ci-release.md
areas:
  - ci
  - release
  - docs
components:
  - changesets
  - version-sync
  - github-actions
tags:
  - tauri
  - ci
  - release
  - changesets
related-to:
  - docs/superpowers/specs/2026-09-17-ci-release-design.md
  - docs/superpowers/plans/2026-09-17-ci-release.md
  - history/2026-09-16-v1-app.md
---

# ci: changesets, CI and macOS release pipeline

| Field       | Value                                    |
| ----------- | ----------------------------------------- |
| **Status**  | complete                                   |
| **Branch**  | `chore/ci-release` (squash-merged as PR #2)|
| **Ticket**  | none                                       |
| **Created** | 2026-09-17                                 |
| **Updated** | 2026-09-17                                 |

## Summary

Wired the v1 app into a full CI/release pipeline: Changesets for versioning a single private
package, `scripts/sync-version.mjs` to mirror `package.json`'s version into
`src-tauri/tauri.conf.json` (`Cargo.toml` stays inert at `0.0.0`), a `ci.yml` that runs
`pnpm verify` plus an app-only Tauri build on every PR and push to `main`, and a two-stage
release: `release.yml` (Changesets action opens/updates the "Version Packages" PR; merging it
tags `vX.Y.Z`) dispatching `build-release.yml` (builds `aarch64-apple-darwin`, publishes the
GitHub Release with `.dmg` and `.app.tar.gz`). Both version anchors were reset to `0.0.0` with a
`minor` changeset for v1 so the first tagged release comes out `0.1.0`.

## Initial Request

Build the CI/release pipeline per the approved design spec
(`docs/superpowers/specs/2026-09-17-ci-release-design.md`) and implementation plan
(`docs/superpowers/plans/2026-09-17-ci-release.md`): adopt Changesets, add the version-sync
script, add `ci.yml`, add `release.yml` + `build-release.yml`, update docs, then watch CI to a
green run.

## Acceptance Criteria

- [x] Changesets adopted (`privatePackages: { version: true, tag: true }`), single private
      package, `.changeset/config.json` and `.changeset/README.md` scaffolded.
- [x] `scripts/sync-version.mjs` copies `package.json`'s version into
      `src-tauri/tauri.conf.json`; idempotent (`git diff --stat` empty on a no-op run); fails
      loudly if `tauri.conf.json` has no `version` field.
- [x] `ci.yml` runs `pnpm verify` and `pnpm tauri build --bundles app --target
      aarch64-apple-darwin` on every PR and push to `main`.
- [x] `release.yml` uses the Changesets action to keep the "Version Packages" PR current; on
      merge it runs `changeset git-tag`, pushes the tag, and dispatches `build-release.yml`.
- [x] `build-release.yml` accepts `workflow_dispatch` (for the release.yml-driven path) and a
      manual `v*` tag push; `tauri-action` builds Apple Silicon only and publishes the GitHub
      Release with `.dmg` and `.app.tar.gz`.
- [x] Both version anchors reset to `0.0.0`; a `minor` changeset for v1 exists so the first
      release is `0.1.0`.
- [x] Workflows use only `secrets.GITHUB_TOKEN` with minimum `permissions` blocks; third-party
      actions pinned to major tags; no `pull_request_target`.
- [x] README gains a "Releases" section; `CLAUDE.md` gains "Versioning and Releases" and
      `pnpm changeset` in Commands.
- [x] CI green on this PR's own branch.

## Plan

Full step-by-step plan lives at `docs/superpowers/plans/2026-09-17-ci-release.md` (4 tasks:
Changesets setup + version-sync script + anchor reset; CI workflow; release workflows; docs,
push, PR, watch CI).

## Execution Log

Branch `chore/ci-release`, squash-merged to `main` as PR #2 (`5a40503`).

- 2026-09-17 (`f195684`): docs — added the CI and release pipeline design spec.
- 2026-09-17 (`f7fd1ef`): docs — added the CI and release pipeline implementation plan.
- 2026-09-17 (`21fe7a8`): chore — adopted Changesets with `tauri.conf.json` version sync —
  `.changeset/config.json`, `sync-version.mjs`, anchors reset to `0.0.0`, v1 changeset added.
- 2026-09-17 (`63fc536`): fix — `sync-version.mjs` now fails the run when `tauri.conf.json`
  has no `version` field instead of silently no-op'ing.
- 2026-09-17 (`b77b772`): ci — added `ci.yml` — verify and build the macOS app on pull
  requests and `main`.
- 2026-09-17 (`4d9cd7b`): ci — added `release.yml` and `build-release.yml` — version with
  Changesets, publish macOS releases on tags.
- 2026-09-17 (`2130bca`): ci — installed `rustfmt` and `clippy` components explicitly for the
  verify step.
- 2026-09-17 (`50472a1`): docs — described the changeset-driven versioning and release
  pipeline in `CLAUDE.md`/README.
- 2026-09-17 (`22bfaec`): ci — dispatch the release build from the Changesets action, after
  discovering `changesets/action@v1` never detects a published tag — see Final Notes.
- 2026-09-17 (`a60681c`): docs — updated the versioning docs and design spec to match
  (`git-tag` not `tag`, `changesets/action@v2` inputs, dispatch-driven `build-release.yml`).
- 2026-09-17 (`f40ca1b`): docs — aligned the spec's decision table with the tag-dispatch flow.
- 2026-09-17 (`5a40503`): squash-merged as PR #2 after watching CI to green
  (`gh pr checks --watch`).

## Files Changed

- `.github/workflows/ci.yml` — `pnpm verify` + `pnpm tauri build --bundles app --target
  aarch64-apple-darwin` on PRs and `main`.
- `.github/workflows/release.yml` — Changesets action (v2) opens/updates the Version Packages
  PR; on merge runs `changeset git-tag`, pushes the tag, dispatches `build-release.yml` via
  `gh workflow run`.
- `.github/workflows/build-release.yml` — gains `workflow_dispatch`; on a `v*` tag,
  `tauri-action` builds `aarch64-apple-darwin` and publishes the GitHub Release.
- `scripts/sync-version.mjs` — copies `package.json`'s version into
  `src-tauri/tauri.conf.json`; fails if the target has no `version` field.
- `test/sync-version.test.ts` — Vitest coverage for the sync script (idempotency, failure on
  missing field).
- `package.json` — Changesets devDependency, `pnpm version-packages` script.
- `src-tauri/tauri.conf.json` — version anchor reset to `0.0.0`.
- `.changeset/README.md`, `.changeset/config.json` — Changesets scaffold
  (`privatePackages: { version: true, tag: true }`).
- `.changeset/v1-app.md` — `minor` changeset so the first tagged release is `0.1.0`.
- `tsconfig.json` — include the new `scripts/` and `test/` files for `tsc --noEmit`.
- `CLAUDE.md` — "Versioning and Releases" section; `pnpm changeset` added to Commands.
- `README.md` — "Releases" section: how a release happens, where to download, unsigned note.
- `docs/superpowers/specs/2026-09-17-ci-release-design.md`,
  `docs/superpowers/plans/2026-09-17-ci-release.md` — design and plan.

## Testing

- `pnpm verify`: 10 Vitest, 22 cargo tests, all green.
- `pnpm changeset status` listed the v1 changeset; a throwaway-branch dry run of `pnpm
  version-packages` bumped both anchors to `0.1.0` and wrote `CHANGELOG.md` correctly, then was
  discarded.
- `node scripts/sync-version.mjs` confirmed idempotent (`git diff --stat` empty on a repeat
  run).
- CI (`verify-and-build`) went green on this PR's own branch via `gh pr checks --watch`.
- End-to-end release path (Version Packages PR → tag → Release assets) could only be exercised
  after merge — left as the one manual follow-up, and it is what surfaced the tag-dispatch bug
  fixed in this same cycle (see Final Notes).

## Final Notes

- `changesets/action@v1` greps its own stdout for `New tag:`, but `@changesets/cli` 3 prints
  `Created git tags:` and emits ndjson only the v2 action parses — so the v1 action never
  detected a published tag, and only pushes tags at all when `createGithubReleases` is true
  (deliberately `false` here). Separately, a tag pushed with `GITHUB_TOKEN` does not trigger
  other workflows' `on: push: tags` (GitHub suppresses recursive workflow triggers from the
  default token). Fix: `changesets/action@v2` (defaults `github-token` to `github.token`), the
  changeset alias renamed `tag` → `git-tag` (`tag` is deprecated in cli 3), and an explicit
  `gh workflow run build-release.yml` dispatch step for the newly created tag;
  `build-release.yml` gained `workflow_dispatch` to be startable that way.
- Signing and notarization, Windows/Intel builds, updater JSON, and changeset-bot PR comments
  are explicitly out of scope for this cycle — deferred to later versions.
