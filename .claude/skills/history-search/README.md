# History Search Skill

A Claude Code skill for efficiently searching and filtering history records based on metadata.

## What Is This?

This skill enables Claude to quickly find relevant history records without reading entire documents. It uses YAML front matter in history records to filter by:

- **Files changed**: Find records that modified specific files
- **Areas**: Search by high-level categories (workflows, scripts, tests, etc.)
- **Components**: Find work on specific functional components
- **Type**: Filter by change type (feat, fix, docs, etc.)
- **Status**: Show only drafts, in-progress, or completed work
- **Tags**: Search by keywords
- **Combinations**: Use multiple criteria together

## Benefits

- **Fast**: Grep-based extraction only reads front matter, not full files
- **Relevant**: Find related work before starting new tasks
- **Informed**: Learn from past decisions and patterns
- **Efficient**: No need to manually browse all history records
- **Smart**: Auto-ranks results by relevance score

## Prerequisites

- History records must include YAML front matter (see history-driven-workflow skill)
- History folder exists (default: `./history/`)
- Standard Unix tools available (grep, sed)

## Installation

### Option 1: User-Level (Personal)

```bash
mkdir -p ~/.claude/skills/history-search
cp SKILL.md ~/.claude/skills/history-search/SKILL.md
```

### Option 2: Project-Level (Team)

```bash
mkdir -p .claude/skills/history-search
cp SKILL.md .claude/skills/history-search/SKILL.md
```

## Quick Start

### Basic Searches

```bash
# Find records that modified a specific file
/history search files:src/changelog.ts

# Find all workflow-related changes
/history search area:workflows

# Find completed features
/history search type:feat status:complete

# Find work on a specific component
/history search component:release-automation
```

### Combination Searches

```bash
# Find completed features in workflows
/history search type:feat area:workflows status:complete

# Find test-related changes to TypeScript files
/history search files:*.ts area:tests

# Find release-related work
/history search component:release area:workflows,scripts
```

## Search Query Syntax

### File Matching

```bash
files:path/to/file.ts           # Exact match
files:*.yml                     # Glob pattern
files:.github/workflows/        # Directory match
files:src/,tests/               # Multiple files (OR)
```

### Area Matching

```bash
area:workflows                  # Single area
area:workflows,scripts          # Multiple areas (OR)
```

**Valid areas:** `workflows`, `scripts`, `tests`, `docs`, `core`, `config`

### Component Matching

```bash
component:release-automation    # Single component
component:changelog,github      # Multiple components (OR)
```

### Type Matching

```bash
type:feat                       # Single type
type:feat,fix                   # Multiple types (OR)
```

**Valid types:** `feat`, `fix`, `docs`, `chore`, `test`, `refactor`

### Status Matching

```bash
status:complete                 # Single status
status:in-progress,blocked      # Multiple statuses (OR)
```

**Valid statuses:** `draft`, `in-progress`, `complete`, `blocked`, `abandoned`

### Tag Matching

```bash
tag:github-actions              # Single tag
tag:automation,ci-cd            # Multiple tags (OR)
```

### Combining Criteria

Separate criteria with spaces for AND logic:

```bash
# All of these must match
/history search files:src/ type:feat status:complete

# Files in src/ AND type is feat AND status is complete
```

## Output Format

### Summary View (Default)

Shows metadata and brief summary:

```
Found 3 relevant history records:

## 1. feat: Transform Release Workflow into GitHub Action (2026-01-30)
**Status:** complete
**Files:** src/changelog.ts, src/github.ts, +7 more
**Areas:** workflows, scripts, core
**Components:** release-automation, changelog-generation

Brief overview of what this change accomplishes.
```

### Full View

Load complete record when needed:

```
/history show 2026-01-30-release-action.md
```

## Integration with History-Driven Workflow

When using the history-driven-workflow skill, history search runs automatically during planning:

1. You enter plan mode and describe your task
2. Claude extracts key information from your description
3. History search finds relevant past work
4. Claude presents: "I found 3 relevant records that might inform this work..."
5. You can review findings before creating the plan

**Configure in CLAUDE.md:**

```markdown
## History-Driven Workflow Configuration

| Setting                     | Value     |
| --------------------------- | --------- |
| **Auto-search on planning** | `true`    |
| **Search depth**            | `summary` |
| **Min relevance score**     | `2`       |
```

## Use Cases

### Before Starting New Work

```
You: "I want to improve the changelog generation"
Claude: "Let me search for relevant history..."
        /history search component:changelog
        "I found 2 records about changelog work. Let me review those first..."
```

### Finding Related Fixes

```
You: "The release workflow is failing"
Claude: /history search area:workflows type:fix
        "I found 3 previous fixes in workflows. Here's what was done..."
```

### Understanding a Component

```
You: "How does the template sync work?"
Claude: /history search component:template-sync
        "I found the complete history of template sync development..."
```

### Learning from Past Patterns

```
You: "What's our approach to GitHub Actions?"
Claude: /history search area:workflows tag:github-actions
        "Here's how we've built GitHub Actions in the past..."
```

## Additional Commands

### List All Records

```bash
/history list
```

### Show Specific Record

```bash
/history show 2026-01-30-release-action.md
```

### Statistics

```bash
/history stats
```

Shows:
- Total records
- Distribution by type, area, status
- Most frequently changed files
- Common components

## Troubleshooting

### No Results Found

Try:
- Broadening your search (area instead of specific files)
- Checking file paths match exactly
- Using component or type instead
- Running `/history list` to see available records

### Some Records Missing Front Matter

Run the migration utility:

```bash
pnpm migrate-history
```

This adds front matter to all existing history records.

### Search Too Slow

History search is designed to be fast. If slow:
- Check history folder size (thousands of records?)
- Ensure grep/sed are available and working
- Consider archiving very old records

## Examples

### Example 1: Starting Work on Workflows

```
You: "I need to update the CI workflow"
Claude: /history search area:workflows files:_ci.yml

Result:
Found 2 records:
1. Backport Improvements from Fork
   - Renamed ci.yml to _ci.yml
   - Updated workflow structure
2. feat: Template Sync Workflow
   - Added workflow naming conventions
```

### Example 2: Understanding Release Process

```
You: "How do we handle releases?"
Claude: /history search component:release-automation

Result:
Found 2 records:
1. feat: Transform Release Workflow into GitHub Action
   - Complete release automation system
   - Changelog generation, tagging, GitHub Releases
2. Backport Improvements from Fork
   - Original release workflow setup
```

### Example 3: Finding Test Patterns

```
You: "Show me how we write tests"
Claude: /history search area:tests type:test

Result:
Found tests in:
1. feat: Transform Release Workflow
   - 108 tests across 6 test files
   - Vitest with mocked @actions/core
```

## See Also

- **history-driven-workflow skill**: Main workflow that uses this search
- **CLAUDE.md**: Project configuration
- **history/ folder**: All history records