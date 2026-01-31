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
│  post-commit  ──►  Analyze pending buffer, create attribution  │
│  pre-push     ──►  Push notes alongside code                   │
└─────────────────────────────────────────────────────────────────┘
```

## Claude Code Hooks

### Configuration

Add to `~/.claude/settings.json`:

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
| `WHOGITIT_DEBUG` | Enable debug logging |

### Debug Logging

```bash
# Enable debug mode
export WHOGITIT_DEBUG=1

# Logs go to
/tmp/whogitit-hook-debug.log
/tmp/whogitit-hook-errors.log
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

## Installing Hooks

### Automatic

```bash
whogitit init
```

Installs both git hooks and configures notes fetching.

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
# Check if whogitit is in PATH
which whogitit

# Check capture hook
ls -la ~/.claude/hooks/whogitit-capture.sh

# Check debug logs
cat /tmp/whogitit-hook-debug.log
cat /tmp/whogitit-hook-errors.log
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
