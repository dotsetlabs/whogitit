# doctor

Check whogitit configuration and diagnose issues.

## Usage

```bash
whogitit doctor
```

## Description

The `doctor` command performs a comprehensive check of your whogitit configuration. It verifies that all components are properly installed and configured, and provides hints for fixing any issues found.

## What It Checks

| Check | Description |
|-------|-------------|
| whogitit binary | Confirms the binary is installed and running |
| Capture hook | Verifies hook script exists at `~/.claude/hooks/whogitit-capture.sh` |
| Hook permissions | Confirms the hook script is executable |
| Claude Code settings | Checks that `~/.claude/settings.json` has whogitit hooks configured |
| Required tools | Verifies `jq` is installed (required by capture hook) |
| Repository hooks | If in a git repo, checks that post-commit, pre-push, and post-rewrite hooks are installed |
| Attribution notes | If notes exist, checks for orphaned notes (attached to deleted commits) |

## Example Output

### All Checks Passing

```text
Checking whogitit configuration...

[OK] whogitit binary: Installed and running
[OK] Capture hook: Installed at /Users/you/.claude/hooks/whogitit-capture.sh
[OK] Hook permissions: Executable
[OK] Claude Code settings: Hooks configured
[OK] Required tools (jq): Available
[OK] Repository hooks: Initialized in current repo
[OK] Attribution notes: 42 notes, all valid

All checks passed! whogitit is properly configured.
```

### With Issues

```text
Checking whogitit configuration...

[OK] whogitit binary: Installed and running
[FAIL] Capture hook: Not installed
   Fix: Run 'whogitit setup' to install
[FAIL] Hook permissions: Hook not installed
   Fix: Run 'whogitit setup'
[FAIL] Claude Code settings: whogitit hooks not configured
   Fix: Run 'whogitit setup' to configure
[OK] Required tools (jq): Available
[FAIL] Repository hooks: Missing or invalid hooks: post-rewrite
   Fix: Run 'whogitit init' in this repository

Some checks failed. Run 'whogitit setup' to fix configuration issues.
```

## Fixing Issues

Most issues can be fixed automatically:

1. **Global configuration issues** (hook, settings):
   ```bash
   whogitit setup
   ```

2. **Repository-specific issues** (git hooks):
   ```bash
   whogitit init
   ```

3. **Missing jq**:
   - macOS: `brew install jq`
   - Ubuntu/Debian: `apt install jq`
   - Fedora: `dnf install jq`

## When to Run Doctor

Run `whogitit doctor` when:
- After initial installation to verify setup
- When attribution isn't being captured
- After upgrading whogitit
- When debugging issues

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | All checks passed or completed with warnings |

## See Also

- [setup](./setup.md) - Configure Claude Code integration
- [Troubleshooting](../../appendix/troubleshooting.md) - Common issues and solutions
