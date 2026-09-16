# History-Driven Development Workflow

A Claude Code skill that provides a structured, documented approach to planning, executing, and tracking changes in your codebase.

## What Is This?

This skill teaches Claude Code to follow a consistent workflow where every change is:

1. **Planned** - With clear steps, acceptance criteria, and documented decisions
2. **Tracked** - Each step committed incrementally with meaningful messages
3. **Documented** - History records serve as PR descriptions and change history
4. **Resumable** - Interrupted work can be picked up exactly where you left off

## Benefits

- **AI Memory**: Claude can resume interrupted work by reading the plan
- **PR Documentation**: History records become your PR descriptions automatically
- **Change History**: Understand why decisions were made months later
- **Onboarding**: New team members can read history to understand the codebase evolution
- **Accountability**: Every step is committed, creating a clear audit trail
- **Smart Search**: Claude automatically finds relevant past work to inform new plans
- **Efficient**: Uses front matter metadata for fast searching without reading full records

## History Record Format

All history records include YAML front matter with searchable metadata:

```yaml
---
type: "feat"
status: "complete"
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
---
```

This metadata enables:
- **Fast searching**: Find relevant records by files, areas, or components
- **Automatic discovery**: Claude finds related work when starting new plans
- **Better context**: Understand what areas and components a change affected

### Migration

If you have existing history records without front matter, use the migration utility:

```bash
# Dry run to preview changes
pnpm migrate-history:dry-run

# Apply migration
pnpm migrate-history
```

The utility automatically extracts metadata from your existing records.

## Auto-Search Feature

When starting a new plan, Claude automatically searches for relevant history:

1. **Extracts criteria** from your task description (files, areas, components)
2. **Searches history** for related work
3. **Presents findings**: "I found 3 relevant records that touched similar areas..."
4. **Reviews records** to inform the plan with past decisions and patterns

This feature is **enabled by default** and helps Claude learn from past work.

## Prerequisites

Before using this workflow, ensure you have:

| Tool              | Required | Check Command      |
| ----------------- | -------- | ------------------ |
| git               | Yes      | `git --version`    |
| GitHub CLI (`gh`) | Yes      | `gh auth status`   |
| Claude Code       | Yes      | `claude --version` |

**Important:** All GitHub operations (creating PRs, querying repos) are performed using the `gh` CLI. Ensure it's installed and authenticated before starting.

```bash
# Verify gh is authenticated
gh auth status

# If not authenticated
gh auth login
```

## Installation

### Option 1: User-Level (Personal)

Install for all your projects:

```bash
mkdir -p ~/.claude/skills/history-driven-workflow
cp SKILL.md ~/.claude/skills/history-driven-workflow/SKILL.md
```

### Option 2: Project-Level (Team)

Install for a specific project (committed to repo):

```bash
mkdir -p .claude/skills/history-driven-workflow
cp SKILL.md .claude/skills/history-driven-workflow/SKILL.md
```

### Setup History Folder

Create the history folder in your project:

```bash
mkdir -p history
```

Add the history folder README (see `history-README.md` in this package).

## Usage

### Starting New Work

1. Open Claude Code in your project
2. Enter plan mode (use Claude Code's built-in plan mode)
3. Describe what you want to accomplish
4. Claude will:
   - Ask clarifying questions
   - **Search for relevant history** (if enabled)
   - Present any related past work
   - Create a feature branch
   - Write a plan document in `history/`
   - Iterate until you approve the plan

### Executing the Plan

1. Tell Claude to begin execution
2. Claude will:
   - Execute each step
   - Commit after each step
   - Update the plan with progress
   - Handle issues with fix commits

### Completing Work

1. Confirm all acceptance criteria are met
2. Claude will:
   - Finalize the history record
   - Rename from `_draft-*` to final name
   - Push to GitHub
   - Create a PR with the history record as description

### Resuming Interrupted Work

Just start Claude Code on the branch. Claude will:

1. Detect the incomplete plan
2. Show you where you left off
3. Wait for your instruction to continue

## Project Configuration

Add this to your project's `CLAUDE.md` to configure the workflow:

```markdown
## History-Driven Workflow

This project uses history-driven development.

The skill can be found at:
- User level: `~/.claude/skills/history-driven-workflow/SKILL.md`
- Project level: `.claude/skills/history-driven-workflow/SKILL.md`

### Configuration

| Setting                     | Value                                  |
| --------------------------- | -------------------------------------- |
| **History folder**          | `./history/`                           |
| **JIRA project**            | YOUR-PROJECT-KEY                       |
| **JIRA base URL**           | https://workwave.atlassian.net/browse/ |
| **Auto-search on planning** | `true`                                 |
| **Search depth**            | `summary`                              |
| **Min relevance score**     | `2`                                    |

**Configuration options:**
- `auto-search on planning`: Automatically find relevant history when starting plans (default: `true`)
- `search depth`: `summary` (metadata only) or `full` (complete records) (default: `summary`)
- `min relevance score`: Minimum score (1-5) to show a record as relevant (default: `2`)
```

## File Structure

```
your-project/
├── .claude/
│   └── skills/
│       └── history-driven-workflow/
│           └── SKILL.md          # Project-level skill (optional)
├── history/
│   ├── README.md                 # Explains the history folder
│   ├── _draft-2026-01-28-new-feature.md  # In-progress plan
│   └── 2026-01-15-previous-feature.md    # Completed record
├── CLAUDE.md                     # Project config
└── ...
```

## Workflow Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                      PLAN MODE                               │
├─────────────────────────────────────────────────────────────┤
│  1. Ask clarifying questions                                │
│  2. Create feature branch (if on main)                      │
│  3. Write _draft-*.md with plan                             │
│  4. Iterate until approved                                  │
│  5. Commit: "docs: add plan for [description]"              │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    EXECUTION MODE                            │
├─────────────────────────────────────────────────────────────┤
│  For each step:                                             │
│    1. Execute the work                                      │
│    2. Update execution log                                  │
│    3. Commit: "[type]: step N - [description]"              │
│                                                             │
│  On interruption:                                           │
│    1. Document pause point                                  │
│    2. Commit: "docs: pause execution"                       │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                   COMPLETION MODE                            │
├─────────────────────────────────────────────────────────────┤
│  1. Verify acceptance criteria                              │
│  2. Complete Files Changed & Testing sections               │
│  3. Rename: remove _draft- prefix                           │
│  4. Commit: "docs: complete [description]"                  │
│  5. Push branch                                             │
│  6. Create PR with history record as body                   │
└─────────────────────────────────────────────────────────────┘
```

## FAQ

**Q: What if I need to change the plan mid-execution?**
A: Update the plan document, note the change in the execution log, and commit with `docs: update plan for [description]`.

**Q: Can I have multiple plans in progress?**
A: Currently, one plan per branch. For parallel work, use git worktrees with separate branches.

**Q: What if Claude doesn't detect my incomplete plan?**
A: Ensure the file starts with `_draft-` and is in the correct history folder. You can also explicitly tell Claude to check for incomplete plans.

**Q: How do I abandon a plan?**
A: Either delete the `_draft-*.md` file, or ask Claude to mark it as abandoned (it will add a note and rename appropriately).

## Contributing

Improvements to this workflow are welcome! Please follow the history-driven workflow itself when making changes. 😉