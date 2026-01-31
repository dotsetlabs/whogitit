# Installation

## Prerequisites

- **Rust** (1.70 or later) - [Install Rust](https://rustup.rs/)
- **Git** (2.25 or later)
- **Claude Code** - For automatic AI attribution capture

## Install from Source

```bash
# Clone the repository
git clone https://github.com/dotsetlabs/whogitit
cd whogitit

# Install to ~/.cargo/bin
cargo install --path .
```

## Verify Installation

```bash
whogitit --version
```

You should see output like:

```
whogitit 0.1.0
```

## Install the Capture Hook

The capture hook integrates with Claude Code to automatically track AI-generated changes.

### 1. Copy the hook script

```bash
mkdir -p ~/.claude/hooks
cp hooks/whogitit-capture.sh ~/.claude/hooks/
chmod +x ~/.claude/hooks/whogitit-capture.sh
```

### 2. Configure Claude Code

Add the following to `~/.claude/settings.json`:

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

> **Note**: The hook captures Edit, Write, and Bash tool uses. Bash commands that modify files (like `sed`, `echo >`, etc.) are also tracked.

## Initialize a Repository

In each repository where you want to track AI attribution:

```bash
cd your-project
whogitit init
```

This command:
- Installs a `post-commit` hook that attaches attribution data to commits
- Installs a `pre-push` hook that automatically pushes git notes
- Configures git to fetch notes automatically

## Verify Setup

After setup, you can verify everything is working:

```bash
# Check that hooks are installed
ls -la .git/hooks/post-commit .git/hooks/pre-push

# Check git notes configuration
git config --get-all remote.origin.fetch | grep whogitit
```

## Next Steps

- Continue to [Quick Start](./quick-start.md) to see whogitit in action
- Learn about [Core Concepts](./concepts.md) to understand how attribution works
