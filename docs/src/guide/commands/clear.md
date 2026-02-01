# clear

Discard pending attribution changes without committing.

## Usage

```bash
whogitit clear
```

## Description

The `clear` command removes the pending buffer file (`.whogitit-pending.json`) without creating a commit. This discards all captured AI attribution data from the current session.

## When to Use

Use `clear` when you want to:

- **Abandon an AI-assisted session** - You've decided not to commit the AI-generated changes
- **Start fresh** - Reset the attribution state before a new session
- **Fix a stale buffer** - The pending buffer is outdated or corrupted
- **Testing** - Clear state during development/testing

## What Gets Cleared

The command removes:

- All captured file snapshots
- Session metadata (session ID, model, timestamps)
- Prompt history for the session
- File edit histories

## Examples

### Discard pending changes

```bash
# Check what's pending
whogitit status

# Discard everything
whogitit clear
```

### After a git reset

If you've reset your git state and the pending buffer is now stale:

```bash
git reset --hard HEAD~1
whogitit clear
```

## Relationship to Git

The `clear` command only affects whogitit's pending buffer. It does **not**:

- Modify any files in your working directory
- Affect git's staging area or commit history
- Remove any existing git notes

To discard both git changes and whogitit attribution:

```bash
git checkout -- .           # Discard file changes
whogitit clear              # Discard attribution
```

## Pending Buffer

The pending buffer is stored at `.whogitit-pending.json` in your repository root. This file:

- Is created automatically during Claude Code sessions
- Should be in your `.gitignore`
- Is cleared after successful commits (by the post-commit hook)
- Can be manually inspected for debugging

## See Also

- [status](./status.md) - View pending changes before clearing
- [post-commit](../../reference/hooks.md) - How attribution is finalized
