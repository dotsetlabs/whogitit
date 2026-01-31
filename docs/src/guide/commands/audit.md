# audit

View the audit log for compliance and tracking purposes.

## Synopsis

```bash
whogitit audit [OPTIONS]
```

## Description

The `audit` command displays a log of significant events related to AI attribution data, including deletions, exports, configuration changes, and redactions. This supports compliance requirements and helps track data lifecycle.

## Options

| Option | Description |
|--------|-------------|
| `--since <DATE>` | Only show events after this date (YYYY-MM-DD) |
| `--event-type <TYPE>` | Filter by event type |
| `--json` | Output as JSON |
| `--limit <N>` | Show last N events (default: 50) |

### Event Types

| Type | Description |
|------|-------------|
| `delete` | Attribution data was deleted |
| `export` | Data was exported |
| `retention_apply` | Retention policy was applied |
| `config_change` | Configuration was modified |
| `redaction` | Sensitive data was redacted |

## Examples

### View Recent Events

```bash
whogitit audit
```

Output:

```
Audit Log
============================================================
2026-01-30 14:23:15 delete commit:abc123d user:greg - Retention policy
2026-01-30 14:23:15 delete commit:def5678 user:greg - Retention policy
2026-01-28 10:15:00 export commits:45 format:json user:greg
2026-01-25 09:00:00 retention commits:12 user:greg - Quarterly cleanup
2026-01-20 16:30:00 config user:greg - Updated max_age_days
2026-01-15 11:45:00 redaction pattern:API_KEY redactions:3
```

### Filter by Event Type

```bash
whogitit audit --event-type delete
```

Output:

```
Audit Log
============================================================
2026-01-30 14:23:15 delete commit:abc123d user:greg - Retention policy
2026-01-30 14:23:15 delete commit:def5678 user:greg - Retention policy
2026-01-15 09:00:00 delete commit:ghi9012 user:greg - Manual deletion
```

### Filter by Date

```bash
whogitit audit --since 2026-01-01
```

### JSON Output

```bash
whogitit audit --json
```

Output:

```json
[
  {
    "timestamp": "2026-01-30T14:23:15Z",
    "event": "Delete",
    "details": {
      "commit": "abc123def456...",
      "user": "greg",
      "reason": "Retention policy"
    }
  },
  {
    "timestamp": "2026-01-28T10:15:00Z",
    "event": "Export",
    "details": {
      "commit_count": 45,
      "format": "json",
      "user": "greg"
    }
  }
]
```

### Show More Events

```bash
whogitit audit --limit 100
```

## Output Details

### Event Fields

| Field | Description |
|-------|-------------|
| Timestamp | When the event occurred |
| Event type | Category of event (color-coded) |
| Details | Event-specific information |
| Reason | User-provided reason (if any) |

### Event-Specific Details

**Delete events:**
- `commit`: The commit SHA whose attribution was deleted
- `user`: Who performed the deletion
- `reason`: Why it was deleted

**Export events:**
- `commit_count`: Number of commits exported
- `format`: Export format (json/csv)
- `user`: Who performed the export

**Retention events:**
- `commits`: Number of commits affected
- `user`: Who applied the policy
- `reason`: Provided reason

**Config events:**
- `user`: Who changed the config
- Details of what changed

**Redaction events:**
- `pattern_name`: Which pattern matched
- `redaction_count`: How many matches were redacted

## Enabling Audit Logging

Audit logging must be enabled in configuration:

```toml
# .whogitit.toml
[privacy]
audit_log = true
```

If not enabled, the command will prompt you:

```
No audit log found.
Enable audit logging in .whogitit.toml: [privacy]
audit_log = true
```

## Audit Log Storage

The audit log is stored in `.whogitit/audit.log` in your repository. Each line is a JSON object representing one event.

```bash
# View raw audit log
cat .whogitit/audit.log
```

## Use Cases

### Compliance Review

Generate audit report for a time period:

```bash
whogitit audit --since 2026-01-01 --json > q1-audit.json
```

### Investigate Deletions

Find out what was deleted and why:

```bash
whogitit audit --event-type delete --limit 100
```

### Track Configuration Changes

See who changed settings:

```bash
whogitit audit --event-type config_change
```

### Monitor Redactions

Check what sensitive data is being caught:

```bash
whogitit audit --event-type redaction
```

## Notes

- Audit logging is disabled by default for privacy
- The audit log itself is not automatically purged
- Consider including `.whogitit/audit.log` in backups
- Events are appended in real-time

## See Also

- [retention](./retention.md) - Data retention management
- [Privacy & Redaction](../privacy.md) - Redaction configuration
- [Configuration](../configuration.md) - Enabling audit logging
