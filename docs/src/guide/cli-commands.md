# CLI Commands

whogitit provides a comprehensive CLI for viewing and managing AI attribution data.

## Command Overview

| Command | Description |
|---------|-------------|
| [`blame`](./commands/blame.md) | Show AI attribution for each line of a file |
| [`show`](./commands/show.md) | View attribution summary for a commit |
| [`prompt`](./commands/prompt.md) | View the prompt that generated specific lines |
| [`summary`](./commands/summary.md) | Generate summary for a commit range (PRs) |
| [`status`](./commands/status.md) | Check pending attribution changes |
| [`export`](./commands/export.md) | Export attribution data as JSON/CSV |
| [`retention`](./commands/retention.md) | Manage data retention policies |
| [`audit`](./commands/audit.md) | View the audit log |
| `init` | Initialize whogitit in a repository |
| `clear` | Discard pending changes without committing |

## Quick Reference

### Viewing Attribution

```bash
# See AI attribution for a file
whogitit blame src/main.rs

# Show only AI-generated lines
whogitit blame src/main.rs --ai-only

# View commit summary
whogitit show HEAD

# Find prompt that generated a line
whogitit prompt src/main.rs:42
```

### Managing Data

```bash
# Check pending changes
whogitit status

# Clear pending without committing
whogitit clear

# Export all attribution data
whogitit export -o attribution.json

# Preview retention policy
whogitit retention preview
```

### Setup Commands

```bash
# Initialize repository
whogitit init

# Test redaction patterns
whogitit redact-test "api_key=secret123"
```

## Global Options

All commands support these options:

| Option | Description |
|--------|-------------|
| `--help` | Show help for any command |
| `--version` | Show version information |

## Output Formats

Many commands support multiple output formats:

| Format | Flag | Use Case |
|--------|------|----------|
| Pretty | (default) | Human-readable terminal output |
| JSON | `--format json` | Machine parsing, scripts |
| Markdown | `--format markdown` | Documentation, PR comments |
| CSV | `--format csv` | Spreadsheets, data analysis |

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Invalid arguments |

## Next Steps

Explore each command in detail:

- [blame](./commands/blame.md) - Line-level attribution
- [show](./commands/show.md) - Commit summaries
- [prompt](./commands/prompt.md) - Prompt lookup
- [summary](./commands/summary.md) - PR summaries
- [export](./commands/export.md) - Data export
