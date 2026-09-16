---
name: quality-gate
description: Mandatory completion checklist that runs automatically after every task to verify docs, tests, and code health are in sync with changes.
---

# Quality Gate

## Overview

Automatic checklist that runs at task completion. Every implementation task must pass these checks before committing or creating a PR.

**Announce at start:** "Running quality-gate checks before completing this work."

## Activation

This skill activates automatically when:
- A task or plan step is marked complete
- Before any commit of implementation work
- Before invoking `superpowers:finishing-a-development-branch`
- User explicitly invokes `/quality-gate`

## Pre-Work Checks (run BEFORE starting any implementation)

### P1: Feature Branch Enforcement
- Check current branch: `git branch --show-current`
- If on `main` or `master`: **HARD BLOCK** — refuse to proceed
- Message: "You're on [branch]. Create a feature branch before starting work. Use superpowers:using-git-worktrees or create one manually."
- Exception: Only if user explicitly says "work on main" in this conversation

### P2: History Context Search
- Search claude-mem for relevant past work: `mcp search` with keywords from the task
- Search history records in `./history/` if they exist (use history-search skill)
- Present findings: "Found N relevant records. [brief summary]"
- If none found: "No relevant history found. Proceeding fresh."

### P3: Clarification Questions
- Before making assumptions about scope, approach, or acceptance criteria — ASK
- Prefer multiple-choice questions (AskUserQuestion) over open-ended
- Minimum 1 clarifying question per non-trivial task

## Completion Checks (run AFTER implementation, BEFORE commit/PR)

Run each check. Report results as a table:

| # | Check | Status | Details |
|---|-------|--------|---------|
| 1 | Verify | ... | ... |
| 2 | Tauri build | ... | ... |
| 3 | Docs | ... | ... |
| 4 | Staleness scan | ... | ... |
| 5 | Capabilities | ... | ... |

### C1: Verify
**What:** All linters, type checks, and tests pass on both sides.
**How:** Run `pnpm verify` (biome check, tsc --noEmit, vitest run, cargo fmt --check, cargo clippy -D warnings, cargo test). New pure logic in `src/lib/` has a Vitest spec in `test/`; new Rust parsing or state-machine logic has an inline `#[cfg(test)]` test.
**Status values:** PASS, FIX (list failures), ADD (tests missing for changed logic)

### C2: Tauri Build
**What:** The app still compiles and bundles when the Rust side or Tauri config changed.
**How:** If the diff touches `src-tauri/` or `tauri.conf.json`, run `pnpm tauri build` (or at least `cargo build --release` in `src-tauri/`). Skip for frontend-only changes.
**Status values:** PASS, FIX, SKIP

### C3: Docs Sync
**What:** README and `docs/` reflect new behavior, settings, or data-source changes.
**How:**
1. New setting, command, or user-visible behavior -> README updated
2. Change to endpoints, headers, polling, or credential handling -> `docs/research-usage-monitors.md` and the Data Source section of `CLAUDE.md` updated
3. Inline comments on changed public functions explain why, not what
**Status values:** PASS, UPDATE, SKIP

### C4: Staleness Scan
**What:** Flag orphaned files and unused dependencies.
**How:**
1. Files created but never imported (components, utilities, Rust modules not in `lib.rs`)
2. `package.json` and `Cargo.toml` dependencies added but unused, or now removable
3. Doc sections that reference removed/renamed code
**Status values:** CLEAN, FLAGS (list items found)

### C5: Capabilities
**What:** `src-tauri/capabilities/default.json` grants exactly what the code uses.
**How:**
1. Every `invoke('plugin:...')` and Tauri API call in `src/lib/ipc.ts` has a matching permission
2. No permission remains for a command no longer called
3. `opener:allow-open-url` keeps its explicit `allow` URL scope
**Status values:** PASS, UPDATE, SKIP (no IPC changes)

## Output Format

After running all checks, present:

```
## Quality Gate Results

| # | Check | Status | Details |
|---|-------|--------|---------|
| 1 | Verify | PASS | pnpm verify green |
| 2 | Tauri build | SKIP | frontend-only change |
| 3 | Docs | UPDATE | README needs the new threshold setting |
| 4 | Staleness scan | CLEAN | No issues found |
| 5 | Capabilities | PASS | no IPC changes |

**Action needed:** Item 3 requires an update before completing.
```

If all PASS/SKIP/CLEAN -> "Quality gate passed. Ready to commit."
If any UPDATE/ADD/FIX/FLAGS -> List required actions and complete them before proceeding.

## Integration with superpowers Workflow

This skill slots into the superpowers workflow:
1. `superpowers:brainstorming` -> design
2. `superpowers:writing-plans` -> plan
3. **quality-gate Pre-Work Checks (P1-P3)** -> verify branch, search history, ask questions
4. `superpowers:executing-plans` -> implement
5. **quality-gate Completion Checks (C1-C5)** -> verify everything is in sync
6. `superpowers:finishing-a-development-branch` -> merge/PR
