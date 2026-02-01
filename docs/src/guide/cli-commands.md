# CLI Commands

whogitit provides a comprehensive CLI for viewing and managing AI attribution data.

## Command Overview

### Core Attribution Commands

| Command | Description |
|---------|-------------|
| [`blame`](./commands/blame.md) | Show AI attribution for each line of a file |
| [`show`](./commands/show.md) | View attribution summary for a commit |
| [`prompt`](./commands/prompt.md) | View the prompt that generated specific lines |
| [`summary`](./commands/summary.md) | Generate summary for a commit range (PRs) |
| [`status`](./commands/status.md) | Check pending attribution changes |

### Developer Integration Commands

| Command | Description |
|---------|-------------|
| [`annotations`](./commands/annotations.md) | Generate GitHub Checks API annotations |
| [`pager`](./commands/pager.md) | Annotate git diff output with AI markers |

### Data Management Commands

| Command | Description |
|---------|-------------|
| [`export`](./commands/export.md) | Export attribution data as JSON/CSV |
| [`retention`](./commands/retention.md) | Manage data retention policies |
| [`audit`](./commands/audit.md) | View the audit log |
| [`clear`](./commands/clear.md) | Discard pending changes without committing |

### Setup Commands

| Command | Description |
|---------|-------------|
| [`setup`](./commands/setup.md) | Configure Claude Code integration (one-time) |
| [`doctor`](./commands/doctor.md) | Verify whogitit configuration |
| [`init`](./commands/init.md) | Initialize whogitit in a repository |

### Privacy Commands

| Command | Description |
|---------|-------------|
| [`redact-test`](./commands/redact-test.md) | Test redaction patterns against text/files |

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

# Summarize a PR
whogitit summary --base main --format markdown
```

### Developer Integration

```bash
# Generate GitHub Checks annotations for CI
whogitit annotations --base main --head HEAD

# Use as git pager for AI-annotated diffs
git config --global core.pager "whogitit pager"
git diff | whogitit pager

# Create git aliases
git config --global alias.ai-diff '!git diff | whogitit pager --no-pager'
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
# One-time global setup (configures Claude Code)
whogitit setup

# Verify all configuration
whogitit doctor

# Initialize repository hooks
whogitit init

# Initialize even if global setup incomplete
whogitit init --force
```

### Privacy Testing

```bash
# Test redaction patterns
whogitit redact-test --text "api_key=secret123"

# List available patterns
whogitit redact-test --list-patterns

# Show what would be redacted
whogitit redact-test --file .env --matches-only
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

### Core Commands
- [blame](./commands/blame.md) - Line-level attribution
- [show](./commands/show.md) - Commit summaries
- [prompt](./commands/prompt.md) - Prompt lookup
- [summary](./commands/summary.md) - PR summaries

### Developer Integration
- [annotations](./commands/annotations.md) - GitHub Checks API
- [pager](./commands/pager.md) - Git diff annotations

### Data & Privacy
- [export](./commands/export.md) - Data export
- [retention](./commands/retention.md) - Data retention
- [audit](./commands/audit.md) - Audit log
- [redact-test](./commands/redact-test.md) - Privacy testing

### Setup
- [setup](./commands/setup.md) - Global configuration
- [doctor](./commands/doctor.md) - Configuration check
- [init](./commands/init.md) - Repository setup
