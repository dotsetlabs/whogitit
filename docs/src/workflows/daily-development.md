# Daily Development Workflow

This guide covers how to use whogitit as part of your everyday development workflow with Claude Code.

## Typical Session

### 1. Start Coding with Claude

Open your project and start working with Claude Code:

```
> Add a function to validate email addresses
```

Claude edits your files. Behind the scenes, whogitit's hooks capture:
- The file state before each edit
- The file state after each edit
- The prompt from the session transcript

### 2. Review and Modify

Review Claude's changes. Make any modifications you want:

```rust
// Claude wrote:
fn validate_email(email: &str) -> bool {
    email.contains('@')
}

// You improve it:
fn validate_email(email: &str) -> bool {
    let pattern = r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$";
    regex::Regex::new(pattern).unwrap().is_match(email)
}
```

These modifications will be tracked as `AIModified` lines.

### 3. Add Your Own Code

Write additional code yourself:

```rust
// You add:
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_email() {
        assert!(validate_email("test@example.com"));
    }
}
```

This will be tracked as `Human` lines.

### 4. Check Status Before Committing

```bash
whogitit status
```

Output:

```
Pending AI attribution:
  Session: 7f3a-4b2c-9d1e-8a7b
  Files: 1
  Edits: 2
  Lines: 15

Run 'git commit' to finalize attribution.
```

### 5. Commit

```bash
git add src/validation.rs
git commit -m "Add email validation with tests"
```

The post-commit hook automatically:
1. Analyzes the pending changes
2. Creates attribution data
3. Attaches it as a git note
4. Clears the pending buffer

### 6. Verify Attribution

```bash
whogitit blame src/validation.rs
```

```
 LINE   │ COMMIT  │ AUTHOR     │ SRC │ CODE
───────────────────────────────────────────────────────────────────
    1   │ a1b2c3d │ Greg King  │  ◐  │ fn validate_email(email: &str) -> bool {
    2   │ a1b2c3d │ Greg King  │  ◐  │     let pattern = r"^[a-zA-Z0-9...
    3   │ a1b2c3d │ Greg King  │  ◐  │     regex::Regex::new(pattern)...
    4   │ a1b2c3d │ Greg King  │  ◐  │ }
    5   │ a1b2c3d │ Greg King  │  +  │
    6   │ a1b2c3d │ Greg King  │  +  │ #[cfg(test)]
    7   │ a1b2c3d │ Greg King  │  +  │ mod tests {
...
```

## Multiple Edits Per Commit

### Working on Multiple Files

Claude Code often edits multiple files. All changes within a session are tracked together:

```
> Add user authentication with login and registration endpoints
```

This might touch:
- `src/auth.rs` (new file)
- `src/main.rs` (add routes)
- `src/db.rs` (add user model)

All are captured with the same session ID and prompt.

### Multiple Prompts

If you give Claude multiple prompts before committing:

```
> Add the User struct
> Now add password hashing
> Add email verification
```

Each prompt is recorded separately. You can trace which prompt generated which code:

```bash
whogitit show HEAD
```

```
Prompts used:
  #0: "Add the User struct..."
  #1: "Now add password hashing..."
  #2: "Add email verification..."
```

```bash
whogitit prompt src/auth.rs:15
```

Shows which specific prompt generated line 15.

## Discarding AI Changes

Sometimes you don't want to keep AI-generated changes. You have options:

### Discard Everything

```bash
# Discard git changes
git checkout .

# Clear whogitit pending buffer
whogitit clear
```

### Keep Changes, Discard Attribution

If you want to keep the code but not track it as AI-generated:

```bash
# Clear the pending buffer
whogitit clear

# Commit without attribution
git add .
git commit -m "Changes without AI tracking"
```

### Partial Commit

Stage only specific files:

```bash
git add src/auth.rs
git commit -m "Add authentication"
# Only auth.rs gets attribution from pending buffer
```

## Pushing Changes

When you push, git notes are included automatically:

```bash
git push
```

The pre-push hook runs:

```bash
git push origin refs/notes/whogitit
```

## Tips

### Check Status Often

Get in the habit of checking `whogitit status` alongside `git status`:

```bash
git status && whogitit status
```

### Clear Stale Data

If you've been experimenting but don't want to commit:

```bash
whogitit clear
```

### Verify After Major Changes

After significant AI-assisted work, verify attribution looks correct:

```bash
whogitit blame src/new_feature.rs
```

### Use Meaningful Commits

Since whogitit tracks at the commit level, meaningful atomic commits help:

```bash
# Good: One feature per commit
git commit -m "Add email validation"
git commit -m "Add phone validation"

# Less good: Multiple unrelated changes
git commit -m "Add validation and fix typo and update deps"
```

## See Also

- [Quick Start](../getting-started/quick-start.md) - Basic setup
- [Code Review](./code-review.md) - Reviewing AI-generated code
- [Core Concepts](../getting-started/concepts.md) - Understanding attribution
