# Troubleshooting

Common issues and their solutions.

> **Tip:** Run `whogitit doctor` first to diagnose most configuration issues automatically.

## Installation Issues

### `whogitit` command not found

**Symptoms:**
```
bash: whogitit: command not found
```

**Solutions:**

1. **Check if installed:**
   ```bash
   ls ~/.cargo/bin/whogitit
   ```

2. **Add cargo bin to PATH:**
   ```bash
   # Add to ~/.bashrc or ~/.zshrc
   export PATH="$HOME/.cargo/bin:$PATH"

   # Reload
   source ~/.bashrc
   ```

3. **Reinstall:**
   ```bash
   cargo install --path /path/to/whogitit --force
   ```

### Build fails

**Symptoms:**
```
error[E0433]: failed to resolve: could not find `xyz` in `abc`
```

**Solutions:**

1. **Update Rust:**
   ```bash
   rustup update stable
   ```

2. **Clean and rebuild:**
   ```bash
   cargo clean
   cargo build --release
   ```

3. **Check dependencies:**
   ```bash
   cargo update
   ```

## Capture Issues

### No attribution data after commit

**Symptoms:**
- `whogitit status` shows "No pending AI attribution"
- `whogitit show HEAD` shows no attribution

**Quick Fix:**

Run the doctor command to diagnose:
```bash
whogitit doctor
```

If doctor shows issues, run setup to fix them:
```bash
whogitit setup
```

**Manual Solutions:**

1. **Check Claude hooks are configured:**
   ```bash
   cat ~/.claude/settings.json | jq '.hooks'
   ```

   Should show PreToolUse and PostToolUse entries.

2. **Check capture script exists:**
   ```bash
   ls -la ~/.claude/hooks/whogitit-capture.sh
   ```

3. **Check capture script is executable:**
   ```bash
   chmod +x ~/.claude/hooks/whogitit-capture.sh
   ```

4. **Check debug logs:**
   ```bash
   cat .whogitit/state/hook-debug.log
   cat .whogitit/state/hook-errors.log
   ```

5. **Verify whogitit binary path:**
   ```bash
   # In capture script, check WHOGITIT_BIN
   which whogitit
   ```

### Pending buffer not updating

**Symptoms:**
- Making edits with Claude but `whogitit status` shows old data

**Solutions:**

1. **Check you're in a git repo:**
   ```bash
   git rev-parse --show-toplevel
   ```

2. **Check pending file location:**
   ```bash
   ls -la .whogitit-pending.json
   ```

3. **Clear stale data and try again:**
   ```bash
   whogitit clear
   ```

### Only some files tracked

**Symptoms:**
- Some AI edits are tracked, others aren't

**Solutions:**

1. **Check tool matcher in settings.json:**
   ```json
   "matcher": "Edit|Write|Bash"
   ```

   Make sure all relevant tools are included.

2. **For Bash commands:** The hook tracks file changes. Ensure the files exist before the command.

## Git Hooks Issues

### post-commit hook not running

**Symptoms:**
- Commits work but no attribution is created

**Solutions:**

1. **Check hook exists:**
   ```bash
   ls -la .git/hooks/post-commit
   ```

2. **Check hook is executable:**
   ```bash
   chmod +x .git/hooks/post-commit
   ```

3. **Check hook content:**
   ```bash
   cat .git/hooks/post-commit
   ```

   Should contain `whogitit post-commit`.

4. **Reinstall hooks:**
   ```bash
   whogitit init
   ```

### pre-push hook failing

**Symptoms:**
```
error: failed to push some refs to 'origin'
```

**Solutions:**

1. **Push notes manually:**
   ```bash
   git push origin refs/notes/whogitit
   ```

2. **Check for notes divergence:**
   ```bash
   git fetch origin refs/notes/whogitit
   git log refs/notes/whogitit..FETCH_HEAD
   ```

3. **Force push notes (caution):**
   ```bash
   git push origin refs/notes/whogitit --force
   ```

## Notes Issues

### Notes not visible after clone

**Symptoms:**
- Cloned repo, but `whogitit blame` shows no AI attribution

