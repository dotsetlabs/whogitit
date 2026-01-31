# whogitit

Track AI-generated code at line-level granularity. Know exactly which lines were written by AI, which were modified by humans, and what prompts generated them.

## Features

- **Line-level attribution** - Track whether each line is AI-generated, human-modified, or original
- **Prompt preservation** - Store the prompts that generated code alongside commits
- **Three-way diff analysis** - Accurate attribution even when you edit AI code before committing
- **Git-native storage** - Uses git notes that travel with your repository
- **Claude Code integration** - Automatic capture via hooks
- **GitHub Action** - PR comments showing AI attribution summaries with prompts
- **Privacy protection** - Automatic redaction of API keys, passwords, and sensitive data
- **Data retention policies** - Configurable age limits and auto-purge for compliance
- **Audit logging** - Track deletions, exports, and configuration changes
- **Export capabilities** - Bulk export attribution data as JSON or CSV

## Installation

```bash
# From source
git clone https://github.com/dotsetlabs/whogitit
cd whogitit
cargo install --path .
```

## Setup

### 1. Initialize in your repository

```bash
cd your-project
whogitit init
```

This installs git hooks that:
- Attach attribution data to commits (post-commit)
- Push git notes with your commits (pre-push)
- Configure git to fetch notes automatically

### 2. Configure Claude Code hooks

Copy the capture script to your Claude hooks directory:

```bash
mkdir -p ~/.claude/hooks
cp hooks/whogitit-capture.sh ~/.claude/hooks/
chmod +x ~/.claude/hooks/whogitit-capture.sh
```

Add to `~/.claude/settings.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Edit|Write",
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
        "matcher": "Edit|Write",
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

### 3. Push notes with commits

Git notes must be pushed separately. After `whogitit init`, this happens automatically on `git push`. To push manually:

```bash
git push origin refs/notes/whogitit
```

## CLI Commands

### `whogitit blame <file>`

Show AI attribution for each line:

```
$ whogitit blame src/main.rs

 LINE   │ COMMIT  │ AUTHOR     │ SRC │ CODE
─────────────────────────────────────────────────────────────────────────────────────
    1   │ a1b2c3d │ Greg King  │  ─  │ use std::io;
    2   │ d4e5f6g │ Greg King  │  ●  │ use anyhow::Result;
    3   │ d4e5f6g │ Greg King  │  ●  │ use serde::{Deserialize, Serialize};
    4   │ d4e5f6g │ Greg King  │  ◐  │ use chrono::Utc;  // modified
    5   │ h8i9j0k │ Greg King  │  +  │ // Added by human
─────────────────────────────────────────────────────────────────────────────────────
Legend: ● AI (2) ◐ AI-modified (1) + Human (1) ─ Original (1)
AI involvement: 60% (3 of 5 lines)
```

Options:
- `--revision <ref>` - Blame at a specific revision
- `--format json` - JSON output
- `--ai-only` - Show only AI-generated lines
- `--human-only` - Show only human-written lines

### `whogitit show <commit>`

View attribution summary for a commit:

```
$ whogitit show HEAD

Commit: d4e5f6g
Session: 7f3a-4b2c-9d1e-8a7b
Model: claude-opus-4-5-20251101
Started: 2026-01-30T14:23:17Z

Prompts used:
  #0: "Add error handling with anyhow..."
  #1: "Implement the serialize trait..."

Files with AI changes:
  src/auth.rs (25 AI, 3 modified, 2 human) - 45 total lines
  src/main.rs (10 AI, 5 original) - 15 total lines

Summary:
  35 AI-generated lines
  3 AI lines modified by human
  2 human-added lines
  5 original/unchanged lines
```

### `whogitit prompt <file:line>`

View the prompt that generated specific lines:

```
$ whogitit prompt src/main.rs:42

╔════════════════════════════════════════════════════════════════════╗
║  PROMPT #2 in session 7f3a-4b2c-9d1e...                            ║
║  Model: claude-opus-4-5-20251101 | 2026-01-30T14:23:17Z            ║
╠════════════════════════════════════════════════════════════════════╣
║  Add JWT token generation with 24-hour expiration. Use the         ║
║  jsonwebtoken crate. The function should take a user_id and        ║
║  return a Result<String>.                                          ║
╚════════════════════════════════════════════════════════════════════╝

Files affected by this prompt:
  - src/auth.rs
  - src/main.rs
```

### `whogitit summary`

Generate attribution summary for a commit range (useful for PRs):

```bash
whogitit summary --base main --head HEAD
whogitit summary --base main --format markdown
whogitit summary --format json
```

### `whogitit status`

Check pending attribution changes (before commit):

```
$ whogitit status

Pending AI attribution:
  Session: 7f3a-4b2c-9d1e-8a7b
  Files: 3
  Lines: 45

