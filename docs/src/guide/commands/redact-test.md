# redact-test

Test redaction patterns against text or files.

## Usage

```bash
whogitit redact-test [OPTIONS]
```

## Description

The `redact-test` command allows you to test whogitit's privacy redaction system against sample text or files. This helps verify that sensitive data will be properly redacted before being stored in git notes.

## Options

| Option | Description |
|--------|-------------|
| `--text <TEXT>` | Text to test redaction on (conflicts with `--file`) |
| `--file <FILE>` | File to read and test redaction on (conflicts with `--text`) |
| `--matches-only` | Show only matches without redacting |
| `--audit` | Show audit trail of redactions |
| `--list-patterns` | List available redaction patterns |
| `--json` | Output as JSON |

## Built-in Patterns

whogitit includes patterns for common sensitive data:

| Pattern | Description |
|---------|-------------|
| `API_KEY` | Generic API keys (`api_key`, `apikey`, `secret`, `token`) |
| `AWS_KEY` | AWS access keys (`AKIA...`) |
| `PRIVATE_KEY` | PEM-encoded private keys |
| `BEARER_TOKEN` | Bearer authentication tokens |
| `GITHUB_TOKEN` | GitHub tokens (`ghp_`, `gho_`, `ghs_`, `ghr_`) |
| `SLACK_TOKEN` | Slack tokens (`xoxb-`, `xoxp-`, `xoxa-`) |
| `STRIPE_KEY` | Stripe API keys (`sk_live_`, `pk_live_`) |
| `PASSWORD` | Password patterns in config-like contexts |
| `EMAIL` | Email addresses |
| `SSN` | Social Security Numbers |

## Examples

### Test text inline

```bash
whogitit redact-test --text "My API key is api_key=sk-1234567890"
```

Output:
```
Redacted output:
My API key is api_key=[REDACTED]
```

### Test a file

```bash
whogitit redact-test --file .env
```

### List available patterns

```bash
whogitit redact-test --list-patterns
```

Output:
```
Available Redaction Patterns
==================================================
API_KEY          Generic API key patterns
AWS_KEY          AWS access key IDs
PRIVATE_KEY      PEM private keys
...
```

### Show matches only

See what would be redacted without showing the redacted output:

```bash
whogitit redact-test --text "email: user@example.com, key: sk-123" --matches-only
```

Output:
```
Sensitive data 2 found:

  EMAIL            user@example.com
  API_KEY          sk-123
```

### Audit trail

See detailed information about each redaction:

```bash
whogitit redact-test --text "password=secret123" --audit
```

Output:
```
Audit Trail: 1 redactions made:

  Pattern: PASSWORD  Range: (9, 18)  Preview: secret123

Redacted output:
password=[REDACTED]
```

### JSON output

```bash
whogitit redact-test --text "token=abc123" --json
```

Output:
```json
{
  "input_length": 12,
  "output": "token=[REDACTED]",
  "match_count": 1,
  "matches": ["API_KEY"]
}
```

## Custom Patterns

You can add custom redaction patterns in `.whogitit.toml`:

```toml
[privacy]
audit_log = true

[[privacy.custom_patterns]]
name = "INTERNAL_ID"
pattern = "INTERNAL-\\d+"
description = "Internal tracking IDs"

[[privacy.custom_patterns]]
name = "COMPANY_SECRET"
pattern = "ACME-[A-Z0-9]+"
description = "Company-specific secrets"
```

Then test your custom patterns:

```bash
whogitit redact-test --text "Reference: INTERNAL-12345" --list-patterns
```

## Disabling Patterns

Disable built-in patterns you don't need:

```toml
[privacy]
disabled_patterns = ["EMAIL", "SSN"]
```

## See Also

- [Privacy & Redaction](../privacy.md) - Full privacy configuration guide
- [Configuration](../configuration.md) - `.whogitit.toml` reference
