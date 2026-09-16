---
name: history-driven-workflow
description: A structured workflow for planning, executing, and documenting changes using persistent history records that serve as AI memory, PR documentation, and change history.
---

# History-Driven Workflow

A structured workflow for planning, executing, and documenting changes using persistent history records that serve as AI memory, PR documentation, and change history.

## Activation

This workflow activates automatically when:
- User enters plan mode
- User mentions: "plan", "history-driven", "start new work", "new feature", "new task"
- A `_draft-*.md` file exists in the history folder of the current branch
- User references planning, documenting changes, or PR preparation

This skill is designed to work alongside other skills and plugins. It focuses on workflow orchestration and does not conflict with code-generation or other specialized skills.

## Prerequisites

The following tools must be installed and configured:

| Tool                | Purpose                                   | Verification     |
| ------------------- | ----------------------------------------- | ---------------- |
| **git**             | Version control                           | `git --version`  |
| **gh** (GitHub CLI) | All GitHub operations (PRs, repo queries) | `gh auth status` |

**Important:** All read and write operations to GitHub must use the `gh` CLI tool. Do not use direct API calls or other methods.

### GitHub CLI Quick Setup

If `gh` is not configured:
```bash
# Install (macOS)
brew install gh

# Install (Windows)
winget install GitHub.cli

# Authenticate
gh auth login
```

## Workflow Overview

When the user enters plan mode or requests to start a new task:

1. **Check for incomplete plans** in the current branch's history folder
2. **If found**: Notify user and wait for instructions  
3. **If not found**: Begin the planning phase

---

## Critical: Commit Discipline

**Every state change must be committed.** This is essential for resumability and audit trails.

| Event             | Required Commit                          |
| ----------------- | ---------------------------------------- |
| Plan created      | `docs: add plan for [description]`       |
| Plan updated      | `docs: update plan for [description]`    |
| Execution started | `docs: begin execution of [description]` |
| Step completed    | `[type]: step N - [step description]`    |
| Issue fixed       | `fix: [description]`                     |
| Work paused       | `docs: pause execution - [reason]`       |
| Work completed    | `docs: complete [description]`           |

**Never proceed to the next phase or step without committing the current state.**

---

## Phase 1: Planning

### 1.1 Initial Discovery

When entering plan mode:

1. Confirm you are in Claude Code's built-in plan mode
2. Ask all clarifying questions needed to understand the full scope
3. **Do NOT proceed** until you have enough information to create a complete plan
4. Generate acceptance criteria collaboratively with the user
5. List any unanswered questions explicitly

### 1.1a Search Relevant History (Auto-Search)

**If enabled in configuration (default: true)**, after initial discovery and before creating the plan:

1. **Extract search criteria** from the user's task description:
   - File paths mentioned or implied
   - Areas affected (workflows, scripts, tests, docs, core, config)
   - Components involved (from context)
   - Type of work (feat, fix, docs, etc.)

2. **Search for relevant history** using the history-search skill:
   ```
   Construct query from extracted criteria, e.g.:
   - files:src/changelog.ts area:core
   - area:workflows component:release
   - files:.github/workflows/ type:feat
   ```

3. **Present findings to user:**
   ```markdown
   I found N relevant history records that touched similar areas:

   1. [Record Title] (YYYY-MM-DD) - Brief summary
   2. [Record Title] (YYYY-MM-DD) - Brief summary

   I'll review these records to inform the plan. Would you like me to show you any of these in detail?
   ```

4. **Review relevant records** to inform planning:
   - Load full content of highly relevant records (relevance score >= 3)
   - Extract lessons learned, patterns, and approaches
   - Note any related work or dependencies

5. **If no relevant records found:**
   ```
   I searched the history for related work but didn't find any directly relevant records.
   This appears to be new territory. I'll proceed with planning based on our discussion.
   ```

**Configuration options** (in project CLAUDE.md):

```markdown
| Setting                     | Value      |
| --------------------------- | ---------- |
| **Auto-search on planning** | `true`     |
| **Search depth**            | `summary`  |
| **Min relevance score**     | `2`        |
```

- `auto-search on planning`: Enable/disable automatic history search (default: `true`)
- `search depth`: `summary` (metadata only) or `full` (load complete records) (default: `summary`)
- `min relevance score`: Minimum score to consider a record relevant (1-5, default: `2`)

**Manual search:**