Run 'git commit' to finalize attribution.
```

### `whogitit clear`

Discard pending changes without committing:

```bash
whogitit clear
```

### `whogitit export`

Export attribution data for multiple commits:

```bash
whogitit export                           # JSON to stdout
whogitit export --format csv -o data.csv  # CSV to file
whogitit export --since 2026-01-01        # Filter by date
whogitit export --full-prompts            # Include full prompt text
```

Options:
- `--format json|csv` - Output format (default: json)
- `--since <date>` - Only commits after date (YYYY-MM-DD)
- `--until <date>` - Only commits before date
- `-o, --output <file>` - Output file (default: stdout)
- `--full-prompts` - Include full prompt text (default: truncated)

### `whogitit retention`

Manage data retention policies:

```bash
whogitit retention config   # Show current retention settings
whogitit retention preview  # Preview what would be deleted
whogitit retention apply    # Dry-run deletion
whogitit retention apply --execute  # Actually delete old data
```

### `whogitit audit`

View the audit log (tracks deletions, exports, config changes):

```bash
whogitit audit                    # Show last 50 events
whogitit audit --limit 100        # Show more events
whogitit audit --since 2026-01-01 # Filter by date
whogitit audit --event-type delete # Filter by type
whogitit audit --json             # JSON output
```

### `whogitit redact-test`

Test redaction patterns against text or files:

```bash
whogitit redact-test "text with api_key=secret123"
whogitit redact-test --file config.txt
```

## Configuration

Create `.whogitit.toml` in your repository root to configure privacy and retention:

```toml
[privacy]
# Enable audit logging for compliance
audit_log = true
# Disable specific builtin patterns
disabled_patterns = ["EMAIL"]

# Additional custom redaction patterns
[[privacy.custom_patterns]]
name = "INTERNAL_ID"
pattern = "INTERNAL-\\d+"
description = "Internal tracking IDs"

[retention]
# Delete attribution older than 365 days
max_age_days = 365
# Keep at least 100 commits regardless of age
min_commits = 100
# Never delete attribution for these refs
retain_refs = ["refs/heads/main"]
# Auto-purge on commit (default: false)
auto_purge = false
```

## GitHub Action

Add AI attribution summaries to pull requests automatically.

### Setup

Create `.github/workflows/ai-attribution.yml`:

```yaml
name: AI Attribution Summary

on:
  pull_request:
    types: [opened, synchronize, reopened]

permissions:
  contents: read
  pull-requests: write

jobs:
  analyze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
          ref: ${{ github.event.pull_request.head.sha }}

      - name: Fetch git notes
        run: git fetch origin refs/notes/whogitit:refs/notes/whogitit || true
        continue-on-error: true

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            target/
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Build whogitit
        run: cargo build --release

      # ... analysis and comment posting steps
      # See .github/workflows/ai-attribution.yml for full implementation
```

### Example PR Comment

The action posts a comment like this:

---

## 🤖🤖 AI Attribution Summary

This PR contains **3** of **5** commits with AI-assisted changes.

### Overview

| Metric | Lines | Percentage |
|--------|------:|----------:|
| 🟢 AI-generated | 145 | 58.0% |
| 🟡 AI-modified by human | 12 | 4.8% |
| 🔵 Human-added | 43 | 17.2% |
| ⚪ Original/unchanged | 50 | 20.0% |
| **Total** | **250** | **100%** |

**AI involvement: 62.8%** of changed lines

### Commits with AI Attribution

| Commit | Message | AI | Modified | Human | Files |
|--------|---------|---:|--------:|------:|------:|
| `abc1234` | Add user authentication | 45 | 3 | 10 | 2 |
| `def5678` | Implement JWT tokens | 100 | 9 | 33 | 3 |

### Prompts Used (2)

**Prompt 1** (src/auth.rs, src/main.rs)
<details>
<summary>Add user authentication with bcrypt password hashing...</summary>

```
Add user authentication with bcrypt password hashing. Create a User struct
with email and password_hash fields. Implement register and login functions
that return Result types.
```
</details>

**Prompt 2** (src/jwt.rs)
<details>
<summary>Implement JWT token generation with 24-hour expiration...</summary>

```
Implement JWT token generation with 24-hour expiration. Use the jsonwebtoken
crate. The function should take a user_id and return a Result<String>.
```
</details>

---

## How It Works

### Three-Way Diff Analysis

whogitit captures complete file snapshots during editing, enabling accurate attribution:

1. **Original** - Content before any AI edits
2. **AI Snapshots** - Content after each AI edit
3. **Final** - Content at commit time

This allows tracking even when you modify AI-generated code before committing.

### Line Attribution Types

| Source | Symbol | Description |
|--------|--------|-------------|
| AI | `●` | Generated by AI, unchanged |
| AIModified | `◐` | Generated by AI, then edited by human |
| Human | `+` | Added by human after AI edits |
| Original | `─` | Existed before AI session |
| Unknown | `?` | Could not determine source |

### Data Flow

```
Claude Code (Edit/Write tools)
         │
         ├─► PreToolUse: Save file state
         ├─► PostToolUse: Capture change + prompt
         │
         ▼
Pending Buffer (.whogitit-pending.json)
         │
         ▼ git commit
         │
Three-Way Analysis → Git Notes (refs/notes/whogitit)
```

## Storage

Attribution is stored in git notes (`refs/notes/whogitit`), which:
- Travel with repository when pushed/fetched
- Don't clutter commit history
- Can be inspected with standard git commands

```bash
# View raw attribution
git notes --ref=whogitit show HEAD

# List all attributed commits
git notes --ref=whogitit list
```

## Privacy

Prompts are automatically scanned and redacted for:

- API keys (`api_key`, `apikey`, `secret`, `token`)
- AWS credentials (`AKIA...`)
- Private keys (`-----BEGIN.*PRIVATE KEY-----`)
- Bearer tokens
- GitHub tokens (`ghp_`, `gho_`, `ghs_`, `ghr_`)
- Email addresses
- Passwords

Sensitive data is replaced with `[REDACTED]` before storage.

## License

MIT
