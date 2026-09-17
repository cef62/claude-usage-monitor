# CI and Release Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Validate and build the macOS app on every PR, and publish a GitHub Release with the `.dmg` and `.app.tar.gz` whenever a Changesets "Version Packages" PR is merged.

**Architecture:** Changesets owns the version in `package.json`; a tiny script mirrors it into `src-tauri/tauri.conf.json`. Three GitHub Actions workflows: `ci.yml` (verify + build on PRs and `main`), `release.yml` (Changesets action on `main`: opens the Version Packages PR, and on its merge tags `vX.Y.Z`), `build-release.yml` (on `v*` tags: `tauri-action` builds `aarch64-apple-darwin` and publishes the Release with assets).

**Tech Stack:** `@changesets/cli` 3.0.3, `changesets/action@v1`, `tauri-apps/tauri-action@v0`, `pnpm/action-setup@v4`, `actions/setup-node@v4`, `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`, `macos-latest`, `ubuntu-latest`.

**Spec:** `docs/superpowers/specs/2026-09-17-ci-release-design.md`

## Global Constraints

- Branch `chore/ci-release` (already created off `main` at `680ca0d`); never commit to `main`.
- Version anchors: `package.json` is the source; `src-tauri/tauri.conf.json` `version` must always equal it; `src-tauri/Cargo.toml` stays `0.0.0`. Both anchors reset to `0.0.0` in this branch; the first release will be `0.1.0` via the v1 changeset.
- Only `secrets.GITHUB_TOKEN`; explicit minimal `permissions` blocks; actions pinned to major tags (`@v4`, `@v2`, `@v1`, `@v0`); no `pull_request_target`.
- macOS target `aarch64-apple-darwin` only; runner `macos-latest`; Node 24; pnpm from `packageManager`.
- No new runtime dependencies. Biome-clean (`pnpm check`), `pnpm verify` green.
- Commit messages: conventional prefix, normal English, trailer exactly `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>`. Use `git -c commit.gpgsign=false commit` if signing prompts.
- Every user-visible change ships with a changeset (this plan adds the first one).

---

### Task 1: Changesets setup, version sync script, anchors reset

**Files:**
- Create: `.changeset/config.json`, `.changeset/README.md` (from `changeset init`), `.changeset/v1-app.md`, `scripts/sync-version.mjs`, `test/sync-version.test.ts`
- Modify: `package.json` (devDependency, scripts, version), `src-tauri/tauri.conf.json` (version), `pnpm-lock.yaml`, possibly `pnpm-workspace.yaml`/`.npmrc` (see Step 3)

**Interfaces:**
- Produces: `pnpm changeset`, `pnpm version-packages`, `pnpm release:tag` scripts; `scripts/sync-version.mjs` exporting `syncVersion(pkgPath, confPath): string` and running it when invoked as a script.

- [ ] **Step 1: Install Changesets and init**

Run: `pnpm add -D @changesets/cli@3.0.3 && pnpm changeset init`
Expected: `.changeset/config.json` and `.changeset/README.md` created; `package.json` devDependencies gains `"@changesets/cli": "3.0.3"`.

- [ ] **Step 2: Write `.changeset/config.json`**

```json
{
  "$schema": "https://unpkg.com/@changesets/config@3.1.1/schema.json",
  "changelog": "@changesets/cli/changelog",
  "commit": false,
  "fixed": [],
  "linked": [],
  "access": "restricted",
  "baseBranch": "main",
  "updateInternalDependencies": "patch",
  "ignore": [],
  "privatePackages": {
    "version": true,
    "tag": true
  }
}
```

If `changeset init` wrote a different `$schema` URL, keep the one it wrote.

- [ ] **Step 3: Confirm Changesets sees the root package**

Run: `pnpm changeset status`
Expected: prints that no changesets are present (exit 0). If it errors because `pnpm-workspace.yaml` exists without a `packages` field (message mentions "workspace" or "no packages found"), move the pnpm setting to `.npmrc` and delete the workspace file:

