# blame

Show AI attribution for each line of a file, similar to `git blame` but with AI source information.

## Synopsis

```bash
whogitit blame [OPTIONS] <FILE>
```

## Description

The `blame` command displays each line of a file with its attribution status, showing whether the line was:
- Written by AI and unchanged (`●`)
- Written by AI and modified by a human (`◐`)
- Added by a human after AI edits (`+`)
- Original content that existed before AI edits (`─`)

## Arguments

| Argument | Description |
|----------|-------------|
| `<FILE>` | Path to the file to blame |

## Options

| Option | Description |
|--------|-------------|
| `--revision <REF>` | Blame at a specific git revision (default: HEAD) |
| `--format <FORMAT>` | Output format: `pretty` (default), `json` |
| `--ai-only` | Show only AI-generated lines |
| `--human-only` | Show only human-written lines |

## Examples

### Basic Usage

```bash
whogitit blame src/main.rs
```

Output:

```
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

### Blame at a Specific Revision

```bash
whogitit blame --revision v1.0.0 src/main.rs
```

### Show Only AI Lines

```bash
whogitit blame --ai-only src/main.rs
```

### JSON Output

```bash
whogitit blame --format json src/main.rs
```

Output:

```json
{
  "file": "src/main.rs",
  "revision": "a1b2c3d",
  "lines": [
    {
      "line_number": 1,
      "commit": "a1b2c3d",
      "author": "Greg King",
      "source": "Original",
      "content": "use std::io;"
    },
    {
      "line_number": 2,
      "commit": "d4e5f6g",
      "author": "Greg King",
      "source": "AI",
      "content": "use anyhow::Result;"
    }
  ],
  "summary": {
    "ai_lines": 2,
    "ai_modified_lines": 1,
    "human_lines": 1,
    "original_lines": 1,
    "ai_percentage": 60.0
  }
}
```

## Understanding the Output

### Column Descriptions

| Column | Description |
|--------|-------------|
| LINE | Line number in the file |
| COMMIT | Short SHA of the commit that introduced this line |
| AUTHOR | Git author who committed this line |
| SRC | Attribution source symbol |
| CODE | The actual line content |

### Attribution Symbols

| Symbol | Meaning |
|--------|---------|
| `●` | AI-generated, unchanged |
| `◐` | AI-generated, then modified by human |
| `+` | Human-written (after AI session) |
| `─` | Original (existed before AI session) |
| `?` | Unknown (attribution could not be determined) |

### Summary Statistics

The footer shows:
- Count of each attribution type
- AI involvement percentage (AI + AIModified lines / total lines)

## Notes

- If a file has no AI attribution data, the command falls back to standard git blame output with all lines marked as Original (`─`)
- The `--ai-only` and `--human-only` flags are mutually exclusive
- Line numbers start at 1, matching most editor conventions

## See Also

- [show](./show.md) - View commit-level attribution summary
- [prompt](./prompt.md) - Find the prompt that generated a line
