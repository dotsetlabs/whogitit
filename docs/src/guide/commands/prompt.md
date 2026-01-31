# prompt

View the prompt that generated specific lines of code.

## Synopsis

```bash
whogitit prompt [OPTIONS] <FILE:LINE>
```

## Description

The `prompt` command retrieves and displays the prompt that was used to generate a specific line of code. This helps you understand the context and intent behind AI-generated code.

## Arguments

| Argument | Description |
|----------|-------------|
| `<FILE:LINE>` | File path and line number (e.g., `src/main.rs:42`) |

## Options

| Option | Description |
|--------|-------------|
| `--revision <REF>` | Look up prompt at a specific revision (default: HEAD) |
| `--format <FORMAT>` | Output format: `pretty` (default), `json` |

## Examples

### Basic Usage

```bash
whogitit prompt src/main.rs:42
```

Output:

```
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

### At a Specific Revision

```bash
whogitit prompt --revision v1.0.0 src/auth.rs:15
```

### JSON Output

```bash
whogitit prompt --format json src/main.rs:42
```

Output:

```json
{
  "prompt": {
    "index": 2,
    "text": "Add JWT token generation with 24-hour expiration. Use the jsonwebtoken crate. The function should take a user_id and return a Result<String>.",
    "affected_files": ["src/auth.rs", "src/main.rs"]
  },
  "session": {
    "session_id": "7f3a-4b2c-9d1e-8a7b",
    "model": "claude-opus-4-5-20251101",
    "started_at": "2026-01-30T14:23:17Z"
  },
  "line": {
    "file": "src/main.rs",
    "line_number": 42,
    "source": "AI",
    "content": "    let token = generate_jwt(user_id)?;"
  }
}
```

## Output Details

### Prompt Box

The pretty output displays:
- Prompt index number (within the session)
- Session ID (truncated)
- Model used
- Timestamp
- Full prompt text

### Affected Files

Lists all files that were modified as a result of this prompt. This helps you understand the scope of changes a single prompt triggered.

## Use Cases

### Understanding AI-Generated Code

When reviewing code and you see an AI attribution symbol, use `prompt` to understand why the AI wrote it that way:

```bash
# See the blame first
whogitit blame src/auth.rs
#   42   │ abc1234 │ Greg King  │  ●  │     let expiry = Utc::now() + Duration::hours(24);

# Look up the prompt
whogitit prompt src/auth.rs:42
```

### Code Review

During code review, quickly check what prompts led to specific changes:

```bash
# Find AI lines in a changed file
whogitit blame --ai-only src/api.rs

# Check prompts for interesting lines
whogitit prompt src/api.rs:55
```

### Debugging

If AI-generated code isn't working as expected, the prompt can reveal misunderstood requirements:

```bash
# What was the AI asked to do?
whogitit prompt src/buggy_function.rs:10
```

## Notes

- If the line is not AI-generated (Original or Human), the command will indicate this
- Prompts may be redacted if they contained sensitive information
- The prompt index can be used to cross-reference with `whogitit show` output

## See Also

- [blame](./blame.md) - Find AI-generated lines
- [show](./show.md) - See all prompts for a commit