`.npmrc`:
```
minimum-release-age-exclude[]=@biomejs/biome@2.5.14
minimum-release-age-exclude[]=@biomejs/cli-darwin-arm64@2.5.14
minimum-release-age-exclude[]=@biomejs/cli-darwin-x64@2.5.14
minimum-release-age-exclude[]=@biomejs/cli-linux-arm64-musl@2.5.14
minimum-release-age-exclude[]=@biomejs/cli-linux-arm64@2.5.14
minimum-release-age-exclude[]=@biomejs/cli-linux-x64-musl@2.5.14
minimum-release-age-exclude[]=@biomejs/cli-linux-x64@2.5.14
minimum-release-age-exclude[]=@biomejs/cli-win32-arm64@2.5.14
minimum-release-age-exclude[]=@biomejs/cli-win32-x64@2.5.14
```

Then `git rm pnpm-workspace.yaml && pnpm install --frozen-lockfile && pnpm changeset status` — must exit 0. Record which path you took in the report.

- [ ] **Step 4: Write the failing test for the sync script**

`test/sync-version.test.ts`:

```ts
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { syncVersion } from '../scripts/sync-version.mjs';

function fixture(pkgVersion: string, confVersion: string) {
  const dir = mkdtempSync(join(tmpdir(), 'sync-version-'));
  const pkg = join(dir, 'package.json');
  const conf = join(dir, 'tauri.conf.json');
  writeFileSync(pkg, `${JSON.stringify({ name: 'x', version: pkgVersion }, null, 2)}\n`);
  writeFileSync(
    conf,
    `${JSON.stringify({ productName: 'X', version: confVersion, build: {} }, null, 2)}\n`,
  );
  return { pkg, conf };
}

describe('syncVersion', () => {
  it('copies package.json version into tauri.conf.json', () => {
    const { pkg, conf } = fixture('1.2.3', '0.0.0');
    expect(syncVersion(pkg, conf)).toBe('1.2.3');
    expect(JSON.parse(readFileSync(conf, 'utf8')).version).toBe('1.2.3');
  });

  it('preserves formatting and trailing newline', () => {
    const { pkg, conf } = fixture('1.2.3', '1.2.3');
    const before = readFileSync(conf, 'utf8');
    syncVersion(pkg, conf);
    expect(readFileSync(conf, 'utf8')).toBe(before);
  });

  it('throws when package.json has no version', () => {
    const { pkg, conf } = fixture('1.0.0', '1.0.0');
    writeFileSync(pkg, '{"name":"x"}\n');
    expect(() => syncVersion(pkg, conf)).toThrow(/version/);
  });
});
```

- [ ] **Step 5: Run the test to verify it fails**

Run: `pnpm test:run`
Expected: FAIL with `Failed to resolve import "../scripts/sync-version.mjs"` (or "Cannot find module").

- [ ] **Step 6: Write `scripts/sync-version.mjs`**

```js
// Mirrors package.json's version into src-tauri/tauri.conf.json.
// Changesets bumps package.json; Tauri reads tauri.conf.json. Cargo.toml stays 0.0.0 on purpose.
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

export function syncVersion(pkgPath, confPath) {
  const { version } = JSON.parse(readFileSync(pkgPath, 'utf8'));
  if (typeof version !== 'string' || version.length === 0) {
    throw new Error(`no version in ${pkgPath}`);
  }
  const raw = readFileSync(confPath, 'utf8');
  const conf = JSON.parse(raw);
  conf.version = version;
  const trailing = raw.endsWith('\n') ? '\n' : '';
  writeFileSync(confPath, `${JSON.stringify(conf, null, 2)}${trailing}`);
  return version;
}

const invokedDirectly = process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
  const version = syncVersion(resolve(root, 'package.json'), resolve(root, 'src-tauri/tauri.conf.json'));
  console.log(`tauri.conf.json version -> ${version}`);
}
```

- [ ] **Step 7: Let TypeScript resolve the `.mjs` import, run the test**

