# Quick Start

This guide will get you tracking AI-generated code in under 5 minutes.

## 1. Install whogitit

```bash
# Install whogitit (if not already done)
cargo install --path /path/to/whogitit
```

## 2. Run Global Setup (once)

```bash
whogitit setup
```

This configures Claude Code integration automatically.

## 3. Initialize Your Repository

```bash
cd your-project
whogitit init
```

## 4. Verify Setup

```bash
whogitit doctor
```

You should see all checks passing.

## 5. Write Some Code with Claude

Use Claude Code to make changes to your project. For example:

```text
> Add a function that validates email addresses
```

Claude will edit or create files. whogitit automatically captures:
- The file content before the edit
- The file content after the edit
- The prompt you used

## 6. Check Pending Attribution

Before committing, see what whogitit has captured:

```bash
whogitit status
```

Output:

```text
Pending AI attribution:
  Session: 7f3a-4b2c-9d1e-8a7b
  Files: 2
  Edits: 3
  Lines: 45

Run 'git commit' to finalize attribution.
```

## 7. Commit Your Changes

Commit as normal:

```bash
git add .
git commit -m "Add email validation"
```

The post-commit hook automatically:
1. Analyzes the pending changes
2. Performs three-way diff analysis
3. Attaches attribution data as a git note

## 8. View Attribution

### See AI attribution for a file

```bash
whogitit blame src/validation.rs
```

Output:

```text
 LINE   │ COMMIT  │ AUTHOR     │ SRC │ CODE
─────────────────────────────────────────────────────────────────────────────────────
    1   │ a1b2c3d │ Greg King  │  ●  │ use regex::Regex;
    2   │ a1b2c3d │ Greg King  │  ●  │
    3   │ a1b2c3d │ Greg King  │  ●  │ pub fn validate_email(email: &str) -> bool {
    4   │ a1b2c3d │ Greg King  │  ◐  │     let pattern = r"^[a-zA-Z0-9._%+-]+@";  // simplified
    5   │ a1b2c3d │ Greg King  │  ●  │     let re = Regex::new(pattern).unwrap();
    6   │ a1b2c3d │ Greg King  │  ●  │     re.is_match(email)
    7   │ a1b2c3d │ Greg King  │  ●  │ }
─────────────────────────────────────────────────────────────────────────────────────
Legend: ● AI (6) ◐ AI-modified (1)
AI involvement: 100% (7 of 7 lines)
```

### See the commit summary

```bash
whogitit show HEAD
```

Output:

```text
Commit: a1b2c3d
Session: 7f3a-4b2c-9d1e-8a7b
Model: claude-opus-4-5-20251101
Started: 2026-01-30T14:23:17Z

Prompts used:
  #0: "Add a function that validates email addresses..."

Files with AI changes:
  src/validation.rs (6 AI, 1 modified) - 7 total lines

Summary:
  6 AI-generated lines
  1 AI line modified by human
```

### Find the prompt that generated a line

```bash
whogitit prompt src/validation.rs:3
```

Output:

```text
╔════════════════════════════════════════════════════════════════════╗
║  PROMPT #0 in session 7f3a-4b2c-9d1e...                            ║
║  Model: claude-opus-4-5-20251101 | 2026-01-30T14:23:17Z            ║
╠════════════════════════════════════════════════════════════════════╣
║  Add a function that validates email addresses                      ║
╚════════════════════════════════════════════════════════════════════╝

Files affected by this prompt:
  - src/validation.rs
```

## 9. Push with Attribution

When you push, git notes are automatically included:

```bash
git push
```

The pre-push hook handles `git push origin refs/notes/whogitit` automatically.

## What's Next?

- Learn about [Core Concepts](./concepts.md)
- Explore all [CLI Commands](../guide/cli-commands.md)
- Set up [CI/CD Integration](../workflows/ci-cd.md) for PR comments
- Configure [Privacy & Redaction](../guide/privacy.md) rules
