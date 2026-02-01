# Git Integration

whogitit provides deep integration with git to surface AI attribution data directly in your terminal workflow.

## AI-Annotated Diff Pager

The `pager` command reads git diff output from stdin and annotates it with AI attribution markers.

### Quick Setup

Configure git to use whogitit as the diff pager:

```bash
# Option 1: Replace the default pager for all git output
git config --global core.pager "whogitit pager"

# Option 2: Create an alias for AI-annotated diffs
git config --global alias.ai-diff '!git diff | whogitit pager --no-pager'
git config --global alias.ai-show '!git show | whogitit pager --no-pager'
```

### Usage

With the pager configured, run any git diff command:

```bash
git diff HEAD~1           # Annotated diff
git diff main..feature    # Compare branches with attribution
git show abc123           # Show commit with AI markers
```

### Output Format

Added lines are annotated with AI attribution markers:

```diff
diff --git a/src/main.rs b/src/main.rs
@@ -40,4 +45,8 @@ impl Server {
● +    fn handle_error(e: Error) -> Result<()> {  # AI
● +        log::error!("Failed: {}", e);
● +        retry_with_backoff(|| reconnect())
● +    }
+ +    // Added manual timeout check  # AI-mod
```

**Markers:**
- `●` (green) - AI-generated line, unchanged
- `◐` (yellow) - AI-generated line, modified by human
- No marker - Human-written or original lines

### Options

```bash
whogitit pager [OPTIONS]

Options:
  --no-color     Disable colored output
  -v, --verbose  Show detailed attribution (edit ID, similarity %)
  --no-pager     Output directly to stdout instead of through pager
```

### Examples

```bash
# View diff with verbose attribution
git diff | whogitit pager --verbose

# Pipe to less manually
git diff | whogitit pager --no-pager | less -R

# Combine with git log
git log -p | whogitit pager
```

## Git Aliases

Here are some useful git aliases for working with whogitit:

```bash
# AI-aware diff commands
git config --global alias.ai-diff '!git diff | whogitit pager --no-pager'
git config --global alias.ai-show '!git show "$@" | whogitit pager --no-pager'
git config --global alias.ai-log '!git log -p "$@" | whogitit pager --no-pager'

# Quick blame with AI attribution
git config --global alias.ai-blame '!whogitit blame'

# Show AI summary for branch
git config --global alias.ai-summary '!whogitit summary --base main'
```

## Fetching Attribution Notes

whogitit stores attribution data in git notes. To ensure notes are fetched:

```bash
# One-time setup: Configure automatic note fetching
git config --global --add remote.origin.fetch '+refs/notes/whogitit:refs/notes/whogitit'

# Or fetch notes manually
git fetch origin refs/notes/whogitit:refs/notes/whogitit
```

The `whogitit init` command automatically configures this for your repository.

## Pushing Attribution Notes

Notes are automatically pushed when you run `git push` if you've run `whogitit init`. Otherwise:

```bash
# Push notes manually
git push origin refs/notes/whogitit
```

## Integration with Git Hooks

whogitit installs two hooks via `whogitit init`:

1. **post-commit**: Automatically attaches attribution to commits
2. **pre-push**: Automatically pushes notes with regular pushes

These hooks are idempotent and can be safely re-run.

## Troubleshooting

### Pager not showing annotations

1. Check if attribution data exists:
   ```bash
   whogitit blame <file>
   ```

2. Ensure notes are fetched:
   ```bash
   git fetch origin refs/notes/whogitit:refs/notes/whogitit
   ```

3. Verify whogitit is in PATH:
   ```bash
   which whogitit
   ```

### Colors not working

If colors don't appear in your pager, try:

```bash
# Use -R flag with less
git diff | whogitit pager --no-pager | less -R

# Or disable colors
git diff | whogitit pager --no-color
```

### Notes not pushing

Check if the pre-push hook is installed:

```bash
cat .git/hooks/pre-push | grep whogitit
```

If not, run `whogitit init` again.
