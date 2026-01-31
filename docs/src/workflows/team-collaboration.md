# Team Collaboration

Best practices for using whogitit across a development team.

## Setting Up for Teams

### 1. Repository Configuration

Add whogitit configuration to your repository:

```toml
# .whogitit.toml
[privacy]
enabled = true
audit_log = true

# Add organization-specific patterns
[[privacy.custom_patterns]]
name = "INTERNAL_API_KEY"
pattern = "company-api-[a-f0-9]{32}"

[retention]
max_age_days = 365
retain_refs = ["refs/heads/main", "refs/heads/develop"]
min_commits = 500
```

Commit this file so all team members use the same settings.

### 2. Documentation

Add setup instructions to your project README or CONTRIBUTING.md:

```markdown
## AI Attribution Setup

This project uses whogitit to track AI-generated code.

### Setup
1. Install whogitit: `cargo install --git https://github.com/dotsetlabs/whogitit`
2. Initialize: `whogitit init`
3. Configure Claude Code hooks (see [installation docs])

### Pushing Notes
Attribution notes are automatically pushed with `git push`.

To manually push: `git push origin refs/notes/whogitit`
```

### 3. CI Integration

Add the GitHub Action to automatically comment on PRs. See [CI/CD Integration](./ci-cd.md).

## Fetching Team Attribution

When cloning or fetching:

```bash
# Clone with notes
git clone --config remote.origin.fetch='+refs/notes/*:refs/notes/*' <repo-url>

# Or fetch notes manually
git fetch origin 'refs/notes/*:refs/notes/*'
```

After `whogitit init`, this is configured automatically.

## Team Policies

### AI Disclosure Policy

Define when AI attribution is required:

```markdown
## AI Code Policy

1. **All AI-assisted code must be attributed**
   - whogitit must be initialized in your local repo
   - Push git notes with every PR

2. **Review requirements by AI percentage**
   - <25% AI: Standard review
   - 25-75% AI: Two reviewers required
   - >75% AI: Security review required

3. **Sensitive areas require human code**
   - Authentication/authorization
   - Payment processing
   - PII handling
```

### Commit Message Guidelines

Encourage including AI context in commit messages:

```markdown
## Commit Messages

For AI-assisted commits, optionally include:

```
feat: Add email validation

Prompts used:
- "Add email validation using regex"
- "Add test cases for edge cases"

Human modifications:
- Improved regex pattern
- Added international domain support
```
```

### Code Review Standards

Define review expectations:

```markdown
## Reviewing AI Code

When reviewing PRs with AI attribution:

1. Check the AI Attribution Summary comment
2. Focus extra attention on `●` (pure AI) lines
3. Verify `◐` (modified) lines make sense
4. Ensure tests cover AI-generated logic
5. Check for common AI pitfalls:
   - Hallucinated APIs
   - Security vulnerabilities
   - Non-idiomatic patterns
```

## Metrics and Reporting

### Weekly AI Usage Report

Generate team-wide metrics:

```bash
#!/bin/bash
# weekly-report.sh

echo "# AI Usage Report - Week of $(date +%Y-%m-%d)"
echo

# Fetch latest
git fetch --all

# Get stats for the week
whogitit export \
  --since "$(date -d '7 days ago' +%Y-%m-%d)" \
  --format json \
  -o /tmp/weekly.json

# Summarize
jq '{
  total_commits: .summary.total_commits,
  ai_lines: .summary.total_ai_lines,
  human_lines: .summary.total_human_lines,
  ai_percentage: ((.summary.total_ai_lines + .summary.total_ai_modified_lines) * 100 /
    (.summary.total_ai_lines + .summary.total_ai_modified_lines + .summary.total_human_lines))
}' /tmp/weekly.json
```

### Dashboard Integration

Export data for dashboards:

```bash
# Export for Grafana/DataDog
whogitit export --format json | \
  jq -c '.commits[] | {
    timestamp: .committed_at,
    ai_lines: .ai_lines,
    human_lines: .human_lines
  }' | \
  while read line; do
    curl -X POST "https://metrics.example.com/ingest" -d "$line"
  done
```

## Handling Edge Cases

### Contributor Without whogitit

If a contributor doesn't have whogitit set up:

```bash
# Their commits will have no attribution
# This is visible in PR summaries as "commits without attribution"

# After merging, you can optionally add a note:
git notes --ref=whogitit add -m '{"schema_version":2,"session":{"session_id":"unknown"},"prompts":[],"files":[]}' <commit>
```

### Rebasing and Attribution

When rebasing, git notes may not follow:

```bash
# After rebase, copy notes from old commits
git notes --ref=whogitit copy <old-commit> <new-commit>

# Or fetch and let whogitit handle it
git fetch origin refs/notes/whogitit
```

### Squash Merging

Squash merging creates new commits without notes. Options:

1. **Merge commits instead**: Preserves individual commit notes
2. **Generate summary**: Add a note to the squash commit summarizing attribution
3. **Accept loss**: For small PRs, the PR comment serves as record

## Security Considerations

### Redaction Review

Periodically review what's being redacted:

```bash
whogitit audit --event-type redaction --limit 100
```

Ensure sensitive patterns are being caught.

### Access Control

Git notes can contain prompts. Consider:

- Are prompts themselves sensitive?
- Should notes be in a separate repo?
- Do you need fine-grained access control?

### Audit Trail

For compliance, enable audit logging:

```toml
[privacy]
audit_log = true
```

And periodically archive:

```bash
cp .whogitit/audit.log "audit-backup-$(date +%Y%m%d).log"
```

## Training and Onboarding

### New Developer Checklist

```markdown
## AI Attribution Setup Checklist

- [ ] Install whogitit
- [ ] Run `whogitit init` in each project
- [ ] Configure Claude Code hooks
- [ ] Make a test commit and verify `whogitit blame`
- [ ] Review team AI policies
```

### Common Issues

Document solutions to common problems:

```markdown
## Troubleshooting

**No attribution data after commit**
- Check `whogitit status` before committing
- Verify hooks are installed: `ls .git/hooks/post-commit`
- Check Claude Code hooks in `~/.claude/settings.json`

**Notes not pushing**
- Manually push: `git push origin refs/notes/whogitit`
- Check pre-push hook: `cat .git/hooks/pre-push`

**Missing attribution on clone**
- Fetch notes: `git fetch origin refs/notes/whogitit:refs/notes/whogitit`
```

## See Also

- [CI/CD Integration](./ci-cd.md) - Automated PR comments
- [Code Review](./code-review.md) - Review practices
- [Configuration](../guide/configuration.md) - Team-wide settings
