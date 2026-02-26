# Installation

## Prerequisites

- **Git** (2.25 or later)
- **jq** - JSON processor used by capture hook
  - macOS: `brew install jq`
  - Linux: `apt install jq` or `dnf install jq`
- **Claude Code** - For automatic AI attribution capture

## Quick Install (Recommended)

**macOS / Linux:**
```bash
curl -sSL https://github.com/dotsetlabs/whogitit/releases/latest/download/install.sh | sh
```

**Windows (PowerShell):**
```powershell
irm https://github.com/dotsetlabs/whogitit/releases/latest/download/install.ps1 | iex
```

This downloads the latest pre-built binary and installs it to `~/.cargo/bin`.

## Install via Cargo

If you have Rust installed:

```bash
cargo install whogitit
```

## Install from Source

For development or to build from the latest code:

**Prerequisites:** [Rust](https://rustup.rs/) 1.70 or later

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

```text
whogitit 1.0.0
```

## Quick Setup (Recommended)

The easiest way to set up whogitit is with the automated setup command:

### 1. Run global setup (once)

```bash
whogitit setup
```

This automatically:
- Installs the capture hook script to `~/.claude/hooks/`
- Configures Claude Code's `~/.claude/settings.json` with required hooks
- Creates a backup of your existing settings

> **Note**: After running `whogitit setup`, you must also run `whogitit init` in each repository where you want to track AI attribution. The global hooks only activate for initialized repositories.

### 2. Initialize your repository

```bash
cd your-project
whogitit init
```

This installs git hooks for the repository:
- `post-commit` - Attaches attribution data to commits
- `pre-push` - Automatically pushes git notes with your code
- `post-rewrite` - Preserves notes during rebase and amend

### 3. Verify setup

```bash
whogitit doctor
```

This checks all configuration and shows any issues:

```text
[OK] whogitit binary: Installed and running
[OK] Capture hook: Installed at ~/.claude/hooks/whogitit-capture.sh
[OK] Hook permissions: Executable
[OK] Claude Code settings: Hooks configured
[OK] Required tools (jq): Available
[OK] Repository hooks: Initialized in current repo

All checks passed! whogitit is properly configured.
```

## Manual Setup (Alternative)

If you prefer manual configuration or need to customize the setup:

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

### 3. Initialize a Repository

In each repository where you want to track AI attribution:

```bash
cd your-project
whogitit init
```

## Verify Setup

After setup, you can verify everything is working:

```bash
# Run the doctor command for comprehensive checks
whogitit doctor

# Or check manually:
# Check that hooks are installed
ls -la .git/hooks/post-commit .git/hooks/pre-push

# Check git notes configuration
git config --get-all remote.origin.fetch | grep whogitit
```

## Next Steps

- Continue to [Quick Start](./quick-start.md) to see whogitit in action
- Learn about [Core Concepts](./concepts.md) to understand how attribution works