Add `"allowJs": true` to `compilerOptions` in `tsconfig.json` (so `tsc --noEmit` can type the relative `.mjs` import from the test; `checkJs` stays off).

Run: `pnpm test:run && pnpm typecheck`
Expected: `Tests 10 passed` (7 format + 3 sync-version); tsc clean.

- [ ] **Step 8: Add scripts and reset anchors**

In `package.json`: set `"version": "0.0.0"`; add scripts (keep the existing ones):

```json
"changeset": "changeset",
"version-packages": "changeset version && node scripts/sync-version.mjs",
"release:tag": "changeset tag"
```

In `src-tauri/tauri.conf.json`: set `"version": "0.0.0"`.

Run: `node scripts/sync-version.mjs && git diff --stat src-tauri/tauri.conf.json`
Expected: prints `tauri.conf.json version -> 0.0.0`; the diff shows only the version line changed (formatting untouched).

- [ ] **Step 9: Add the v1 changeset**

`.changeset/v1-app.md`:

```markdown
---
"claude-usage-monitor": minor
---

First release: macOS menu bar app showing Claude session and weekly usage with reset countdowns, and a popover with usage bars, links to the claude.ai usage and billing pages, and a status footer.
```

Run: `pnpm changeset status`
Expected: lists `claude-usage-monitor` with a `minor` bump.

- [ ] **Step 10: Lint, verify, commit**

Run: `pnpm check && pnpm typecheck && pnpm test:run`
Expected: clean, 10 tests. (`biome.json` `files.includes` already covers `scripts/`; if Biome flags `console.log` in the script, that rule is not in the recommended set — do not add an ignore comment; report it.)

```bash
git add .changeset scripts test/sync-version.test.ts package.json pnpm-lock.yaml tsconfig.json src-tauri/tauri.conf.json
git add .npmrc pnpm-workspace.yaml 2>/dev/null   # only if Step 3 changed them
git commit -m "chore: adopt changesets with tauri.conf.json version sync"
```

- [ ] **Step 11: Dry-run the version bump on a throwaway branch** (after the commit, because `changeset version` deletes the changeset file)

```bash
git checkout -q -b tmp/version-dry-run
pnpm version-packages
grep -n '"version"' package.json src-tauri/tauri.conf.json
head -8 CHANGELOG.md
git status --short
git reset -q --hard && git clean -fdq && git checkout -q chore/ci-release && git branch -Dq tmp/version-dry-run
git status --short
```

Expected: both files show `"version": "0.1.0"`; `CHANGELOG.md` starts with `# claude-usage-monitor` / `## 0.1.0` / `### Minor Changes`; `git status` during the dry run shows `D .changeset/v1-app.md`, `M package.json`, `M src-tauri/tauri.conf.json`, `?? CHANGELOG.md`; after cleanup the tree is clean and `.changeset/v1-app.md` is back. Record the observed output in the report.

---

### Task 2: CI workflow

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: `pnpm verify`, `pnpm tauri build` (existing), `src-tauri/rust-toolchain.toml`.
- Produces: a required check named `verify-and-build`.

- [ ] **Step 1: Write `.github/workflows/ci.yml`**

```yaml
name: CI

on:
  pull_request:
  push:
    branches: [main]

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

permissions:
  contents: read

jobs:
  verify-and-build:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4

      - uses: pnpm/action-setup@v4

      - uses: actions/setup-node@v4
        with:
          node-version: 24
          cache: pnpm

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: aarch64-apple-darwin

      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri

      - run: pnpm install --frozen-lockfile

      - run: pnpm verify

      - run: pnpm tauri build --bundles app --target aarch64-apple-darwin
```

- [ ] **Step 2: Validate YAML locally**

