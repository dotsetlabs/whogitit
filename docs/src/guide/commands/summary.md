# summary

Generate an AI attribution summary for a range of commits, ideal for pull requests.

## Synopsis

```bash
whogitit summary [OPTIONS]
```

## Description

The `summary` command aggregates AI attribution data across multiple commits, producing a comprehensive report suitable for pull request descriptions or compliance documentation.

## Options

| Option | Description |
|--------|-------------|
| `--base <REF>` | Base branch/commit to compare against (default: `main`) |
| `--head <REF>` | Head branch/commit (default: `HEAD`) |
| `--format <FORMAT>` | Output format: `pretty` (default), `json`, `markdown` |

## Examples

### Basic Usage

```bash
whogitit summary --base main
```

Output:

```
AI Attribution Summary
======================

Commits analyzed: 5 (3 with AI attribution)

Overview:
  AI-generated lines:     145 (58.0%)
  AI-modified by human:    12 (4.8%)
  Human-added lines:       43 (17.2%)
  Original/unchanged:      50 (20.0%)
  ─────────────────────────────────
  Total:                  250

AI involvement: 62.8% of changed lines

Commits with AI Attribution:
  abc1234 Add user authentication     (45 AI, 3 mod, 10 human)
  def5678 Implement JWT tokens        (100 AI, 9 mod, 33 human)
  ghi9012 Add password hashing        (0 AI - no attribution)

Prompts Used: 5
  #0: "Add user authentication with bcrypt..."
  #1: "Create a User struct with email and..."
  #2: "Implement JWT token generation..."
```

### Markdown Output (for PRs)

```bash
whogitit summary --base main --format markdown
```

Output:

```markdown
## AI Attribution Summary

This PR contains **3** of **5** commits with AI-assisted changes.

### Overview

| Metric | Lines | Percentage |
|--------|------:|----------:|
| AI-generated | 145 | 58.0% |
| AI-modified by human | 12 | 4.8% |
| Human-added | 43 | 17.2% |
| Original/unchanged | 50 | 20.0% |
| **Total** | **250** | **100%** |

**AI involvement: 62.8%** of changed lines

### Commits with AI Attribution

| Commit | Message | AI | Modified | Human |
|--------|---------|---:|--------:|------:|
| `abc1234` | Add user authentication | 45 | 3 | 10 |
| `def5678` | Implement JWT tokens | 100 | 9 | 33 |

### Prompts Used (5)

<details>
<summary>Add user authentication with bcrypt...</summary>

Add user authentication with bcrypt password hashing. Create a User struct
with email and password_hash fields.
</details>
```

### JSON Output

```bash
whogitit summary --base main --format json
```

```json
{
  "base": "main",
  "head": "HEAD",
  "commits": {
    "total": 5,
    "with_attribution": 3
  },
  "summary": {
    "ai_lines": 145,
    "ai_modified_lines": 12,
    "human_lines": 43,
    "original_lines": 50,
    "total_lines": 250,
    "ai_percentage": 62.8
  },
  "commit_details": [...],
  "prompts": [...]
}
```

### Custom Range

```bash
# Compare feature branch to develop
whogitit summary --base develop --head feature/auth

# Specific commit range
whogitit summary --base abc1234 --head def5678
```

## Output Details

### Overview Section

Aggregate statistics for all commits in the range:

| Metric | Description |
|--------|-------------|
| AI-generated lines | Lines written by AI, unchanged |
| AI-modified by human | Lines written by AI, then edited |
| Human-added lines | Lines written by humans (after AI session) |
| Original/unchanged | Lines that existed before AI sessions |
| AI involvement | (AI + AI-modified) / (AI + AI-modified + Human) |

### Commits Section

Per-commit breakdown showing:
- Commit SHA (short)
- Commit message (first line)
- Line counts by category

### Prompts Section

All unique prompts used across the commit range, with:
- Prompt text (truncated in pretty format, full in markdown)
- Files affected

## Use Cases

### Pull Request Descriptions

Generate markdown summary to paste into PR description:

```bash
whogitit summary --base main --format markdown | pbcopy
```

### CI/CD Integration

The GitHub Action uses this command to generate PR comments automatically. See [CI/CD Integration](../../workflows/ci-cd.md).

### Compliance Reporting

Export JSON for compliance documentation:

```bash
whogitit summary --base main --format json > pr-attribution.json
```

## Notes

- Commits without AI attribution are counted but contribute 0 to AI metrics
- The `--base` ref should be an ancestor of `--head`
- Empty commit ranges produce a summary with all zeros

## See Also

- [show](./show.md) - Single commit attribution
- [export](./export.md) - Bulk data export
- [CI/CD Integration](../../workflows/ci-cd.md) - Automated PR comments
