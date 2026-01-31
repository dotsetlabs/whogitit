# Code Review Workflow

whogitit enhances code review by providing visibility into AI-generated code. This guide covers best practices for reviewing PRs with AI attribution.

## Reviewing a Pull Request

### 1. Get the PR Summary

First, get an overview of AI involvement:

```bash
# Fetch and checkout the PR
git fetch origin pull/123/head:pr-123
git checkout pr-123

# Get attribution summary
whogitit summary --base main
```

Output:

```
AI Attribution Summary
======================

Commits analyzed: 5 (3 with AI attribution)

Overview:
  AI-generated lines:     145 (58.0%)
  AI-modified by human:    12 (4.8%)
  Human-added lines:       43 (17.2%)
  Original/unchanged:      50 (20.0%)

AI involvement: 62.8% of changed lines
```

This tells you:
- How much of the PR was AI-assisted
- Whether the author modified AI output (AIModified)
- How much was purely human-written

### 2. Identify AI-Generated Files

See which files have the most AI involvement:

```bash
whogitit show HEAD~2..HEAD
```

Or blame specific files:

```bash
whogitit blame --ai-only src/auth.rs
```

### 3. Review AI-Generated Code

AI-generated code (`●`) deserves extra scrutiny for:

**Security Issues:**
- SQL injection vulnerabilities
- XSS in templates
- Hardcoded secrets (should be redacted, but check)
- Insecure defaults

**Logic Errors:**
- Off-by-one errors
- Incorrect boundary conditions
- Missing error handling

**Style Issues:**
- Non-idiomatic patterns
- Inconsistent naming
- Missing documentation

### 4. Review Human Modifications

Lines marked `AIModified` (`◐`) were AI-generated then changed. These are interesting because:

- The author saw something to improve
- They might have caught an AI mistake
- Or they might have introduced a new issue

Look at what changed:

```bash
whogitit prompt src/auth.rs:15
```

Compare the prompt to the actual code - does the modification make sense?

### 5. Review Prompts

Understanding what the author asked for helps contextualize the code:

```bash
whogitit show HEAD
```

Good prompts lead to better AI output. Consider:
- Was the prompt clear and specific?
- Did it mention edge cases?
- Did it specify error handling?

## Review Checklist

### For AI-Generated Lines (`●`)

- [ ] Does the code actually do what the prompt asked?
- [ ] Are there security vulnerabilities?
- [ ] Is error handling appropriate?
- [ ] Are there obvious logic errors?
- [ ] Does it follow project conventions?
- [ ] Are there unnecessary dependencies?

### For AI-Modified Lines (`◐`)

- [ ] Why was modification needed?
- [ ] Does the modification fix a real issue?
- [ ] Is the modification correct?
- [ ] Should similar patterns elsewhere be checked?

### For Human Lines (`+`)

- [ ] Standard code review practices apply
- [ ] Does it integrate well with AI-generated code?
- [ ] Are there gaps in AI-generated code that human code fills?

## Common Patterns

### Good Signs

- **High AIModified percentage**: Author is reviewing and improving AI output
- **Human tests for AI code**: Author is verifying AI behavior
- **Clear prompts**: Author knew what they wanted
- **Incremental prompts**: Complex features built step-by-step

### Warning Signs

- **100% AI, no modifications**: Author may not have reviewed carefully
- **Complex logic with no tests**: AI-generated logic should be tested
- **Security-sensitive code**: Extra scrutiny needed
- **Vague prompts**: "Make it work" leads to unpredictable code

## Leaving Feedback

### Reference AI Attribution

When commenting, reference the attribution:

```
This AI-generated code (lines 15-30) doesn't handle the case where
`user_id` is None. The prompt asked for "user authentication" but
didn't specify guest user handling.
```

### Suggest Prompt Improvements

If the issue stems from the prompt:

```
The AI generated this based on "Add authentication". For future
features, consider more specific prompts like "Add JWT authentication
with 24-hour token expiry and refresh token support".
```

### Ask About Modifications

If AIModified code is unclear:

```
Line 42 was modified from AI output. Could you explain what the
original AI generated and why you changed it?
```

## GitHub Action Integration

For automated PR comments with AI attribution summaries, see [CI/CD Integration](./ci-cd.md).

The action adds a comment like:

```markdown
## 🤖 AI Attribution Summary

This PR contains **3** of **5** commits with AI-assisted changes.

| Metric | Lines | Percentage |
|--------|------:|----------:|
| AI-generated | 145 | 58.0% |
| AI-modified | 12 | 4.8% |
| Human-added | 43 | 17.2% |
```

## Team Policies

Consider establishing team guidelines:

### Minimum Review for AI Code

```markdown
PRs with >50% AI-generated code require:
- Two reviewers
- Explicit security review
- Test coverage for AI-generated functions
```

### Prompt Documentation

```markdown
For significant AI-generated features, include the prompts in PR description:
- What prompts were used
- Why that approach was chosen
- What modifications were made
```

### Attribution in Commit Messages

```markdown
Commits with significant AI involvement should note it:

git commit -m "Add email validation

AI-assisted: 80% of validation logic
Human-modified: regex pattern
Human-added: test cases"
```

## See Also

- [CI/CD Integration](./ci-cd.md) - Automated PR comments
- [Team Collaboration](./team-collaboration.md) - Team workflows
- [summary](../guide/commands/summary.md) - Summary command reference
