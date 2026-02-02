# init

Initialize whogitit in a git repository.

## Usage

```bash
whogitit init [OPTIONS]
```

## Description

The `init` command sets up whogitit in the current git repository by installing git hooks and configuring note fetching.

This command should be run once per repository after the global `setup` command has been run.

## Options

| Option | Description |
|--------|-------------|
| `--force` | Skip global setup check and proceed anyway |

## What It Does

1. **Installs post-commit hook** (`.git/hooks/post-commit`)
   - Finalizes AI attribution after each commit
   - Attaches attribution data as git notes

2. **Installs pre-push hook** (`.git/hooks/pre-push`)
   - Automatically pushes git notes with regular pushes
   - Ensures attribution travels with your code

3. **Installs post-rewrite hook** (`.git/hooks/post-rewrite`)
   - Preserves attribution during `git rebase`
   - Preserves attribution during `git commit --amend`
   - Automatically copies notes from old to new commit SHAs

4. **Configures git fetch**
   - Adds fetch refspec for `refs/notes/whogitit`
   - Notes are automatically fetched on `git fetch`/`git pull`

5. **Updates git exclude**
   - Adds whogitit local artifacts to `.git/info/exclude`
   - Prevents accidental commits of `.whogitit-pending.json` and `.whogitit/`

## Examples

### Standard initialization

```bash
cd your-project
whogitit init
```

### Force initialization

Skip the global setup check (useful in CI or when you know what you're doing):

```bash
whogitit init --force
```

## Prerequisites

Before running `init`, you should:

1. Run `whogitit setup` (one-time global setup)
2. Be in a git repository

If global setup hasn't been run, `init` will warn you and suggest running `setup` first. Use `--force` to bypass this check.

## Hook Details

### post-commit hook

The post-commit hook:
- Runs `whogitit post-commit` after each commit
- Processes the pending buffer (`.whogitit-pending.json`)
- Creates git notes with attribution data
- Clears the pending buffer

### pre-push hook

The pre-push hook:
- Runs before each push
- Pushes `refs/notes/whogitit` to the remote
- Ensures attribution notes travel with commits

### post-rewrite hook

The post-rewrite hook:
- Runs after `git rebase` and `git commit --amend`
- Receives old→new SHA mappings on stdin
- Copies attribution notes to the new commits
- Reports how many notes were preserved

## Idempotency

The `init` command is idempotent - running it multiple times is safe. It will:
- Update hooks if they exist
- Add the fetch refspec if not already present
- Not duplicate any configuration

## Troubleshooting

### "Not in a git repository"

Ensure you're in a directory with a `.git` folder:

```bash
git status  # Should show repository status
```

### "Global setup not complete"

Run the global setup first:

```bash
whogitit setup
whogitit init
```

Or use `--force` if you want to skip the check.

### Hooks not running

Check hook permissions:

```bash
ls -la .git/hooks/post-commit
chmod +x .git/hooks/post-commit
```

## See Also

- [setup](./setup.md) - Global setup (run before init)
- [doctor](./doctor.md) - Diagnose configuration issues
- [Hook System](../../reference/hooks.md) - Technical hook details
