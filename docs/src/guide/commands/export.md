# export

Export AI attribution data for multiple commits in JSON or CSV format.

## Synopsis

```bash
whogitit export [OPTIONS]
```

## Description

The `export` command extracts attribution data from git notes and outputs it in a structured format suitable for analysis, reporting, or archival purposes.

## Options

| Option | Description |
|--------|-------------|
| `--format <FORMAT>` | Output format: `json` (default), `csv` |
| `--since <DATE>` | Only include commits on or after this date (YYYY-MM-DD) |
| `--until <DATE>` | Only include commits on or before this date (YYYY-MM-DD) |
| `-o, --output <FILE>` | Output file (default: stdout) |
| `--full-prompts` | Include full prompt text (default: truncated to 100 chars) |
| `--prompt-max-len <N>` | Max prompt length when not using --full-prompts (default: 100) |

## Examples

### Basic JSON Export

```bash
whogitit export
```

Output (to stdout):

```json
{
  "export_version": 1,
  "exported_at": "2026-01-30T15:00:00Z",
  "date_range": null,
  "commits": [
    {
      "commit_id": "abc123def456...",
      "commit_short": "abc123d",
      "message": "Add user authentication",
      "author": "Greg King",
      "committed_at": "2026-01-30T14:30:00Z",
      "session_id": "7f3a-4b2c-9d1e-8a7b",
      "model": "claude-opus-4-5-20251101",
      "ai_lines": 145,
      "ai_modified_lines": 12,
      "human_lines": 43,
      "original_lines": 50,
      "files": ["src/auth.rs", "src/main.rs"],
      "prompts": [
        {
          "index": 0,
          "text": "Add user authentication with bcrypt...",
          "affected_files": ["src/auth.rs"]
        }
      ]
    }
  ],
  "summary": {
    "total_commits": 10,
    "commits_with_ai": 7,
    "total_ai_lines": 523,
    "total_ai_modified_lines": 45,
    "total_human_lines": 128,
    "total_original_lines": 89,
    "total_prompts": 15
  }
}
```

### Export to File

```bash
whogitit export -o attribution-data.json
```

Output:

```text
Exported 10 commits to attribution-data.json
```

### CSV Format

```bash
whogitit export --format csv -o attribution.csv
```

CSV columns:
- `commit_id`
- `commit_short`
- `message`
- `author`
- `committed_at`
- `session_id`
- `model`
- `ai_lines`
- `ai_modified_lines`
- `human_lines`
- `original_lines`
- `files_count`
- `prompts_count`

### Date Filtering

```bash
# Last 30 days
whogitit export --since 2026-01-01

# Specific range
whogitit export --since 2026-01-01 --until 2026-01-31

# Q4 2025
whogitit export --since 2025-10-01 --until 2025-12-31 -o q4-2025.json
```

### Full Prompts

```bash
# Include complete prompt text
whogitit export --full-prompts -o full-export.json

# Custom truncation length
whogitit export --prompt-max-len 200 -o export.json
```

## Output Details

### JSON Schema

```text
{
  export_version: number,      // Schema version (currently 1)
  exported_at: string,         // ISO 8601 timestamp
  date_range: {                // null if no date filter
    since: string | null,
    until: string | null
  },
  commits: [CommitExport],     // Array of commit data
  summary: ExportSummary       // Aggregate statistics
}
```

### Summary Statistics

| Field | Description |
|-------|-------------|
| `total_commits` | Number of commits with attribution in range |
| `commits_with_ai` | Commits that have AI-generated lines |
| `total_ai_lines` | Sum of AI lines across all commits |
| `total_ai_modified_lines` | Sum of AI-modified lines |
| `total_human_lines` | Sum of human-added lines |
| `total_original_lines` | Sum of original lines |
| `total_prompts` | Total number of prompts used |

## Use Cases

### Compliance Reporting

Generate monthly AI usage reports:

```bash
whogitit export \
  --since 2026-01-01 \
  --until 2026-01-31 \
  --full-prompts \
  -o january-2026-ai-report.json
```

### Data Analysis

Export to CSV for spreadsheet analysis:

```bash
whogitit export --format csv -o data.csv
```

Then open in Excel, Google Sheets, or analyze with pandas.

### Backup/Archival

Periodically export all attribution data:

```bash
whogitit export --full-prompts -o "backup-$(date +%Y%m%d).json"
```

### CI Integration

Export as part of release process:

```bash
# In CI pipeline
whogitit export --since "$LAST_RELEASE_DATE" -o release-attribution.json
```

## Notes

- Only commits with attribution in git notes are included
- Commits are sorted by date (newest first)
- Date filters are inclusive at day boundaries (`--since` starts at `00:00:00`, `--until` ends at `23:59:59`)
- Prompts are redacted according to privacy settings
- Large exports may take time; consider date filtering

## See Also

- [summary](./summary.md) - Quick summary without full data
- [retention](./retention.md) - Managing data lifecycle
- [Privacy & Redaction](../privacy.md) - Understanding redacted content