Users can also explicitly request history search at any time:
- "Search history for similar work"
- "Have we done anything like this before?"
- Use the history-search skill directly

### 1.2 Branch Creation

Before writing the plan, check if on `main` or `master`. If so:

1. Ask for ticket number (optional) and short description
2. Create branch following the pattern:
   - With ticket: `[type]/TICKET-123-short-description`
   - Without ticket: `[type]/short-description`
3. Valid types: `feat`, `fix`, `chore`, `docs`

### 1.3 Plan Document Creation

1. Determine the history folder path (default: `./history/`, check project CLAUDE.md for overrides)
2. Create file: `_draft-YYYY-MM-DD-short-description.md`
3. Use the template structure from Section 6
4. Set status: `draft`
5. Include all unanswered questions in the "Unanswered Questions" section
6. **⚠️ COMMIT IMMEDIATELY:** `docs: add plan for [short-description]`

### 1.4 Plan Iteration

- Iterate with the user until the plan is explicitly approved
- **⚠️ COMMIT after significant changes:** `docs: update plan for [short-description]`
- Remove questions from "Unanswered Questions" as they're resolved
- Delete the "Unanswered Questions" section entirely when empty

---

## Phase 2: Execution

### 2.1 Starting Execution

1. Update status from `draft` to `in-progress`
2. **⚠️ COMMIT IMMEDIATELY:** `docs: begin execution of [short-description]`

### 2.2 Executing Steps

For each step in the plan:

1. Update step status comment to `in-progress`
2. Execute the work defined in the step
3. **On success:**
   - Update step status comment to `complete`
   - Add entry to Execution Log with timestamp and notes
   - **⚠️ COMMIT IMMEDIATELY:** `[type]: step N - [step description]`
4. **On issues:**
   - **⚠️ COMMIT fix:** `fix: [description of fix]`
   - Update plan with notes about what changed
   - If blocked, update status to `blocked` and document reason

### 2.3 Handling Interruptions

If work must be paused:

1. Update the current step with clear interruption notes
2. Add a marker in Execution Log:
   ```markdown
   ### Paused - YYYY-MM-DD HH:MM
   - Last completed: Step N
   - Current state: [description]
   - To resume: [what's needed]
   ```
3. **⚠️ COMMIT IMMEDIATELY:** `docs: pause execution - [reason if any]`

---

## Phase 3: Completion

### 3.1 Finalizing the Record

1. Verify all acceptance criteria are met (update checkboxes)
2. Complete the "Files Changed" section with all modified files
3. Complete the "Testing" section with verification details
4. Add any observations to "Final Notes"
5. Change status to `complete`
6. Rename file: remove `_draft-` prefix
7. **⚠️ COMMIT IMMEDIATELY:** `docs: complete [short-description]`

### 3.2 Creating the Pull Request

**Prerequisite:** The GitHub CLI (`gh`) must be installed and authenticated. All GitHub operations use `gh`.

1. Push branch to GitHub: `git push -u origin [branch-name]`
2. Create PR using GitHub CLI:
   ```bash
   gh pr create --title "[type]: [short-description]" --body-file [history-file-path]
   ```
   Or with ticket: `gh pr create --title "[TICKET-123] [short-description]" --body-file [history-file-path]`
3. Share the PR link with the user

---

## Phase 4: Resumption Protocol

### On Session Start

Check for files matching `_draft-*.md` in the history folder of the **current branch only**.

### If Incomplete Plan Found

Display this notification and **wait for user instruction**:

```
I found an incomplete plan: `_draft-YYYY-MM-DD-feature-name.md`

**Status:** [status]
**Last step completed:** Step N - [description]
**Next pending step:** Step N+1 - [description]

Would you like to:
1. Continue from where we left off
2. Review the plan first
3. Abandon this plan
```

Do NOT proceed until the user responds.

---

## Section 5: Commit Message Conventions

| Action                 | Format                                   |
| ---------------------- | ---------------------------------------- |
| Plan creation          | `docs: add plan for [description]`       |
| Plan updates           | `docs: update plan for [description]`    |
| Execution start        | `docs: begin execution of [description]` |
| Step completion        | `[type]: step N - [step description]`    |
| Fixes during execution | `fix: [description]`                     |
| Pause                  | `docs: pause execution - [reason]`       |
| Completion             | `docs: complete [description]`           |

---

## Section 6: History Record Template

