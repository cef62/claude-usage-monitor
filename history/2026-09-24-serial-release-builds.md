---
type: "fix"
status: "complete"
files:
  - .github/workflows/build-release.yml
  - .changeset/macos-update-feed.md
  - CLAUDE.md
areas:
  - ci
  - updater
components:
  - build-release-workflow
tags:
  - tauri-action
  - latest-json
  - race-condition
related-to:
  - history/2026-09-23-updater-notes.md
---

# fix: build release platforms one at a time

| Field       | Value                          |
| ----------- | ------------------------------ |
| **Status**  | complete                       |
| **Branch**  | `fix/serial-release-builds`    |
| **Ticket**  | none                           |
| **Created** | 2026-09-24                     |
| **Updated** | 2026-09-24                     |

## Summary

v0.14.0's `latest.json` lists only `windows-x86_64` / `windows-x86_64-nsis`, so macOS installs
never saw the update. The macOS and Windows matrix jobs finished one second apart
(18:48:58 / 18:48:59); tauri-action merges its platform into `latest.json` by download → edit →
re-upload, and the Windows job read the file before macOS had written its entry. Earlier releases
only worked because Windows finished minutes later.

Published releases are immutable, so 0.14.0 stays broken; 0.14.1 ships the fix.

- `max-parallel: 1` on the build matrix: platforms build one after the other (~5 min slower).
- `publish` now fails, leaving the release a draft, unless `latest.json` has both
  `darwin-aarch64` and `windows-x86_64`. Checked locally: passes on v0.13.0, fails on v0.14.0.

## Check on the next release

`gh release download vX.Y.Z -p latest.json -O - | jq '.platforms | keys'` lists all four keys.