**Solutions:**

1. **Fetch notes:**
   ```bash
   git fetch origin refs/notes/whogitit:refs/notes/whogitit
   ```

2. **Verify notes exist on remote:**
   ```bash
   git ls-remote origin refs/notes/whogitit
   ```

3. **Configure auto-fetch:**
   ```bash
   git config --add remote.origin.fetch '+refs/notes/whogitit:refs/notes/whogitit'
   ```

### Notes missing after rebase

**Symptoms:**
- After rebasing, attribution data is gone

**Cause:**
Git notes are attached to specific commit SHAs. Rebase creates new SHAs.

**Solutions:**

1. **Install the post-rewrite hook (recommended):**
   ```bash
   whogitit init
   ```

   This installs a `post-rewrite` hook that automatically preserves attribution during rebase and amend operations. Run this once per repository.

2. **Manually copy notes after the fact:**
   ```bash
   whogitit copy-notes <old-sha> <new-sha>
   ```

3. **For cherry-pick (not covered by hook):**
   ```bash
   whogitit copy-notes <original-sha> <cherry-picked-sha>
   ```

### Corrupt note data

**Symptoms:**
```
Error: invalid JSON in note
```

**Solutions:**

1. **View raw note:**
   ```bash
   git notes --ref=whogitit show <commit> | cat
   ```

2. **Remove corrupt note:**
   ```bash
   git notes --ref=whogitit remove <commit>
   ```

3. **Recreate if pending buffer exists:** Not possible after buffer is cleared.

## Output Issues

### JSON parse errors

**Symptoms:**
```
Error: expected value at line 1 column 1
```

**Solutions:**

1. **Check for empty output:**
   ```bash
   whogitit show HEAD 2>&1 | head -1
   ```

2. **Verify note exists:**
   ```bash
   git notes --ref=whogitit show HEAD
   ```

### Blame shows all "Unknown"

**Symptoms:**
- `whogitit blame` shows `?` for all lines

**Cause:**
- File not in any commit with attribution
- Attribution data doesn't include this file

**Solutions:**

1. **Check if file has attribution:**
   ```bash
   whogitit show HEAD | grep -i "filename"
   ```

2. **Check commit history:**
   ```bash
   git log --oneline -- <file>
   ```

## Performance Issues

### Slow blame on large files

**Symptoms:**
- `whogitit blame` takes long time on large files

**Solutions:**

1. **Use `--revision` to limit scope:**
   ```bash
   whogitit blame --revision HEAD~10 <file>
   ```

2. **Use JSON output for scripting:**
   ```bash
   whogitit blame --format json <file>
   ```

### Large pending buffer

**Symptoms:**
- `.whogitit-pending.json` is very large

**Solutions:**

1. **Commit more frequently:** Pending buffer grows with each edit.

2. **Clear and restart:**
   ```bash
   whogitit clear
   ```

## Getting Help

### Run Doctor First

The doctor command checks all configuration automatically:

```bash
whogitit doctor
```

It verifies:
- whogitit binary is installed
- Capture hook is installed and executable
- Claude Code settings are configured
- Required tools (jq) are available
- Repository hooks are installed (if in a git repo)

If any checks fail, it provides fix hints.

### Debug Mode

Enable verbose logging:

```bash
# For capture hook
export WHOGITIT_HOOK_DEBUG=1

# Then use Claude Code, check:
cat .whogitit/state/hook-debug.log
```

### Reporting Issues

When reporting issues, include:

1. **whogitit version:**
   ```bash
   whogitit --version
   ```

2. **OS and shell:**
   ```bash
   uname -a
   echo $SHELL
   ```

3. **Relevant logs:**
   ```bash
   cat .whogitit/state/hook-debug.log
   cat .whogitit/state/hook-errors.log
   ```

4. **Steps to reproduce**

Report at: https://github.com/dotsetlabs/whogitit/issues

## See Also

- [Installation](../getting-started/installation.md) - Setup guide
- [Hook System](../reference/hooks.md) - Hook details
- [FAQ](./faq.md) - Common questions
