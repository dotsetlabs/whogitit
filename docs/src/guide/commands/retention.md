# retention

Manage data retention policies for AI attribution data.

## Synopsis

```bash
whogitit retention <SUBCOMMAND>
```

## Description

The `retention` command helps you manage the lifecycle of AI attribution data, allowing you to preview and apply retention policies that automatically clean up old data while preserving important commits.

## Subcommands

| Subcommand | Description |
|------------|-------------|
| `config` | Show current retention settings |
| `preview` | Preview what would be deleted |
| `apply` | Apply retention policy (dry-run by default) |

## Subcommand: config

Show the current retention configuration:

```bash
whogitit retention config
```

Output:

```
Current Retention Configuration
==================================================
Config file: /path/to/repo/.whogitit.toml

max_age_days: 365
auto_purge: false
retain_refs: refs/heads/main
min_commits: 100

Example configuration:

# .whogitit.toml
[retention]
max_age_days = 365
auto_purge = false
retain_refs = ["refs/heads/main"]
min_commits = 100
```

## Subcommand: preview

Preview what would be deleted based on current policy:

```bash
whogitit retention preview
```

Output:

```
Retention Policy Preview
==================================================
Max age: 365 days
Retained refs: refs/heads/main
Min commits to keep: 100

● 95 commits to keep
● 12 commits to delete

Commits that would be deleted:
  abc1234 Fix typo in readme (2024-08-15) - would be deleted
  def5678 Update dependencies (2024-07-22) - would be deleted
  ...

Run 'whogitit retention apply --execute' to delete these.
```

## Subcommand: apply

Apply the retention policy:

```bash
# Dry-run (default)
whogitit retention apply

# Actually delete
whogitit retention apply --execute

# With reason for audit log
whogitit retention apply --execute --reason "Quarterly cleanup"
```

### Options

| Option | Description |
|--------|-------------|
| `--execute` | Actually delete (without this, does a dry-run) |
| `--reason <TEXT>` | Reason for deletion (recorded in audit log) |

### Dry-Run Output

```
Preview: 12 commits would be deleted (dry-run)
Run with --execute to actually delete.
```

### Execute Output

```
Done: Deleted attribution for 12 commits
Reason: Quarterly cleanup
```

## Configuration

Retention is configured in `.whogitit.toml`:

```toml
[retention]
# Delete attribution older than this many days
max_age_days = 365

# Automatically purge on each commit (default: false)
auto_purge = false

# Never delete attribution for commits reachable from these refs
retain_refs = ["refs/heads/main", "refs/heads/release"]

# Keep at least this many commits regardless of age
min_commits = 100
```

### Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `max_age_days` | number | none | Delete data older than N days |
| `auto_purge` | boolean | false | Purge automatically on commit |
| `retain_refs` | array | `["refs/heads/main"]` | Refs to always preserve |
| `min_commits` | number | 100 | Minimum commits to keep |

## Retention Logic

The retention policy applies these rules in order:

1. **Age filter**: Identify commits older than `max_age_days`
2. **Ref protection**: Exclude commits reachable from `retain_refs`
3. **Minimum guarantee**: Keep at least `min_commits` (most recent)

### Example Scenario

Configuration:
```toml
[retention]
max_age_days = 180
retain_refs = ["refs/heads/main"]
min_commits = 50
```

Repository has:
- 200 commits with attribution
- 80 commits older than 180 days
- 60 of those old commits are on main branch

Result:
- 60 old commits on main: **kept** (ref protection)
- 20 old commits not on main: **deleted**
- 120 recent commits: **kept** (within age limit)
- Final count: 180 commits kept, 20 deleted

## Use Cases

### Quarterly Cleanup

```bash
# Preview first
whogitit retention preview

# Apply with audit trail
whogitit retention apply --execute --reason "Q1 2026 cleanup"
```

### Compliance Requirements

If your organization requires data to be deleted after a certain period:

```toml
[retention]
max_age_days = 365  # 1 year retention
retain_refs = []    # No exceptions
min_commits = 0     # Delete all old data
```

### Preserve Release History

Keep attribution for all released versions:

```toml
[retention]
max_age_days = 90
retain_refs = [
  "refs/heads/main",
  "refs/tags/v1.0.0",
  "refs/tags/v2.0.0"
]
```

## Notes

- Deletion is **permanent** - there's no undo
- Deleted attribution is logged in the audit log (if enabled)
- The `--execute` flag is required to actually delete data
- Use `preview` liberally before applying

## See Also

- [Configuration](../configuration.md) - Full configuration reference
- [audit](./audit.md) - View deletion audit trail
- [export](./export.md) - Backup data before deletion
