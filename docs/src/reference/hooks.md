# Hook System

whogitit uses two hook systems: Claude Code hooks for capturing changes, and git hooks for processing commits.

## Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                      Claude Code Hooks                          │
│                                                                 │
│  PreToolUse (Edit|Write|Bash)  ──►  Save "before" state        │
│  PostToolUse (Edit|Write|Bash) ──►  Save "after" state + prompt│
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
                    .whogitit-pending.json
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                        Git Hooks                                │
│                                                                 │
│  post-commit   ──►  Analyze pending buffer, create attribution │
│  pre-push      ──►  Push notes alongside code                  │
│  post-rewrite  ──►  Preserve notes during rebase/amend         │
└─────────────────────────────────────────────────────────────────┘
```

## Claude Code Hooks

### Automatic Configuration

The easiest way to configure Claude Code hooks is using the setup command:

```bash
whogitit setup
```

This automatically:
- Installs the capture script to `~/.claude/hooks/whogitit-capture.sh`
- Configures `~/.claude/settings.json` with the required hooks
- The capture script is embedded in the whogitit binary, so no source files are needed

### Manual Configuration

If you prefer manual configuration, add to `~/.claude/settings.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Edit|Write|Bash",
        "hooks": [
          {
            "type": "command",
            "command": "WHOGITIT_HOOK_PHASE=pre ~/.claude/hooks/whogitit-capture.sh"
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Edit|Write|Bash",
        "hooks": [
          {
            "type": "command",
            "command": "WHOGITIT_HOOK_PHASE=post ~/.claude/hooks/whogitit-capture.sh"
          }
        ]
      }
    ]
  }
}
```

### Matched Tools

| Tool | Description |
|------|-------------|
| `Edit` | Modifies existing files |
| `Write` | Creates or overwrites files |
| `Bash` | Shell commands (may modify files) |

### Hook Input

Hooks receive JSON on stdin:

```json
{
  "tool_name": "Edit",
  "tool_input": {
    "file_path": "/path/to/file.rs",
    "old_string": "original text",
    "new_string": "replacement text"
  },
  "transcript_path": "/tmp/claude-transcript-xyz.jsonl"
}
```

### Capture Script

The capture script (`hooks/whogitit-capture.sh`) handles:

1. **Parsing hook input** - Extracts tool name, file path
2. **Phase routing** - PreToolUse vs PostToolUse
3. **File tracking** - Edit/Write: single file, Bash: all modified files
4. **Prompt extraction** - Reads transcript JSONL
5. **Calling whogitit** - `whogitit capture --stdin`

### PreToolUse Flow

```
1. Hook receives JSON with tool_name and file_path
2. If Edit/Write: save current file content as "before" snapshot
3. If Bash: snapshot all dirty files in repo
```

### PostToolUse Flow

```
1. Hook receives JSON with tool_name, file_path, transcript_path
2. If Edit/Write: save file content as "after" snapshot
3. If Bash: compare file states, detect which changed
4. Read transcript to extract user prompt
5. Update pending buffer with edit and prompt
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `WHOGITIT_HOOK_PHASE` | `pre` or `post` |
| `WHOGITIT_BIN` | Path to whogitit binary |
| `WHOGITIT_HOOK_DEBUG` | Enable debug logging |

### Debug Logging

```bash
# Enable debug mode
export WHOGITIT_HOOK_DEBUG=1

# Logs go to
.whogitit/state/hook-debug.log
.whogitit/state/hook-errors.log
```

## Git Hooks

### post-commit

Created by `whogitit init` in `.git/hooks/post-commit`:

```bash
#!/bin/bash
# whogitit post-commit hook

if command -v whogitit &> /dev/null; then
    whogitit post-commit 2>/dev/null || true
elif [[ -x "$HOME/.cargo/bin/whogitit" ]]; then
    "$HOME/.cargo/bin/whogitit" post-commit 2>/dev/null || true
fi
```

This hook:
1. Runs after every commit
2. Processes the pending buffer
3. Creates git note with attribution
4. Clears pending buffer
5. Applies retention policy automatically if `retention.auto_purge = true`

### pre-push

Created by `whogitit init` in `.git/hooks/pre-push`:

```bash
#!/bin/bash
# whogitit pre-push hook

# Prevent recursion
[[ "$WHOGITIT_PUSHING_NOTES" == "1" ]] && exit 0

remote="$1"

# Only push notes if they exist
if git notes --ref=whogitit list &>/dev/null; then
    WHOGITIT_PUSHING_NOTES=1 git push "$remote" refs/notes/whogitit 2>/dev/null || true
fi
```

