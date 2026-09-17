# CI and Release Pipeline — Design

Date: 2026-09-17. Status: approved.

## Goal

Every pull request and push to `main` is validated and the macOS bundle is built. Releases are
versioned with Changesets and published automatically: merging the "Version Packages" PR pushes a
`vX.Y.Z` tag, and the tag triggers a macOS build that creates a GitHub Release with the `.dmg`
and `.app.tar.gz` attached.

## Decisions

| Topic | Decision |
|---|---|
| Versioning | Changesets, single private package, `privatePackages: { version: true, tag: true }` |
| Version anchors | `package.json` is the source; `scripts/sync-version.mjs` copies it into `src-tauri/tauri.conf.json`; `Cargo.toml` stays `0.0.0` |
| Starting version | Reset both anchors to `0.0.0` and add a `minor` changeset for v1, so the first release is `0.1.0` |
| Release trigger | Merge of the Changesets "Version Packages" PR → `changeset tag` → tag push → build + GitHub Release |
| macOS target | `aarch64-apple-darwin` only, on `macos-latest` |
| CI scope | `pnpm verify` + `pnpm tauri build --bundles app` on every PR and push to `main` |
| Signing | None (unsigned, as in v1) |

## Out of scope

Code signing and notarization, Windows or Intel builds, Tauri updater JSON, changeset-bot PR
comments, branch protection rules.

## Components

### Changesets

- `@changesets/cli` as a devDependency; `pnpm changeset init` output kept
  (`.changeset/config.json`, `.changeset/README.md`).
- `.changeset/config.json`: `baseBranch: "main"`, `commit: false`, `access: "restricted"`,
  `privatePackages: { "version": true, "tag": true }`, `updateInternalDependencies: "patch"`,
  `ignore: []`.
- `package.json` scripts:
  - `changeset`: `changeset`
  - `version-packages`: `changeset version && node scripts/sync-version.mjs`
  - `release:tag`: `changeset tag`
- `scripts/sync-version.mjs`: reads `package.json` `version`, writes it to
  `src-tauri/tauri.conf.json` `version`, preserving the file's 2-space formatting and trailing
  newline. Exits non-zero if either file is unreadable.
- Initial changeset `.changeset/v1-app.md`: `"claude-usage-monitor": minor`, summary describing
  the v1 menu bar app.
- Rule (CLAUDE.md): every user-visible change ships with a changeset in the same PR.

### `.github/workflows/ci.yml`

- Triggers: `pull_request` (all branches), `push` to `main`.
- `concurrency`: group by workflow + ref, `cancel-in-progress: true`.
- One job `verify-and-build` on `macos-latest`:
  1. `actions/checkout@v4`
  2. `pnpm/action-setup@v4` (version from `packageManager`)
  3. `actions/setup-node@v4` with `node-version: 24`, `cache: pnpm`
  4. `dtolnay/rust-toolchain@stable` (honors `src-tauri/rust-toolchain.toml`) with
     `targets: aarch64-apple-darwin`
  5. `Swatinem/rust-cache@v2` with `workspaces: src-tauri`
  6. `pnpm install --frozen-lockfile`
  7. `pnpm verify`
  8. `pnpm tauri build --bundles app --target aarch64-apple-darwin`

### `.github/workflows/release.yml`

- Trigger: `push` to `main`.
- `concurrency`: group `release`, no cancel.
- Permissions: `contents: write`, `pull-requests: write`.
- Job `version-or-tag` on `ubuntu-latest`: checkout (`fetch-depth: 0`), pnpm, Node 24,
  `pnpm install --frozen-lockfile`, then `changesets/action@v1` with:
  - `version: pnpm version-packages`
  - `publish: pnpm release:tag`
  - `createGithubReleases: false`
  - `commit: "chore: version packages"`, `title: "chore: version packages"`
  - `GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}`
- Behavior: with pending changesets the action opens or updates the "Version Packages" PR
  (running `version-packages`, so both anchors bump and `CHANGELOG.md` is written). When that PR
  is merged and no changesets remain, the action runs `changeset tag` and pushes the new tag.

### `.github/workflows/build-release.yml`

- Trigger: `push` of tags matching `v*`. `ponytail:` if `changeset tag` turns out to emit
  `claude-usage-monitor@X.Y.Z` for this repo layout, add that pattern to the trigger and derive
  the release name from the version suffix.
- Permissions: `contents: write`.
- Job `macos` on `macos-latest`: checkout, pnpm, Node 24, Rust stable with
  `aarch64-apple-darwin`, rust-cache, `pnpm install --frozen-lockfile`, then
  `tauri-apps/tauri-action@v0` with:
  - `tagName: ${{ github.ref_name }}`
  - `releaseName: "Claude Usage Monitor ${{ github.ref_name }}"`
  - `releaseBody: "See CHANGELOG.md for details. The app is unsigned: right-click → Open on first launch."`
  - `releaseDraft: false`, `prerelease: false`, `includeUpdaterJson: false`
  - `args: --target aarch64-apple-darwin`
  - `GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}`
- Result: a published GitHub Release for the tag with `Claude Usage Monitor_X.Y.Z_aarch64.dmg`
  and `Claude Usage Monitor.app.tar.gz` attached.

### Docs

- README: "Releases" section — how a release happens, where to download, unsigned note.
- CLAUDE.md: "Versioning and releases" section (changeset rule, anchors, scripts, workflow names)
  and the `Commands` block gains `pnpm changeset`.

## Verification

- Local: `pnpm changeset status` lists the v1 changeset. On a throwaway branch,
  `pnpm version-packages` bumps `package.json` and `tauri.conf.json` to `0.1.0` and writes
  `CHANGELOG.md`; discard the branch.
- `node scripts/sync-version.mjs` is idempotent and leaves the JSON formatting unchanged
  (`git diff --stat` empty after a no-op run).
- Workflow syntax: `gh workflow list` after push; CI must go green on this branch's own PR.
- End-to-end release path is exercised by merging this PR, then merging the first "Version
  Packages" PR and checking that `v0.1.0` gets a Release with two assets.

## Security notes

- Workflows use only `secrets.GITHUB_TOKEN` with the minimum `permissions` blocks above.
- Third-party actions pinned to major tags (`@v4`, `@v2`, `@v1`, `@v0`); no `pull_request_target`.
