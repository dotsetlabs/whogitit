# Privacy & Redaction

whogitit automatically redacts sensitive information from prompts before storing them in git notes. This protects credentials, personal information, and other secrets that might accidentally appear in your prompts.

## How Redaction Works

When a prompt is captured, whogitit scans it against a set of patterns and replaces matches with `[REDACTED]`:

**Before:**
```text
Add authentication using api_key = sk-12345abcdef
```

**After (stored):**
```text
Add authentication using api_key = [REDACTED]
```

## Built-in Patterns

whogitit includes patterns for common sensitive data:

### Credentials

| Pattern | Examples |
|---------|----------|
| **API_KEY** | `api_key=xxx`, `apikey: xxx`, `token=xxx` |
| **AWS_KEY** | `AKIA...` and matching AWS secret keys |
| **BEARER_TOKEN** | `Bearer eyJ...`, `Authorization: Bearer ...` |
| **GITHUB_TOKEN** | `ghp_xxx`, `gho_xxx`, `ghs_xxx`, `ghr_xxx` |
| **GENERIC_SECRET** | Generic `secret=...`, `credential=...` assignments |
| **DB_CONNECTION** | `postgres://user:pass@host/db`, similar DSNs |
| **SLACK_TOKEN** | `xoxb-xxx`, `xoxp-xxx`, `xoxa-xxx` |
| **STRIPE_KEY** | `sk_live_...`, `pk_test_...`, `rk_live_...` |
| **JWT_TOKEN** | `eyJ...` JSON Web Tokens |
| **GOOGLE_OAUTH** | `1//...` Google refresh tokens |
| **MICROSOFT_OAUTH** | `0.A...` Microsoft/Azure refresh tokens |
| **DOCKER_REGISTRY** | Docker registry `user:pass@registry` credentials |
| **K8S_SECRET** | Kubernetes secret declarations/commands |
| **BASE64_SECRET** | Long base64 values tied to secret-like keys |
| **NPM_TOKEN** | `npm_...` authentication tokens |
| **PYPI_TOKEN** | `pypi-AgEI...` API tokens |
| **PASSWORD** | `password=xxx`, `passwd: xxx` |
| **PRIVATE_KEY** | `-----BEGIN.*PRIVATE KEY-----` blocks |

### Personal Information

| Pattern | Examples |
|---------|----------|
| **EMAIL** | `user@example.com` |
| **PHONE** | `(555) 123-4567`, `+1-555-123-4567` |
| **SSN** | `123-45-6789` |
| **CREDIT_CARD** | `4111-1111-1111-1111` |

## Testing Redaction

Use `redact-test` to see how your text would be redacted:

```bash
whogitit redact-test --text "Connect using api_key=sk-secret123 and email user@example.com"
```

Output:

```text
Redacted output:
Connect using [REDACTED] and email [REDACTED]
```

Use `--matches-only` to list exact pattern matches without printing redacted output.

### Testing Files

```bash
whogitit redact-test --file config.example.txt
```

## Custom Patterns

Add organization-specific patterns in `.whogitit.toml`:

```toml
[privacy]
# Custom patterns
[[privacy.custom_patterns]]
name = "INTERNAL_ID"
pattern = "INT-[A-Z0-9]{8}"
description = "Internal system IDs"

[[privacy.custom_patterns]]
name = "EMPLOYEE_ID"
pattern = "EMP\\d{6}"
description = "Employee identification numbers"

[[privacy.custom_patterns]]
name = "DATABASE_URL"
pattern = "postgres://[^\\s]+"
description = "PostgreSQL connection strings"
```

### Pattern Syntax

Patterns use Rust regex syntax (similar to PCRE):

| Syntax | Meaning |
|--------|---------|
| `\d` | Digit |
| `\w` | Word character |
| `\s` | Whitespace |
| `[A-Z]` | Character class |
| `+` | One or more |
| `*` | Zero or more |
| `{n}` | Exactly n times |
| `{n,m}` | Between n and m times |
| `(?i)` | Case insensitive |

### Testing Custom Patterns

After adding patterns, verify they work:

```bash
whogitit redact-test --text "Reference INT-ABC12345 for employee EMP123456"
```

## Disabling Patterns

If a built-in pattern is too aggressive, disable it:

```toml
[privacy]
disabled_patterns = ["EMAIL", "PHONE"]
```

Common reasons to disable:

| Pattern | Why Disable |
|---------|-------------|
| EMAIL | Open source projects where contributor emails are public |
| PHONE | False positives with version numbers or IDs |

## Disabling Redaction

To disable redaction entirely:

```toml
[privacy]
enabled = false
```

> **Warning**: Disabling redaction may expose sensitive data in your git history. Only do this if you're certain no sensitive data will appear in prompts.

## Audit Trail

When audit logging is enabled, redaction events are recorded:

```toml
[privacy]
audit_log = true
```

View redaction events:

```bash
whogitit audit --event-type redaction
```

Output:

```text
2026-01-30 14:23:15 redaction pattern:API_KEY redactions:2
2026-01-30 14:20:00 redaction pattern:EMAIL redactions:1
```

## Best Practices

### 1. Test Before Production

Before enabling whogitit on a project, test redaction with representative prompts:

```bash
# Test various scenarios
whogitit redact-test --text "Your typical prompt with api_key=xxx"
```

### 2. Add Organization Patterns

Identify internal secret formats and add patterns:

```toml
[[privacy.custom_patterns]]
name = "ACME_API_KEY"
pattern = "acme_[a-f0-9]{32}"
description = "ACME Corp API keys"
```

### 3. Review Periodically

Check the audit log for missed patterns:

```bash
whogitit audit --event-type redaction --limit 100
```

If certain patterns are matching frequently, they're working. If sensitive data is getting through, add new patterns.

### 4. Use Environment Variables

Instead of including secrets in prompts, reference environment variables:

**Instead of:**
```text
Connect to postgres://user:password@host/db
```

**Use:**
```text
Connect using the DATABASE_URL environment variable
```

### 5. Enable Audit Logging

For compliance-sensitive projects:

```toml
[privacy]
audit_log = true
```

## Limitations

- Redaction is pattern-based and cannot catch everything
- Novel secret formats may not be detected
- Context-dependent secrets (e.g., "the password is banana") are not detected
- Redaction is one-way - original text cannot be recovered

## See Also

- [Configuration](./configuration.md) - Full configuration reference
- [audit](./commands/audit.md) - Viewing redaction audit trail
