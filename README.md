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

### Quick Install (Recommended)

**macOS / Linux:**
```bash
curl -sSL https://github.com/dotsetlabs/whogitit/releases/latest/download/install.sh | sh
```

**Windows (PowerShell):**
```powershell
irm https://github.com/dotsetlabs/whogitit/releases/latest/download/install.ps1 | iex
```

### Via Cargo

```bash
cargo install whogitit
```

### From Source

```bash
git clone https://github.com/dotsetlabs/whogitit
cd whogitit
cargo install --path .
```

## Setup

### 1. One-time global setup

Run this once to configure Claude Code integration:

```bash
whogitit setup
```

This automatically:
- Installs the capture hook to `~/.claude/hooks/`
- Configures Claude Code's `settings.json` with the required hooks
- No manual file copying or JSON editing needed

### 2. Initialize each repository

```bash
cd your-project
whogitit init
```

This installs git hooks that:
- Attach attribution data to commits (post-commit)
- Push git notes with your commits (pre-push)
- Preserve notes during rebase/amend (post-rewrite)
- Configure git to fetch notes automatically

### 3. Verify your setup

```bash
whogitit doctor
```

This checks all configuration and shows any issues.

### Manual setup (alternative)

If you prefer manual configuration, see [detailed installation docs](docs/src/getting-started/installation.md).

### Push notes with commits

Git notes are pushed automatically on `git push` after `whogitit init`. To push manually:

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
whogitit redact-test --text "text with api_key=secret123"
whogitit redact-test --file config.txt
whogitit redact-test --list-patterns  # Show available patterns
whogitit redact-test --text "..." --matches-only  # Show matches without redacting
whogitit redact-test --text "..." --audit  # Show audit trail
```

### `whogitit annotations`

Generate annotations for GitHub Checks API (used by CI):

```bash
whogitit annotations --base main --head HEAD
whogitit annotations --format json
whogitit annotations --consolidate file    # One annotation per file
whogitit annotations --consolidate lines   # Granular line annotations
whogitit annotations --min-ai-lines 10     # Filter insignificant files
whogitit annotations --diff-only           # Only annotate lines in PR diff
whogitit annotations --group-ai-types      # Group AI and AI-modified together
whogitit annotations --sort-by coverage    # Sort by AI coverage (default)
```

Options:
- `--base <commit>` - Base commit (exclusive)
- `--head <commit>` - Head commit (default: HEAD)
- `--format github-checks|json` - Output format
- `--consolidate auto|file|lines` - Consolidation mode (default: auto)
- `--consolidate-threshold <0.0-1.0>` - AI coverage threshold for auto mode (default: 0.7)
- `--consolidate-prompt-limit <n>` - Max prompts for auto-consolidation (default: 3)
- `--min-lines <n>` - Minimum AI lines for line-level annotation (default: 1)
- `--min-ai-lines <n>` - Minimum AI lines for a file to be annotated (default: 3)
- `--min-ai-percent <n>` - Minimum AI percentage for a file (default: 5.0)
- `--max-annotations <n>` - Maximum annotations (default: 50)
- `--ai-only` - Only annotate pure AI lines
- `--diff-only` - Only annotate lines within the PR diff
- `--group-ai-types` - Group AI and AIModified together
- `--sort-by coverage|lines|alpha` - Sort files by (default: coverage)

### `whogitit copy-notes`

Copy attribution between commits (useful after cherry-pick):

```bash
whogitit copy-notes <source-sha> <target-sha>
whogitit copy-notes abc123 def456 --dry-run
```

Note: For rebase and amend, the post-rewrite hook automatically preserves notes. Use `copy-notes` for cherry-pick or manual recovery.

### `whogitit pager`

Annotate git diff output with AI attribution (use as git pager):

```bash
# Configure as git pager
git config --global core.pager "whogitit pager"

# Or use with aliases
git config --global alias.ai-diff '!git diff | whogitit pager --no-pager'
git diff | whogitit pager

# Options
whogitit pager --verbose     # Show model, timestamps
whogitit pager --no-color    # Disable colors
whogitit pager --no-pager    # Output directly to stdout
```

See [Git Integration](docs/src/usage/git-integration.md) for more details.

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

This PR adds **+200** lines with AI attribution across **3** files.

### Additions Breakdown

| Metric | Lines | % of Additions |
|--------|------:|--------------:|
| 🟢 AI-generated | +145 | 72.5% |
| 🟡 AI-modified by human | +12 | 6.0% |
| 🔵 Human-written | +43 | 21.5% |
| **Total additions** | **+200** | **100%** |

**AI involvement: 78.5%** of additions are AI-generated

### Files Changed

| File | +Added | AI | Human | AI % | Status |
|------|-------:|---:|------:|-----:|--------|
| `src/auth.rs` | +80 | 72 | 8 | 90% | New |
| `src/main.rs` | +45 | 32 | 13 | 71% | Modified |
| `src/jwt.rs` | +75 | 53 | 22 | 71% | New |

### Commits with AI Attribution

| Commit | Message | AI | Modified | Human | Files |
|--------|---------|---:|--------:|------:|------:|
| `abc1234` | Add user authentication | 72 | 3 | 5 | 2 |
| `def5678` | Implement JWT tokens | 85 | 9 | 26 | 2 |

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
