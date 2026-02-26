# Configuration

whogitit is configured via a TOML file (`.whogitit.toml`) in your repository root or globally at `~/.config/whogitit/config.toml`.

## Configuration File Location

whogitit looks for configuration in this order:

0. **Environment override**: `WHOGITIT_CONFIG=/path/to/config.toml` (if set)
1. **Repository-local**: `.whogitit.toml` in the repository root
2. **Global**: `~/.config/whogitit/config.toml`
3. **Defaults**: Built-in default values

Repository-local configuration takes precedence over global configuration.
When `WHOGITIT_CONFIG` is set, it takes precedence over all other config locations.

If a configuration file is present but invalid, CLI commands will return an error so you can fix it.
Hook-based capture will log a warning and fall back to defaults to avoid breaking your workflow.

## Complete Configuration Reference

```toml
# .whogitit.toml

[privacy]
# Enable/disable redaction (default: true)
enabled = true

# Use built-in redaction patterns (default: true)
use_builtin_patterns = true

# Disable specific built-in patterns by name
disabled_patterns = ["EMAIL"]

# Enable audit logging (default: false)
audit_log = true

# Add custom redaction patterns
[[privacy.custom_patterns]]
name = "INTERNAL_ID"
pattern = "INTERNAL-\\d+"
description = "Internal tracking IDs"

[[privacy.custom_patterns]]
name = "PROJECT_SECRET"
pattern = "PROJ_[A-Z0-9]{16}"
description = "Project-specific secrets"

[retention]
# Maximum age of attribution data in days
max_age_days = 365

# Automatically purge old data on commit (default: false)
auto_purge = false

# Never delete attribution for commits reachable from these refs
retain_refs = ["refs/heads/main", "refs/heads/release"]

# Keep at least this many commits regardless of age
min_commits = 100

[analysis]
# Maximum pending buffer age in hours (default: 24)
max_pending_age_hours = 24

# Similarity threshold for AIModified detection (default: 0.6)
similarity_threshold = 0.6
```

## Privacy Section

### enabled

```toml
[privacy]
enabled = true  # default
```

Master switch for redaction. When `false`, no redaction is performed.

### use_builtin_patterns

```toml
[privacy]
use_builtin_patterns = true  # default
```

Whether to use the built-in redaction patterns. See [Privacy & Redaction](./privacy.md) for the full list.

## Analysis Section

### max_pending_age_hours

```toml
[analysis]
max_pending_age_hours = 24  # default
```

Controls when the pending buffer is considered stale (used by `whogitit status` and warnings).

### similarity_threshold

```toml
[analysis]
similarity_threshold = 0.6  # default
```

Similarity threshold for detecting AI‑modified lines. Lower values are more aggressive.

### disabled_patterns

```toml
[privacy]
disabled_patterns = ["EMAIL", "PHONE"]
```

Disable specific built-in patterns by name. Available patterns:

| Name | Description |
|------|-------------|
| `API_KEY` | Generic API keys |
| `AWS_ACCESS_KEY` | AWS access key IDs |
| `AWS_SECRET_KEY` | AWS secret access keys |
| `BEARER_TOKEN` | Bearer tokens in headers |
| `CREDIT_CARD` | Credit card numbers |
| `EMAIL` | Email addresses |
| `GITHUB_TOKEN` | GitHub personal access tokens |
| `GOOGLE_API_KEY` | Google API keys |
| `JWT` | JSON Web Tokens |
| `PASSWORD` | Password patterns |
| `PHONE` | Phone numbers |
| `PRIVATE_KEY` | Private key blocks |
| `SLACK_TOKEN` | Slack tokens |
| `SSN` | Social Security Numbers |

### audit_log

```toml
[privacy]
audit_log = true
```

Enable logging of significant events (deletions, exports, etc.) for compliance. Events are logged to `.whogitit/audit.jsonl`.

### custom_patterns

```toml
[[privacy.custom_patterns]]
name = "PATTERN_NAME"
pattern = "regex-pattern-here"
description = "Optional description"
```

Add custom redaction patterns. Each pattern needs:

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Unique identifier (appears in audit log) |
| `pattern` | Yes | Regular expression to match |
| `description` | No | Human-readable description |

## Retention Section

### max_age_days

```toml
[retention]
max_age_days = 365
```

Delete attribution data older than this many days. Set to `null` or omit for no age limit.

### auto_purge

```toml
[retention]
auto_purge = false  # default
```

When `true`, automatically apply retention policy after each commit via the post-commit hook.
Use with caution.

### retain_refs

```toml
[retention]
retain_refs = ["refs/heads/main"]  # default
```

Git refs whose commits should never have their attribution deleted, regardless of age. Useful for preserving history on main branches.

Format: Full ref names like `refs/heads/main`, `refs/tags/v1.0.0`.

### min_commits

```toml
[retention]
min_commits = 100  # default
```

Minimum number of commits to keep regardless of age. Prevents accidental deletion of all attribution data.
When enforcing this minimum, whogitit keeps the newest commits by commit time.

## Example Configurations

### Minimal (Defaults)

```toml
# No configuration needed - defaults are sensible
```

### Privacy-Focused

```toml
[privacy]
enabled = true
audit_log = true

[[privacy.custom_patterns]]
name = "EMPLOYEE_ID"
pattern = "EMP\\d{6}"
description = "Employee IDs"

[retention]
max_age_days = 90
min_commits = 50
```

### Enterprise Compliance

```toml
[privacy]
enabled = true
audit_log = true

# Custom patterns for internal systems
[[privacy.custom_patterns]]
name = "INTERNAL_API"
pattern = "int-api-[a-f0-9]{32}"

[[privacy.custom_patterns]]
name = "CUSTOMER_ID"
pattern = "CUST-\\d{8}"

[retention]
max_age_days = 365
auto_purge = false
retain_refs = [
  "refs/heads/main",
  "refs/heads/production",
  "refs/heads/staging"
]
min_commits = 500
```

### Open Source Project

```toml
[privacy]
enabled = true
# Disable email redaction for open source
disabled_patterns = ["EMAIL"]

[retention]
# Keep everything
max_age_days = null
```

## Validating Configuration

Use the `retention config` command to verify your configuration is loaded correctly:

```bash
whogitit retention config
```

Test redaction patterns:

```bash
whogitit redact-test "Test string with api_key=secret123"
```

## Environment Variables

Some settings can be overridden via environment variables:

| Variable | Description |
|----------|-------------|
| `WHOGITIT_CONFIG` | Absolute or relative path to a TOML config file (overrides repo/global discovery) |
| `WHOGITIT_BIN` | Path to whogitit binary (used by hooks) |

## See Also

- [Privacy & Redaction](./privacy.md) - Detailed redaction information
- [retention](./commands/retention.md) - Retention policy management
- [audit](./commands/audit.md) - Viewing audit logs
