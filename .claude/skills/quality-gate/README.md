# Quality Gate Skill

Mandatory completion checklist that runs automatically after every implementation task.

## What It Checks
- `pnpm verify` (biome, tsc, vitest, cargo fmt/clippy/test)
- Tauri build when `src-tauri/` changed
- README/docs sync
- Staleness scan (orphaned files, outdated docs, unused deps)
- Capabilities file matches the IPC the code uses

## Usage
Invoked automatically at task completion. Can also be run manually:
```
/quality-gate
```

## Pre-Work Checks
Also enforces before starting work:
- Feature branch required (hard block on main)
- History context search
- Clarification questions
