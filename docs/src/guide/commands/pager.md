# pager

Annotate git diff output with AI attribution markers.

## Usage

```bash
git diff | whogitit pager [OPTIONS]
```

## Description

The `pager` command reads git diff output from stdin and annotates added lines with AI attribution markers. This provides inline visibility into which diff lines were AI-generated directly in your terminal.

## Options

| Option | Description |
|--------|-------------|
| `--no-color` | Disable colored output |
| `-v, --verbose` | Show detailed attribution info (model, timestamps) |
| `--no-pager` | Output directly to stdout instead of through pager |

## Attribution Markers

Added lines are prefixed with attribution markers:

| Marker | Color | Meaning |
|--------|-------|---------|
| `●` | Green | AI-generated line, unchanged |
| `◐` | Yellow | AI-generated line, modified by human |
| (none) | - | Human-written or original line |

## Setup

### Option 1: Replace default git pager

Configure git to use whogitit for all diff output:

```bash
git config --global core.pager "whogitit pager"
```

### Option 2: Create git aliases

Create specific aliases for AI-annotated commands:

```bash
git config --global alias.ai-diff '!git diff | whogitit pager --no-pager'
git config --global alias.ai-show '!git show "$@" | whogitit pager --no-pager'
git config --global alias.ai-log '!git log -p "$@" | whogitit pager --no-pager'
```

## Examples

### Basic usage

```bash
git diff HEAD~1 | whogitit pager
```

### Compare branches

```bash
git diff main..feature | whogitit pager
```

### Verbose output

```bash
git diff | whogitit pager --verbose
```

Output includes edit IDs and similarity percentages for AI-modified lines.

### Pipe to less manually

```bash
git diff | whogitit pager --no-pager | less -R
```

The `-R` flag tells less to interpret color codes.

### View commit with attribution

```bash
git show abc123 | whogitit pager
```

## Output Example

```diff
diff --git a/src/main.rs b/src/main.rs
@@ -40,4 +45,8 @@ impl Server {
● +    fn handle_error(e: Error) -> Result<()> {
● +        log::error!("Failed: {}", e);
● +        retry_with_backoff(|| reconnect())
● +    }
◐ +    // Added timeout handling
  +    const TIMEOUT: u64 = 30;  // Human-added
```

## Troubleshooting

### Annotations not appearing

1. Ensure attribution data exists:
   ```bash
   whogitit blame <file>
   ```

2. Fetch git notes:
   ```bash
   git fetch origin refs/notes/whogitit:refs/notes/whogitit
   ```

3. Verify whogitit is in PATH:
   ```bash
   which whogitit
   ```

### Colors not working

If colors don't appear in your pager:

```bash
# Use -R flag with less
git diff | whogitit pager --no-pager | less -R

# Or disable colors
git diff | whogitit pager --no-color
```

### Slow performance on large diffs

For very large diffs, the pager needs to look up attribution for each file. Consider using `--no-pager` and piping through `head` to preview:

```bash
git diff | whogitit pager --no-pager | head -100
```

## See Also

- [blame](./blame.md) - Line-by-line attribution
- [Git Integration](../../usage/git-integration.md) - Complete git setup guide