Run: `ruby -ryaml -e 'YAML.load_file(".github/workflows/ci.yml"); puts "yaml ok"'`
Expected: `yaml ok`. Also confirm `pnpm/action-setup@v4` reads `packageManager` — `package.json` has `"packageManager": "pnpm@12.3.4"` (no `version` input needed).

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: verify and build the macOS app on pull requests and main"
```

---

### Task 3: Release workflows

**Files:**
- Create: `.github/workflows/release.yml`, `.github/workflows/build-release.yml`

**Interfaces:**
- Consumes: `pnpm version-packages`, `pnpm release:tag` (Task 1).
- Produces: tag `vX.Y.Z` on Version-PR merge; GitHub Release with `.dmg` and `.app.tar.gz`.

- [ ] **Step 1: Write `.github/workflows/release.yml`**

```yaml
name: Release

on:
  push:
    branches: [main]

concurrency:
  group: release
  cancel-in-progress: false

permissions:
  contents: write
  pull-requests: write

jobs:
  version-or-tag:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: pnpm/action-setup@v4

      - uses: actions/setup-node@v4
        with:
          node-version: 24
          cache: pnpm

      - run: pnpm install --frozen-lockfile

      # With pending changesets: opens/updates the "Version Packages" PR.
      # Without: runs `changeset tag` and pushes the new vX.Y.Z tag, which
      # triggers build-release.yml. GitHub Releases are created there, not here.
      - uses: changesets/action@v1
        with:
          version: pnpm version-packages
          publish: pnpm release:tag
          createGithubReleases: false
          commit: "chore: version packages"
          title: "chore: version packages"
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

- [ ] **Step 2: Write `.github/workflows/build-release.yml`**

```yaml
name: Build release

on:
  push:
    tags: ["v*"]

permissions:
  contents: write

jobs:
  macos:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4

      - uses: pnpm/action-setup@v4

      - uses: actions/setup-node@v4
        with:
          node-version: 24
          cache: pnpm

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: aarch64-apple-darwin

      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri

      - run: pnpm install --frozen-lockfile

      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          tagName: ${{ github.ref_name }}
          releaseName: "Claude Usage Monitor ${{ github.ref_name }}"
          releaseBody: "See CHANGELOG.md for details. The app is unsigned: right-click the .app and choose Open on first launch."
          releaseDraft: false
          prerelease: false
          includeUpdaterJson: false
          args: --target aarch64-apple-darwin
```

- [ ] **Step 3: Check the tag format Changesets will emit**

Run (dry, no writes): `node -e "const {getPackages}=require('@manypkg/get-packages'); getPackages(process.cwd()).then(p=>console.log(p.packages.length, p.packages.map(x=>x.packageJson.name), 'root:', !!p.rootPackage))"`
Expected: `1 [ 'claude-usage-monitor' ] root: true` — a single-package repo, for which `changeset tag` emits `v<version>`. If it reports more than one package or a missing root, `changeset tag` would emit `claude-usage-monitor@<version>`: change `build-release.yml` trigger to `tags: ["v*", "claude-usage-monitor@*"]` and add a step that strips the prefix for `releaseName`:

```yaml
      - id: ver
        run: echo "name=${GITHUB_REF_NAME#claude-usage-monitor@}" >> "$GITHUB_OUTPUT"
```
and use `Claude Usage Monitor v${{ steps.ver.outputs.name }}`. Record the result in the report.

- [ ] **Step 4: Validate YAML**

Run: `ruby -ryaml -e 'Dir[".github/workflows/*.yml"].each { |f| YAML.load_file(f) }; puts "yaml ok"'`
Expected: `yaml ok`.

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/release.yml .github/workflows/build-release.yml
git commit -m "ci: version with changesets and publish macOS releases on tags"
```

---

### Task 4: Docs, push, PR, watch CI

**Files:**
- Modify: `README.md`, `CLAUDE.md`

- [ ] **Step 1: README — add a "Releases" section before "Not yet"**

```markdown
## Releases

Every user-visible change lands with a changeset (`pnpm changeset`). On `main`, the Changesets
bot keeps a "Version Packages" pull request up to date; merging it bumps `package.json` and
`src-tauri/tauri.conf.json`, writes `CHANGELOG.md`, and pushes a `vX.Y.Z` tag. The tag builds
the Apple Silicon app and publishes a GitHub Release with the `.dmg` and `.app.tar.gz`.

