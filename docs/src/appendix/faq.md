# FAQ

Frequently asked questions about whogitit.

## General

### What is whogitit?

whogitit is a tool that tracks AI-generated code at the line level. It tells you which lines were written by AI, which were modified by humans, and what prompts generated them.

### Why track AI-generated code?

- **Transparency**: Know exactly what percentage of your codebase was AI-generated
- **Code Review**: Identify AI-generated sections that may need extra scrutiny
- **Compliance**: Meet organizational requirements for AI usage disclosure
- **Learning**: Understand how AI suggestions were modified by humans
- **Debugging**: When something breaks, know if AI wrote it and what prompt was used

### Does whogitit work with ChatGPT/Copilot/other AI tools?

Currently, whogitit is designed for Claude Code integration. It uses Claude Code's hook system to capture changes. Support for other tools may be added in the future.

### Is whogitit open source?

Yes. whogitit is available at https://github.com/dotsetlabs/whogitit under the MIT license.

## Usage

### Do I need to do anything special when coding?

No. Once set up, whogitit works automatically:
1. You use Claude Code normally
2. Hooks capture your changes
3. You commit normally
4. Attribution is attached to the commit

### What happens if I modify AI-generated code?

whogitit tracks this! Lines modified by humans after AI generation are marked as `AIModified` (◐) rather than `AI` (●). This shows that a human reviewed and changed the code.

### Can I use whogitit without Claude Code?

The capture system is designed for Claude Code hooks. Without Claude Code, you can still:
- View attribution from commits made by others
- Manually add attribution (advanced)
- Use the CLI tools for analysis

### Does whogitit slow down my workflow?

The overhead is minimal:
- Hooks run in milliseconds
- Post-commit analysis is fast (typically <1 second)
- No impact on normal git operations

## Privacy

### Are my prompts stored publicly?

Prompts are stored in git notes, which are pushed with your repository. If your repo is public, prompts are visible. However:
- Sensitive data is automatically redacted
- You can add custom redaction patterns
- You can disable prompt storage entirely

### What gets redacted automatically?

- API keys and secrets
- AWS credentials
- Private keys
- Bearer tokens
- GitHub tokens
- Email addresses
- Passwords
- Credit card numbers
- Social Security Numbers

### Can I disable redaction?

Yes, but it's not recommended:

```toml
# .whogitit.toml
[privacy]
enabled = false
```

### Can I add custom redaction patterns?

Yes:

```toml
[[privacy.custom_patterns]]
name = "MY_SECRET"
pattern = "my-secret-[a-f0-9]{32}"
description = "My custom secrets"
```

## Data Storage

### Where is attribution data stored?

Attribution is stored as git notes under `refs/notes/whogitit`. This is a standard git feature - notes are stored in the git object database.

### Does attribution increase repository size?

Slightly. Each attributed commit adds a small JSON blob (typically 1-10 KB). For most projects, this is negligible.

### What happens if I delete a branch?

Git notes are attached to commits, not branches. If commits are garbage-collected, their notes go too. If commits are still reachable, notes remain.

### Can I delete attribution data?

Yes:

```bash
# Single commit
git notes --ref=whogitit remove <commit>

# Using retention policy
whogitit retention apply --execute
```

## Git Operations

### Does attribution survive rebasing?

Not automatically. Rebase creates new commits with new SHAs. You can manually copy notes from old to new commits.

### Does attribution survive squash merging?

No. Squash creates a new commit. The individual commit notes are not combined. Consider using regular merge to preserve notes.

### Can I cherry-pick commits with attribution?

Cherry-pick creates new commits. Attribution doesn't automatically transfer. You can manually copy the note.

### How do I share attribution with my team?

Attribution is stored in git notes, which are pushed/fetched:

```bash
# Push
git push origin refs/notes/whogitit

# Fetch (configured automatically by `whogitit init`)
git fetch origin refs/notes/whogitit:refs/notes/whogitit
```

## Troubleshooting

### Why is there no attribution after my commit?

Common causes:
1. Claude Code hooks not configured
2. `whogitit init` not run in the repo
3. Capture script not executable
4. Edits made outside Claude Code

See [Troubleshooting](./troubleshooting.md) for details.

### Why are some lines marked "Unknown"?

This happens when:
- The file wasn't included in the AI session
- Attribution data is corrupted or missing
- The line existed before whogitit was set up

### Why don't I see attribution after cloning?

Git notes need to be fetched separately:

```bash
git fetch origin refs/notes/whogitit:refs/notes/whogitit
```

### Can I fix incorrect attribution?

Attribution can be edited (it's just JSON in git notes):

```bash
git notes --ref=whogitit edit <commit>
```

But be careful - manual edits can cause inconsistencies.

## Advanced

### Can I use whogitit in CI/CD?

Yes! See [CI/CD Integration](../workflows/ci-cd.md). Common uses:
- Add attribution summaries to PR comments
- Enforce review policies based on AI percentage
- Export metrics for dashboards

### Can I query attribution programmatically?

Yes:

```bash
# JSON output
whogitit blame --format json file.rs
whogitit show --format json HEAD
whogitit summary --format json

# Export
whogitit export --format json
```

### Can I integrate with other tools?

whogitit outputs standard JSON. Integrate with:
- Dashboards (Grafana, DataDog)
- Code quality tools
- Custom scripts
- Slack/Teams notifications

### How do I contribute to whogitit?

See the repository at https://github.com/dotsetlabs/whogitit:
- Report issues
- Submit pull requests
- Improve documentation

## See Also

- [Troubleshooting](./troubleshooting.md) - Problem solutions
- [Core Concepts](../getting-started/concepts.md) - How it works
- [Architecture](../reference/architecture.md) - Technical details
