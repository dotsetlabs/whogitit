# summary

Generate an AI attribution summary for a range of commits, ideal for pull requests.

## Synopsis

```bash
whogitit summary [OPTIONS]
```

## Description

The `summary` command aggregates AI attribution data across multiple commits, producing a comprehensive report suitable for pull request descriptions or compliance documentation.

The output focuses on **additions** (lines added in the commit range), making it directly comparable to what you see in a PR diff.

## Options

| Option | Description |
|--------|-------------|
| `--base <REF>` | Base branch/commit to compare against (default: first commit in repo) |
| `--head <REF>` | Head branch/commit (default: `HEAD`) |
| `--format <FORMAT>` | Output format: `pretty` (default), `json`, `markdown` |

## Examples

### Basic Usage

```bash
whogitit summary --base main
```

Output:

```text
AI Attribution Summary
======================

Commits analyzed: 5 (3 with AI attribution)

Lines Added:
  +145 AI-generated (72.5%)
  +12 AI-modified by human (6.0%)
  +43 Human-written (21.5%)
  +200 Total additions

AI involvement: 78.5% of additions are AI-generated

Files Changed:
  src/auth.rs +80 (90% AI) (new)
  src/main.rs +45 (70% AI)
  src/jwt.rs +75 (80% AI) (new)

Models used:
  - claude-opus-4-5-20251101
```

### Markdown Output (for PRs)

```bash
whogitit summary --base main --format markdown
```

Output:

```markdown
## 🤖🤖 AI Attribution Summary

This PR adds **+200** lines with AI attribution across **3** files.

### Additions Breakdown

| Metric | Lines | % of Additions |
|--------|------:|--------------:|
| 🟢 AI-generated | +145 | 72.5% |
| 🟡 AI-modified by human | +12 | 6.0% |
| 🔵 Human-written | +43 | 21.5% |
| **Total additions** | **+200** | **100%** |

**AI involvement: 78.5%** of additions are AI-generated

### Files Changed

| File | +Added | AI | Human | AI % | Status |
|------|-------:|---:|------:|-----:|--------|
| `src/auth.rs` | +80 | 72 | 8 | 90% | New |
| `src/main.rs` | +45 | 32 | 13 | 71% | Modified |
| `src/jwt.rs` | +75 | 53 | 22 | 71% | New |

### Models Used

- claude-opus-4-5-20251101
```

### JSON Output

```bash
whogitit summary --base main --format json
```

```json
{
  "schema_version": 1,
  "schema": "whogitit.summary.v1",
  "commits_analyzed": 5,
  "commits_with_ai": 3,
  "additions": {
    "total": 200,
    "ai": 145,
    "ai_modified": 12,
    "human": 43
  },
  "ai_percentage": 78.5,
  "files": [
    {
      "path": "src/auth.rs",
      "additions": 80,
      "ai_additions": 72,
      "ai_lines": 70,
      "ai_modified_lines": 2,
      "human_lines": 8,
      "ai_percent": 90.0,
      "is_new_file": true
    },
    ...
  ],
  "models": ["claude-opus-4-5-20251101"]
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

### Additions Breakdown

The summary focuses on lines **added** in the commit range:

| Metric | Description |
|--------|-------------|
| AI-generated | Lines written by AI, unchanged at commit time |
| AI-modified by human | Lines written by AI, then edited before commit |
| Human-written | Lines added by humans (not from AI) |
| Total additions | Sum of all added lines (maps to `+` in git diff) |
| AI involvement | (AI + AI-modified) / Total additions × 100% |

### Files Changed

Per-file breakdown showing:
- File path
- Lines added (`+Added`)
- AI contributions (AI + AI-modified)
- Human contributions
- AI percentage for that file
- Status: "New" (file created) or "Modified" (file existed)

### Why Diff-Focused?

Previous versions showed "Original/unchanged" lines, which included lines that existed before the PR. This was confusing because:
- It didn't map to what reviewers see in the PR diff
- Percentages didn't reflect the actual changes being reviewed
- Large existing files would dilute the AI percentage

The new format shows only additions, making it directly comparable to `git diff --stat`.

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
- The output focuses on additions only; deleted lines are not attributed

## See Also

- [show](./show.md) - Single commit attribution
- [export](./export.md) - Bulk data export
- [CI/CD Integration](../../workflows/ci-cd.md) - Automated PR comments
