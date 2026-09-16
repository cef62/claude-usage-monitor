---
name: history-search
description: Search and filter history records by files, areas, components, and other metadata using efficient front matter parsing.
---

# History Search Skill

Search and filter history records based on YAML front matter metadata. Efficiently finds relevant records without reading entire documents.

## Activation

This skill activates when:
- User explicitly invokes: `/history search <query>`
- User mentions: "search history", "find history records", "related history"
- History-driven-workflow skill requests relevant records
- User asks: "what history do we have about X?"

## Prerequisites

- History records must include YAML front matter (see history-driven-workflow skill)
- History folder must exist (default: `./history/`)
- `grep` command available (standard on Unix-like systems)

## Search Capabilities

### 1. Search by File Path

Find records that modified specific files:

```bash
/history search files:src/changelog.ts
/history search files:*.yml
/history search files:.github/workflows/
```

**Matching modes:**
- **Exact**: `files:src/main.ts` - exact file path match
- **Glob**: `files:*.ts` - glob pattern matching
- **Substring**: `files:workflow` - partial path matching

### 2. Search by Area

Find records in high-level categories:

```bash
/history search area:workflows
/history search area:scripts
/history search area:tests,docs
```

**Valid areas:** workflows, scripts, tests, docs, core, config

### 3. Search by Component

Find records affecting functional components:

```bash
/history search component:release-automation
/history search component:changelog
/history search component:auth,sync
```

### 4. Search by Type

Find records of specific change types:

```bash
/history search type:feat
/history search type:fix
/history search type:docs,chore
```

**Valid types:** feat, fix, docs, chore, test, refactor

### 5. Search by Status

Find records in specific states:

```bash
/history search status:complete
/history search status:in-progress
/history search status:draft,blocked
```

**Valid statuses:** draft, in-progress, complete, blocked, abandoned

### 6. Search by Tags

Find records with specific keywords:

```bash
/history search tag:github-actions
/history search tag:automation,ci-cd
```

### 7. Combination Queries

Combine multiple criteria with AND logic:

```bash
/history search files:src/ type:feat status:complete
/history search area:workflows component:release
/history search files:*.yml area:workflows type:feat
```

## Search Algorithm

### Phase 1: Extract Front Matter

Use grep to efficiently extract only the front matter blocks:

```bash
# Extract front matter from all history records
grep -l "^---$" history/*.md | while read file; do
  sed -n '/^---$/,/^---$/p' "$file"
done
```

### Phase 2: Parse and Filter

For each front matter block:
1. Parse YAML to extract metadata fields
2. Apply filter criteria (files, areas, components, type, status, tags)
3. Score relevance (how many criteria matched)
4. Rank results by relevance score

### Phase 3: Return Results

**Default output format (summary + metadata):**

```markdown
Found 3 relevant history records:

## 1. feat: Transform Release Workflow into GitHub Action (2026-01-30)
**Status:** complete
**Files:** src/changelog.ts, src/github.ts, .github/workflows/_release.yml, +7 more
**Areas:** workflows, scripts, core
**Components:** release-automation, changelog-generation

Brief overview of what this change accomplishes.

---

## 2. feat: Template Sync Workflow (2026-01-28)
**Status:** complete
**Files:** .github/workflows/_template-sync.yml, script/template-sync.mts
**Areas:** workflows, scripts
**Components:** template-sync

Automated workflow for syncing changes from template...

---

## 3. Backport Improvements from gha-publish-to-aws-registry (2026-01-28)
**Status:** complete
**Files:** .github/workflows/_release.yml, script/generate-changelog.mjs
**Areas:** workflows, scripts
**Components:** release-automation

Backported internal improvements from fork...
```

**Optional: Return full records**

When more detail is needed, load and return complete record content.

## Implementation Functions

### Core Search Functions

```typescript
interface SearchQuery {
  files?: string[]      // File paths or patterns
  areas?: string[]      // High-level categories
  components?: string[] // Functional components
  type?: string[]       // Change types
  status?: string[]     // Current status
  tags?: string[]       // Keywords
}

interface HistoryRecord {
  filepath: string
  frontMatter: {
    type: string
    status: string
    files: string[]
    areas: string[]
    components: string[]
    tags: string[]
    relatedTo: string[]
  }
  title: string
  summary: string
  relevanceScore: number
}

/**
 * Search history records by query criteria
 */
async function searchHistory(
  query: SearchQuery,
  historyFolder: string = './history/'
): Promise<HistoryRecord[]>

/**
 * Extract front matter from a markdown file
 */
function extractFrontMatter(filepath: string): Record<string, any> | null

/**
 * Parse search query string into SearchQuery object
 * Example: "files:src/main.ts area:core type:feat" -> { files: ['src/main.ts'], areas: ['core'], type: ['feat'] }
 */
function parseSearchQuery(queryString: string): SearchQuery

/**
 * Calculate relevance score for a record against query
 */
function calculateRelevance(
  frontMatter: Record<string, any>,
  query: SearchQuery
): number

/**
 * Match file path against pattern (exact, glob, or substring)
 */
function matchesFilePath(filepath: string, pattern: string): boolean

/**
 * Format search results for display
 */
function formatSearchResults(
  records: HistoryRecord[],
  format: 'summary' | 'full' = 'summary'
): string
```

