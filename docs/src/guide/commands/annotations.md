# annotations

Generate annotations for GitHub Checks API integration.

## Usage

```bash
whogitit annotations [OPTIONS]
```

## Description

The `annotations` command generates annotation data suitable for the GitHub Checks API. This is primarily used in CI/CD pipelines to display AI attribution information directly in the GitHub PR diff view.

Annotations appear as colored markers in the "Files changed" tab, showing which lines were AI-generated and the prompts used.

## Options

### Core Options

| Option | Description |
|--------|-------------|
| `--base <COMMIT>` | Base commit (exclusive). Defaults to first commit if not specified |
| `--head <COMMIT>` | Head commit (inclusive). Default: `HEAD` |
| `--format <FORMAT>` | Output format: `github-checks` (default) or `json` |
| `--max-annotations <N>` | Maximum annotations to output. Default: `50` (GitHub API limit) |
| `--ai-only` | Only annotate pure AI lines (not AI-modified) |

### Consolidation Options

| Option | Description |
|--------|-------------|
| `--consolidate <MODE>` | Consolidation mode: `auto` (default), `file`, or `lines` |
| `--consolidate-threshold <THRESHOLD>` | AI coverage threshold for auto mode (0.0-1.0). Default: `0.7` |
| `--consolidate-prompt-limit <N>` | Max prompts for auto-consolidation. Default: `3` |
| `--min-lines <N>` | Minimum AI lines to create a line-level annotation. Default: `1` |

### Filtering Options

| Option | Description |
|--------|-------------|
| `--min-ai-lines <N>` | Minimum AI lines for a file to be annotated. Default: `3` |
| `--min-ai-percent <N>` | Minimum AI percentage for a file to be annotated (0.0-100.0). Default: `5.0` |
| `--diff-only` | Only annotate lines within the PR diff (requires `--base`) |

### Grouping and Sorting Options

| Option | Description |
|--------|-------------|
| `--group-ai-types` | Group AI and AIModified lines together in annotations |
| `--sort-by <MODE>` | Sort files by: `coverage` (default), `lines`, or `alpha` |

## Consolidation Modes

### Auto (default)

Smart consolidation that chooses between file-level and line-level annotations based on AI coverage:

- **File-level annotation** when:
  - File is new (no original lines), OR
  - AI coverage >= threshold (default 70%) AND prompts <= prompt limit (default 3)

- **Line-level annotations** otherwise

### File

Creates one annotation per file, summarizing all AI attribution.

```bash
whogitit annotations --consolidate file
```

### Lines

Creates granular line-level annotations, grouping consecutive AI lines from the same prompt.

```bash
whogitit annotations --consolidate lines
```

## Annotation Prioritization

When more annotations are generated than `--max-annotations` allows, annotations are scored and prioritized by:

1. **AI coverage** (up to 40 points) - Higher AI percentage scores higher
2. **AI line count** (up to 30 points) - More AI lines score higher (capped at 100)
3. **New file bonus** (15 points) - Files created entirely by AI
4. **In-diff bonus** (15 points) - Lines within the PR diff

The highest-scoring annotations are kept.

## Examples

### Basic usage (for GitHub Actions)

```bash
whogitit annotations --base ${{ github.event.pull_request.base.sha }} --head ${{ github.sha }}
```

### JSON output for debugging

```bash
whogitit annotations --format json
```

### File-level consolidation for cleaner diffs

```bash
whogitit annotations --consolidate file --base main
```

### Custom threshold and prompt limit

```bash
# Consolidate at 50%+ AI coverage with up to 5 prompts
whogitit annotations --consolidate-threshold 0.5 --consolidate-prompt-limit 5
```

### Filter to significant files only

```bash
# Only annotate files with at least 10 AI lines and 20% AI coverage
whogitit annotations --min-ai-lines 10 --min-ai-percent 20.0
```

### Annotate only lines in the PR diff

```bash
whogitit annotations --base main --diff-only
```

### Group AI and AI-modified lines together

```bash
# Creates fewer, larger annotations
whogitit annotations --group-ai-types
```

### Sort by AI line count instead of coverage

```bash
whogitit annotations --sort-by lines
```

## Output Format

### GitHub Checks Format

```json
{
  "annotations": [
    {
      "path": "src/main.rs",
      "start_line": 42,
      "end_line": 48,
      "annotation_level": "notice",
      "title": "AI Generated (7 lines)",
      "message": "Model: claude-opus-4-5-20251101 | Session: 2024-01-15\n\n**Breakdown:** 5 AI, 2 AI-modified, 0 human, 0 original\n\n**Prompt:** Add error handling with retry logic...",
      "raw_details": "Add error handling with retry logic..."
    }
  ],
  "summary": {
    "files_analyzed": 5,
    "models": ["claude-opus-4-5-20251101", "claude-sonnet-4-20250514"],
    "session_range": "2024-01-15 to 2024-01-20"
  }
}
```

### JSON Format

Same structure but without GitHub-specific formatting constraints.

## Shallow Clone Handling

When running in a shallow clone (common in CI), the command automatically:
- Detects the shallow clone
- Switches to file-level consolidation mode
- Displays a warning message

This ensures annotations work even with limited git history.

## GitHub Actions Integration

See the [CI/CD Integration](../../workflows/ci-cd.md) guide for complete GitHub Actions workflow examples.

```yaml
- name: Generate annotations
  run: |
    ANNOTATIONS=$(whogitit annotations \
      --base ${{ github.event.pull_request.base.sha }} \
      --min-ai-lines 5 \
      --sort-by coverage)
    echo "$ANNOTATIONS" > annotations.json

- name: Create Check Run
  uses: actions/github-script@v7
  with:
    script: |
      const fs = require('fs');
      const output = JSON.parse(fs.readFileSync('annotations.json'));
      // ... create check run with output.annotations
```

## See Also

- [summary](./summary.md) - PR summary in Markdown format
- [CI/CD Integration](../../workflows/ci-cd.md) - Full workflow examples
