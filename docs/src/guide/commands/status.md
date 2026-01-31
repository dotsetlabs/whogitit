# status

Check pending AI attribution changes before committing.

## Synopsis

```bash
whogitit status
```

## Description

The `status` command shows the current state of the pending buffer, which accumulates AI attribution data during your Claude Code session. This helps you understand what will be attached to your next commit.

## Examples

### With Pending Changes

```bash
whogitit status
```

Output:

```
Pending AI attribution:
  Session: 7f3a-4b2c-9d1e-8a7b
  Files: 3
  Edits: 7
  Lines: 145
  Age: 2 hours ago

Run 'git commit' to finalize attribution.
```

### No Pending Changes

```bash
whogitit status
```

Output:

```
No pending AI attribution.
```

### Stale Pending Buffer

If the pending buffer is older than 24 hours, a warning is displayed:

```
Pending AI attribution:
  Session: 7f3a-4b2c-9d1e-8a7b
  Files: 3
  Edits: 7
  Lines: 145
  Age: 2 days ago

⚠️  Warning: This pending buffer is stale (> 24 hours old).
   Run 'whogitit clear' if these changes are no longer relevant.
```

## Output Details

| Field | Description |
|-------|-------------|
| Session | The AI session ID |
| Files | Number of files with captured changes |
| Edits | Total number of edit operations captured |
| Lines | Approximate lines affected |
| Age | How long since the first capture |

## Use Cases

### Pre-Commit Check

Before committing, verify what attribution data will be attached:

```bash
whogitit status
git status
git commit -m "Add new feature"
```

### Debugging Hook Issues

If attribution isn't being captured, check if any data is pending:

```bash
# Make an edit with Claude Code
# Then check status
whogitit status

# If empty, hooks may not be configured correctly
```

### Clearing Stale Data

If you've been working but don't want to commit the AI attribution:

```bash
whogitit status
# Shows stale pending data

whogitit clear
# Clears the pending buffer

whogitit status
# No pending AI attribution.
```

## Related Commands

### clear

Discard pending changes without committing:

```bash
whogitit clear
```

This removes the `.whogitit-pending.json` file, discarding all captured attribution data for the current session.

### init

If `status` shows issues, reinitialize:

```bash
whogitit init
```

This reinstalls hooks and reconfigures git.

## Notes

- The pending buffer is stored in `.whogitit-pending.json` in your repository root
- This file is typically in `.gitignore` and should not be committed
- The pending buffer is automatically cleared after a successful commit

## See Also

- [Quick Start](../../getting-started/quick-start.md) - Basic workflow
- [Hook System](../../reference/hooks.md) - How capture works
