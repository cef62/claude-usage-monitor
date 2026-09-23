---
type: "fix"
status: "in-progress"
files:
  - .secrets/ (removed from the tree)
  - src-tauri/tauri.conf.json
  - src-tauri/src/update.rs
  - .github/workflows/build-release.yml
  - .changeset/rotate-updater-key.md
  - README.md
areas:
  - security
  - updater
  - ci
components:
  - update
  - build-release-workflow
tags:
  - minisign
  - key-rotation
  - tauri-plugin-updater
related-to:
  - history/2026-09-19-auto-update.md
---

# fix: rotate the updater signing key

| Field       | Value                        |
| ----------- | ---------------------------- |
| **Status**  | in-progress                  |
| **Branch**  | `fix/rotate-updater-key`     |
| **Ticket**  | none                         |
| **Created** | 2026-09-23                   |
| **Updated** | 2026-09-23                   |

## Summary

The local backup of the updater keypair (`.secrets/`: private key, its password, public key)
was committed to the public repo in #22 (2026-09-21 14:53). The `/.secrets/` ignore rule landed
nine minutes later in #24, but an ignore rule does not untrack files already committed. The key
(id `15F93E2CFFDF07CD`) was the live signing key, so anyone holding it could sign an update that
0.8.0–0.11.2 accept; exploiting it still needs control of the GitHub release feed.

Git history was not rewritten: the repo is public and already forked, and the rotation
neutralises the key. `.secrets/` is removed from the tree in its own commit.

## Rotation

Installed apps trust only the pubkey compiled into them, so the rotation goes through a bridge:

1. **0.11.3 (bridge)** carries the new pubkey (`C62D8762D2E6E325`) but is signed by CI with the
   old key, so 0.11.2 and older install it normally and come out trusting only the new key.
2. After 0.11.3 is published, the `TAURI_SIGNING_PRIVATE_KEY` / `_PASSWORD` repo secrets switch
   to the new key; every later release is signed with it.
3. From then on `build-release.yml` puts a bullet starting with `On 0.11.2 or older?` first in
   the release notes. 0.11.2 shows the first bullet of the notes in its "update available"
   notification, so users who skipped the bridge learn to download by hand (the in-app install
   fails on the signature). 0.11.3+ skip that bullet (`update::LEGACY_KEY_NOTICE`).

The new keypair and password live in `~/.tauri/claude-usage-monitor-v2.*` and the password
manager, never inside the working tree.

## Lessons

- After adding an ignore rule for something already committed, run `git ls-files <path>` and
  `git rm --cached` it.
