---
type: "fix"
status: "complete"
files:
  - .github/workflows/build-release.yml
  - .changeset/updater-notes.md
areas:
  - ci
  - updater
components:
  - build-release-workflow
tags:
  - tauri-action
  - latest-json
related-to:
  - history/2026-09-23-rotate-updater-key.md
---

# fix: put the release notes into latest.json

| Field       | Value               |
| ----------- | ------------------- |
| **Status**  | complete            |
| **Branch**  | `fix/updater-notes` |
| **Ticket**  | none                |
| **Created** | 2026-09-23          |
| **Updated** | 2026-09-23          |

## Summary

`latest.json` for v0.11.3 and v0.11.4 has `"notes": ""`. `build-release.yml` creates the
release itself (`gh release create --notes-file`) and hands tauri-action only `releaseId`, but
tauri-action builds the updater JSON `notes` from its `releaseBody` input
(`src/index.ts` → `uploadVersionJSON`), which was never set.

Effect: the update notification fell back to the menu hint, and 0.11.2 installs never saw the
`On 0.11.2 or older?` bullet the key rotation relied on (only the site banner and release page
reach them). Published releases are immutable, so 0.11.3/0.11.4 stay as they are.

Fix: the `create-release` job exports `notes.md` as a multiline output (`notes<<EOF_NOTES`)
and the build job passes it as `releaseBody`.

## Check on the next release

`gh release download vX.Y.Z -p latest.json -O - | jq .notes` must start with the
`- On 0.11.2 or older?` bullet.