### Grep-Based Extraction

```bash
#!/bin/bash
# Extract front matter efficiently using grep and sed

HISTORY_DIR="${1:-./history}"

# Find all markdown files with front matter
for file in "$HISTORY_DIR"/*.md; do
  # Skip if not a file or is draft
  [[ ! -f "$file" ]] && continue

  # Extract front matter block
  front_matter=$(sed -n '/^---$/,/^---$/p' "$file" | sed '1d;$d')

  # Output: filepath|front_matter_yaml
  echo "$file|$front_matter"
done
```

## Usage Examples

### Example 1: Find Workflow-Related Changes

```
User: "Search history for workflow changes"
Claude: /history search area:workflows

Result:
Found 5 records related to workflows:
1. feat: Transform Release Workflow into GitHub Action
2. feat: Template Sync Workflow
3. Backport Improvements from Fork
...
```

### Example 2: Find Changes to Specific File

```
User: "What history do we have about changelog.ts?"
Claude: /history search files:src/changelog.ts

Result:
Found 2 records that modified src/changelog.ts:
1. feat: Transform Release Workflow into GitHub Action
   - Ported generate-changelog.mjs to TypeScript
   - Created changelog module with PR enrichment
2. ...
```

### Example 3: Find Related Work Before Starting

```
Claude (during planning): "Let me search for relevant history..."
/history search files:src/github.ts area:core component:github-api

Result:
Found 1 relevant record:
1. feat: Transform Release Workflow into GitHub Action
   - Created src/github.ts module
   - Implemented GitHub Release creation
   - Added PR enrichment for changelogs
```

### Example 4: Combination Query

```
User: "Find completed features in workflows"
Claude: /history search type:feat area:workflows status:complete

Result:
Found 3 records:
1. feat: Transform Release Workflow into GitHub Action
2. feat: Template Sync Workflow
3. feat: Automated Tag Sync
```

## Integration with History-Driven Workflow

The history-driven-workflow skill uses this skill internally during Phase 1: Planning.

**Automatic search trigger:**
1. User enters plan mode with a task description
2. Workflow extracts key information (files mentioned, components, areas)
3. Constructs search query automatically
4. Runs history search to find relevant records
5. Presents findings: "I found 3 relevant records that might inform this work..."
6. Optionally loads full records if highly relevant

**Configuration in CLAUDE.md:**

```markdown
## History-Driven Workflow Configuration

| Setting                     | Value      |
| --------------------------- | ---------- |
| **History folder**          | `./history/` |
| **Auto-search on planning** | `true`     |
| **Search depth**            | `summary`  |
| **Min relevance score**     | `2`        |
```

## Error Handling

### No Results Found

```
No history records found matching your criteria.

Try:
- Broadening your search (e.g., area instead of specific files)
- Checking file paths are correct
- Searching by component or type instead
- Using /history list to see all available records
```

### Invalid Query Syntax

```
Invalid search query: "files:src/ AND area:core"

Valid syntax:
  files:path/to/file.ts
  area:workflows,scripts
  type:feat status:complete

Combine criteria with spaces (AND logic):
  files:src/ area:core type:feat
```

### No Front Matter Available

```
Warning: Some history records don't have front matter yet.
Run the migration utility to add front matter to all records:
  pnpm migrate-history
```

## Additional Commands

### List All Records

```bash
/history list
```

Shows all history records with basic metadata.

### Show Record Details

```bash
/history show 2026-01-30-release-action.md
```

Loads and displays the complete record.

### Statistics

```bash
/history stats
```

Shows statistics:
- Total records
- Records by type (feat, fix, docs, etc.)
- Records by area
- Records by status
- Most commonly changed files

## Performance Considerations

- **Fast**: grep + sed extracts only front matter, not entire files
- **Scalable**: Works efficiently with hundreds of history records
- **Cached**: Can cache extracted front matter for repeated searches
- **Lazy loading**: Only loads full record content when explicitly needed

## See Also

- **history-driven-workflow**: Main workflow skill that uses this search capability
- **CLAUDE.md**: Project configuration for search behavior
- **history/ folder**: Where all history records are stored