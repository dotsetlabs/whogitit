# show

View AI attribution summary for a specific commit.

## Synopsis

```bash
whogitit show [OPTIONS] [COMMIT]
```

## Description

The `show` command displays a detailed summary of AI attribution for a commit, including:
- Session information (ID, model, timestamp)
- All prompts used
- Per-file breakdown of attribution
- Overall statistics

## Arguments

| Argument | Description |
|----------|-------------|
| `[COMMIT]` | Git commit reference (default: HEAD) |

## Options

| Option | Description |
|--------|-------------|
| `--format <FORMAT>` | Output format: `pretty` (default), `json`, `markdown` |

## Examples

### Basic Usage

```bash
whogitit show HEAD
```

Output:

```
Commit: d4e5f6g
Session: 7f3a-4b2c-9d1e-8a7b
Model: claude-opus-4-5-20251101
Started: 2026-01-30T14:23:17Z

Prompts used:
  #0: "Add error handling with anyhow..."
  #1: "Implement the serialize trait..."

Files with AI changes:
  src/auth.rs (25 AI, 3 modified, 2 human) - 45 total lines
  src/main.rs (10 AI, 5 original) - 15 total lines

Summary:
  35 AI-generated lines
  3 AI lines modified by human
  2 human-added lines
  5 original/unchanged lines
```

### Show a Specific Commit

```bash
whogitit show abc1234
```

### JSON Output

```bash
whogitit show --format json HEAD
```

Output:

```json
{
  "commit": "d4e5f6gabcdef1234567890",
  "session": {
    "session_id": "7f3a-4b2c-9d1e-8a7b",
    "model": {
      "id": "claude-opus-4-5-20251101",
      "provider": "anthropic"
    },
    "started_at": "2026-01-30T14:23:17Z"
  },
  "prompts": [
    {
      "index": 0,
      "text": "Add error handling with anyhow...",
      "affected_files": ["src/auth.rs", "src/main.rs"]
    }
  ],
  "files": [
    {
      "path": "src/auth.rs",
      "ai_lines": 25,
      "ai_modified_lines": 3,
      "human_lines": 2,
      "original_lines": 15,
      "total_lines": 45
    }
  ],
  "summary": {
    "total_ai_lines": 35,
    "total_ai_modified_lines": 3,
    "total_human_lines": 2,
    "total_original_lines": 5
  }
}
```

### Markdown Output

```bash
whogitit show --format markdown HEAD
```

Useful for including in documentation or PR descriptions.

## Output Details

### Session Information

| Field | Description |
|-------|-------------|
| Session | Unique identifier for the AI session |
| Model | AI model used (e.g., claude-opus-4-5-20251101) |
| Started | When the session began |

### Prompts Section

Lists all prompts used during the session, with:
- Index number (for reference with `whogitit prompt`)
- Truncated prompt text
- Files affected by each prompt

### Files Section

For each file with AI changes:
- File path
- Line counts by attribution type
- Total lines

### Summary Section

Aggregate statistics across all files:
- Total AI-generated lines
- Total AI-modified lines
- Total human-added lines
- Total original lines

## Notes

- If a commit has no AI attribution, the command will indicate this
- Prompts are shown truncated; use `whogitit prompt` for full text
- The commit can be specified as SHA, branch name, tag, or any git revision

## See Also

- [blame](./blame.md) - Line-level attribution for a file
- [prompt](./prompt.md) - View full prompt text
- [summary](./summary.md) - Attribution for a range of commits
