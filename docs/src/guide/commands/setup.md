# setup

Configure Claude Code integration for whogitit.

## Usage

```bash
whogitit setup
```

## Description

The `setup` command performs one-time global configuration to integrate whogitit with Claude Code. This command should be run once after installing whogitit, before initializing any repositories.

## What It Does

1. **Installs capture hook script**
   - Creates `~/.claude/hooks/` directory if needed
   - Writes `whogitit-capture.sh` to `~/.claude/hooks/`
   - Sets executable permissions

2. **Configures Claude Code settings**
   - Creates `~/.claude/settings.json` if it doesn't exist
   - Adds PreToolUse and PostToolUse hook configuration
   - Preserves existing settings (creates backup at `settings.json.backup`)

## Example Output

```text
Setting up whogitit for Claude Code...

  Installed capture hook to ~/.claude/hooks/whogitit-capture.sh
  Configured Claude Code hooks in ~/.claude/settings.json
    (Previous settings backed up to settings.json.backup)

Global setup complete!

Next steps:
  1. Run 'whogitit init' in each repository you want to track
  2. Use Claude Code normally - AI attribution will be captured automatically

Run 'whogitit doctor' to verify your configuration at any time.
```

## Re-running Setup

It's safe to run `setup` multiple times:
- If the hook script is already installed and current, it will be skipped
- If settings are already configured, they won't be duplicated

## After Setup

After running `setup`, initialize each repository where you want to track AI attribution:

```bash
cd your-project
whogitit init
```

## See Also

- [doctor](./doctor.md) - Verify configuration
- [Installation](../../getting-started/installation.md) - Full installation guide