Download the latest build from the Releases page. The app is unsigned: right-click the `.app`
and choose Open on first launch.
```

- [ ] **Step 2: CLAUDE.md — add a "Versioning and Releases" section after "Commands", and add `pnpm changeset` to the Commands block**

Commands block, add after `pnpm tauri build`:
```bash
pnpm changeset          # add a changeset for a user-visible change (required in the PR)
```

New section:
```markdown
## Versioning and Releases

- **Every user-visible change ships with a changeset** in the same PR (`pnpm changeset`,
  file under `.changeset/`). Docs-only changes that alter what a user is told to do count.
- `package.json` is the version source. `pnpm version-packages` runs `changeset version` and
  `scripts/sync-version.mjs`, which mirrors the version into `src-tauri/tauri.conf.json`.
  `Cargo.toml` stays `0.0.0`; nothing reads `CARGO_PKG_VERSION`.
- Workflows: `ci.yml` (verify + build on PRs and `main`), `release.yml` (Changesets action:
  opens the Version Packages PR; on its merge runs `changeset tag`), `build-release.yml`
  (`v*` tags: `tauri-action` builds `aarch64-apple-darwin` and publishes the GitHub Release).
- Never edit versions by hand; never create tags by hand.
```

Also update the `**Status:**` paragraph to:
```markdown
**Status:** v1 merged to `main`. CI and Changesets-driven releases in place; first release
`0.1.0` is produced by merging the Version Packages PR. Spec:
`docs/superpowers/specs/2026-09-16-usage-monitor-v1-design.md`; pipeline spec:
`docs/superpowers/specs/2026-09-17-ci-release-design.md`.
```

- [ ] **Step 3: Verify and commit**

Run: `pnpm check && pnpm verify 2>&1 | tail -3`
Expected: clean; 10 vitest, 22 cargo tests.

```bash
git add README.md CLAUDE.md
git commit -m "docs: describe changeset-driven versioning and the release pipeline"
```

- [ ] **Step 4: Push and open the PR**

```bash
git push -u origin chore/ci-release
gh pr create --base main --head chore/ci-release --title "ci: changesets, CI and macOS release pipeline" --body-file - <<'EOF'
## Summary

- Changesets for versioning (single private package); `scripts/sync-version.mjs` mirrors the version into `tauri.conf.json`; anchors reset to `0.0.0` with a `minor` changeset so the first release is `0.1.0`.
- `ci.yml`: `pnpm verify` + `pnpm tauri build --bundles app --target aarch64-apple-darwin` on PRs and `main`.
- `release.yml`: Changesets action keeps the Version Packages PR; on merge tags `vX.Y.Z`.
- `build-release.yml`: on `v*` tags, `tauri-action` builds Apple Silicon and publishes the GitHub Release with `.dmg` and `.app.tar.gz`.

Spec: `docs/superpowers/specs/2026-09-17-ci-release-design.md`

## Test plan

- [x] `pnpm verify` green locally (10 vitest, 22 cargo)
- [x] `pnpm version-packages` dry run bumps both anchors and writes CHANGELOG
- [ ] CI green on this PR
- [ ] After merge: Version Packages PR appears; merging it produces tag `v0.1.0` and a Release with two assets

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
```

- [ ] **Step 5: Watch CI**

Run: `gh pr checks --watch --interval 30` (up to ~10 minutes on a cold cache).
Expected: `verify-and-build` passes. If it fails, read the log with `gh run view --log-failed`, fix the root cause in the workflow or code, commit with a `ci:`/`fix:` prefix, push, and watch again. Common first-run issues: `pnpm/action-setup` needing `packageManager` (present), rust-cache `workspaces` path, `tauri build` complaining about a missing target (the `targets:` input handles it).

- [ ] **Step 6: Report**

The end-to-end release path (Version Packages PR → tag → Release assets) can only be exercised after this PR merges; list it as the remaining manual verification in the report.