This hook:
1. Runs before every push
2. Pushes notes to the same remote
3. Handles errors gracefully

### post-rewrite

Created by `whogitit init` in `.git/hooks/post-rewrite`:

```bash
#!/bin/bash
# whogitit post-rewrite hook
# Preserves AI attribution notes during rebase/amend

copied=0
while read -r old_sha new_sha extra; do
  [[ -z "$old_sha" || -z "$new_sha" ]] && continue
  if git notes --ref=whogitit show "$old_sha" &>/dev/null; then
    git notes --ref=whogitit copy "$old_sha" "$new_sha" 2>/dev/null && copied=$((copied + 1))
  fi
done

[[ $copied -gt 0 ]] && echo "whogitit: Preserved attribution for $copied commit(s)"
```

This hook:
1. Runs after `git rebase` and `git commit --amend`
2. Receives old→new SHA mappings on stdin
3. Copies notes from old commits to new commits
4. Reports how many notes were preserved

## Installing Hooks

### Automatic

```bash
whogitit init
```

Installs all three git hooks (post-commit, pre-push, post-rewrite) and configures notes fetching.

### Manual Git Hooks

```bash
# post-commit
cat > .git/hooks/post-commit << 'EOF'
#!/bin/bash
whogitit post-commit 2>/dev/null || true
EOF
chmod +x .git/hooks/post-commit

# pre-push
cat > .git/hooks/pre-push << 'EOF'
#!/bin/bash
[[ "$WHOGITIT_PUSHING_NOTES" == "1" ]] && exit 0
git push "$1" refs/notes/whogitit 2>/dev/null || true
EOF
chmod +x .git/hooks/pre-push
```

### Manual Claude Hooks

The recommended method is `whogitit setup`, which extracts the embedded capture script.

For manual installation from source:

```bash
mkdir -p ~/.claude/hooks
cp /path/to/whogitit/hooks/whogitit-capture.sh ~/.claude/hooks/
chmod +x ~/.claude/hooks/whogitit-capture.sh
```

Then edit `~/.claude/settings.json` as shown above.

## Coexistence with Other Hooks

### Appending to Existing Hooks

`whogitit init` detects existing hooks and appends:

```bash
# Existing hook
#!/bin/bash
run-tests.sh

# whogitit appends:

# whogitit post-commit hook
if command -v whogitit &> /dev/null; then
    whogitit post-commit 2>/dev/null || true
fi
```

### Hook Managers

If using a hook manager (husky, pre-commit, etc.), add whogitit commands:

**Husky:**
```bash
# .husky/post-commit
#!/bin/sh
. "$(dirname "$0")/_/husky.sh"

# Your hooks
npm test

# whogitit
whogitit post-commit 2>/dev/null || true
```

**pre-commit:**
```yaml
# .pre-commit-config.yaml
repos:
  - repo: local
    hooks:
      - id: whogitit
        name: whogitit attribution
        entry: whogitit post-commit
        language: system
        stages: [post-commit]
        always_run: true
```

## Troubleshooting

### Run Doctor First

The doctor command checks all hook configuration automatically:

```bash
whogitit doctor
```

### Hooks Not Running

```bash
# Check Claude hooks config
cat ~/.claude/settings.json | jq '.hooks'

# Check git hooks
ls -la .git/hooks/post-commit .git/hooks/pre-push

# Verify executability
file .git/hooks/post-commit
# Should say "executable"
```

### No Attribution Being Captured

```bash
# Run doctor for quick diagnosis
whogitit doctor

# Or check manually:
# Check if whogitit is in PATH
which whogitit

# Check capture hook
ls -la ~/.claude/hooks/whogitit-capture.sh

# Check debug logs
cat .whogitit/state/hook-debug.log
cat .whogitit/state/hook-errors.log
```

### Notes Not Pushing

```bash
# Check pre-push hook exists
cat .git/hooks/pre-push

# Manual push
git push origin refs/notes/whogitit

# Check remote
git ls-remote origin refs/notes/whogitit
```

## See Also

- [Installation](../getting-started/installation.md) - Setup instructions
- [Architecture](./architecture.md) - System design
- [Troubleshooting](../appendix/troubleshooting.md) - Common issues