```markdown
---
type: "feat"
status: "draft"
files:
  - path/to/file1.ts
  - path/to/file2.yml
areas:
  - workflows
  - core
components:
  - release-automation
tags:
  - typescript
related-to:
  - history/YYYY-MM-DD-related-record.md
  - TICKET-123
---

# [type]: Short Description

| Field       | Value                                                          |
| ----------- | -------------------------------------------------------------- |
| **Status**  | draft \| in-progress \| complete \| blocked                    |
| **Branch**  | `[type]/TICKET-123-short-description`                          |
| **Ticket**  | [TICKET-123](https://workwave.atlassian.net/browse/TICKET-123) |
| **Created** | YYYY-MM-DD                                                     |
| **Updated** | YYYY-MM-DD                                                     |

## Summary

Brief overview of what this change accomplishes.

## Initial Request

The original ask or problem statement, captured verbatim or paraphrased.

## Acceptance Criteria

- [ ] Criterion 1
- [ ] Criterion 2
- [ ] Criterion 3

## Unanswered Questions

> Remove this section when all questions are resolved.

| #   | Question                               |
| --- | -------------------------------------- |
| 1   | Example question that needs user input |

## Plan

### Step 1: Description
<!-- Status: pending | in-progress | complete | skipped -->

Details of what this step accomplishes and how.

**Commit:** `[type]: step 1 - description`

### Step 2: Description
<!-- Status: pending -->

Details of what this step accomplishes and how.

**Commit:** `[type]: step 2 - description`

## Execution Log

> Populated during execution. Remove this note when execution begins.

### Step 1 - Complete ✓
- Completed: YYYY-MM-DD HH:MM
- Notes: Any relevant observations or deviations from plan

### Step 2 - In Progress
- Started: YYYY-MM-DD HH:MM

## Files Changed

_Updated upon completion_

- `path/to/file.ts` - Brief description of change
- `path/to/another.ts` - Brief description of change

## Testing

_How the change was verified_

- Manual testing performed: [description]
- Automated tests added/updated: [list]
- Edge cases verified: [list]

## Final Notes

_Post-completion observations, learnings, or follow-up items_
```

### Front Matter Field Definitions

| Field | Type | Description | Example |
|-------|------|-------------|---------|
| **type** | string | Change type | `feat`, `fix`, `docs`, `chore`, `test`, `refactor` |
| **status** | string | Current status | `draft`, `in-progress`, `complete`, `blocked`, `abandoned` |
| **files** | array | Files changed in this work | `["src/main.ts", ".github/workflows/ci.yml"]` |
| **areas** | array | High-level categories affected | `["workflows", "scripts", "tests"]` |
| **components** | array | Functional components involved | `["release-automation", "changelog"]` |
| **tags** | array | Additional searchable keywords | `["github-actions", "automation"]` |
| **related-to** | array | References to related records/tickets | `["history/2026-01-30-release-action.md", "PROJ-123"]` |

---

## Section 7: Configuration

### Project-Level Configuration

Projects can override defaults in their `CLAUDE.md`:

```markdown
## History-Driven Workflow Configuration

| Setting                     | Value                                    |
| --------------------------- | ---------------------------------------- |
| **History folder**          | `./history/`                             |
| **JIRA project**            | PROJECT_KEY                              |
| **JIRA base URL**           | https://company.atlassian.net/browse/    |
| **Auto-search on planning** | `true`                                   |
| **Search depth**            | `summary`                                |
| **Min relevance score**     | `2`                                      |
```

### Default Values

| Setting                     | Default Value                            |
| --------------------------- | ---------------------------------------- |
| **History folder**          | `./history/`                             |
| **JIRA base URL**           | https://workwave.atlassian.net/browse/   |
| **Auto-search on planning** | `true`                                   |
| **Search depth**            | `summary` (options: `summary`, `full`)   |
| **Min relevance score**     | `2` (range: 1-5)                         |

### Configuration Field Descriptions

- **History folder**: Location of history records (relative to project root)
- **JIRA project**: Default JIRA project key for ticket references
- **JIRA base URL**: Base URL for creating JIRA ticket links
- **Auto-search on planning**: Automatically search relevant history when starting new plans
- **Search depth**: `summary` returns metadata only, `full` loads complete record content
- **Min relevance score**: Threshold for showing a record as relevant (1=any match, 5=highly relevant)