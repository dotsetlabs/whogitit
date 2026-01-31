# Data Formats

This document describes the JSON schemas used by whogitit.

## AIAttribution (Git Notes)

The primary data structure stored in git notes:

```json
{
  "schema_version": 2,
  "session": {
    "session_id": "7f3a4b2c-9d1e-8a7b-c3d4-e5f6a7b8c9d0",
    "model": {
      "id": "claude-opus-4-5-20251101",
      "provider": "anthropic"
    },
    "started_at": "2026-01-30T14:23:17Z",
    "cwd": "/path/to/project"
  },
  "prompts": [
    {
      "index": 0,
      "text": "Add user authentication with bcrypt...",
      "affected_files": ["src/auth.rs", "src/main.rs"],
      "timestamp": "2026-01-30T14:23:45Z"
    }
  ],
  "files": [
    {
      "path": "src/auth.rs",
      "lines": [
        {"line_number": 1, "source": "AI", "prompt_index": 0},
        {"line_number": 2, "source": "AI", "prompt_index": 0},
        {"line_number": 3, "source": "AIModified", "prompt_index": 0},
        {"line_number": 4, "source": "Human", "prompt_index": null},
        {"line_number": 5, "source": "Original", "prompt_index": null}
      ],
      "summary": {
        "ai_lines": 2,
        "ai_modified_lines": 1,
        "human_lines": 1,
        "original_lines": 1
      }
    }
  ]
}
```

### Schema Fields

| Field | Type | Description |
|-------|------|-------------|
| `schema_version` | number | Format version (currently 2) |
| `session` | object | AI session information |
| `prompts` | array | Prompts used in this commit |
| `files` | array | Per-file attribution |

### Session Object

| Field | Type | Description |
|-------|------|-------------|
| `session_id` | string | Unique session identifier |
| `model.id` | string | Model identifier |
| `model.provider` | string | Model provider |
| `started_at` | string | ISO 8601 timestamp |
| `cwd` | string | Working directory |

### Prompt Object

| Field | Type | Description |
|-------|------|-------------|
| `index` | number | Order within session |
| `text` | string | Prompt text (may be redacted) |
| `affected_files` | array | Files changed by this prompt |
| `timestamp` | string | When prompt was given |

### LineAttribution Object

| Field | Type | Description |
|-------|------|-------------|
| `line_number` | number | 1-indexed line number |
| `source` | string | `AI`, `AIModified`, `Human`, `Original`, `Unknown` |
| `prompt_index` | number\|null | Index of generating prompt |

## PendingBuffer

Temporary storage during editing session:

```json
{
  "schema_version": 2,
  "session": {
    "session_id": "7f3a4b2c-9d1e-8a7b-c3d4-e5f6a7b8c9d0",
    "model": {
      "id": "claude-opus-4-5-20251101",
      "provider": "anthropic"
    },
    "started_at": "2026-01-30T14:23:17Z",
    "cwd": "/path/to/project"
  },
  "files": {
    "src/auth.rs": {
      "original_content": "// Original file content\n...",
      "edits": [
        {
          "content": "// After first AI edit\n...",
          "prompt_index": 0,
          "timestamp": "2026-01-30T14:23:45Z",
          "tool": "Edit"
        },
        {
          "content": "// After second AI edit\n...",
          "prompt_index": 1,
          "timestamp": "2026-01-30T14:25:00Z",
          "tool": "Write"
        }
      ]
    }
  },
  "prompts": [
    {
      "index": 0,
      "text": "Add user authentication...",
      "affected_files": ["src/auth.rs"],
      "timestamp": "2026-01-30T14:23:45Z"
    }
  ]
}
```

### FileEditHistory

| Field | Type | Description |
|-------|------|-------------|
| `original_content` | string | File content before AI session |
| `edits` | array | Sequence of AI edits |

### AIEdit

| Field | Type | Description |
|-------|------|-------------|
| `content` | string | File content after this edit |
| `prompt_index` | number | Which prompt triggered this |
| `timestamp` | string | When edit occurred |
| `tool` | string | `Edit` or `Write` |

## Export Format

Output of `whogitit export`:

```json
{
  "export_version": 1,
  "exported_at": "2026-01-30T15:00:00Z",
  "date_range": {
    "since": "2026-01-01",
    "until": "2026-01-31"
  },
  "commits": [
    {
      "commit_id": "abc123def456789...",
      "commit_short": "abc123d",
      "message": "Add user authentication",
      "author": "Greg King",
      "committed_at": "2026-01-30T14:30:00Z",
      "session_id": "7f3a4b2c-9d1e-8a7b-c3d4-e5f6a7b8c9d0",
      "model": "claude-opus-4-5-20251101",
      "ai_lines": 145,
      "ai_modified_lines": 12,
      "human_lines": 43,
      "original_lines": 50,
      "files": ["src/auth.rs", "src/main.rs"],
      "prompts": [
        {
          "index": 0,
          "text": "Add user authentication...",
          "affected_files": ["src/auth.rs"]
        }
      ]
    }
  ],
  "summary": {
    "total_commits": 10,
    "commits_with_ai": 7,
    "total_ai_lines": 523,
    "total_ai_modified_lines": 45,
    "total_human_lines": 128,
    "total_original_lines": 89,
    "total_prompts": 15
  }
}
```

## Audit Log Format

Each line in `.whogitit/audit.log` is a JSON object:

```json
{"timestamp":"2026-01-30T14:23:15Z","event":"Delete","details":{"commit":"abc123d","user":"greg","reason":"Retention policy"}}
{"timestamp":"2026-01-28T10:15:00Z","event":"Export","details":{"commit_count":45,"format":"json","user":"greg"}}
{"timestamp":"2026-01-25T09:00:00Z","event":"RetentionApply","details":{"commits":12,"user":"greg","reason":"Quarterly"}}
{"timestamp":"2026-01-20T16:30:00Z","event":"ConfigChange","details":{"user":"greg","field":"max_age_days"}}
{"timestamp":"2026-01-15T11:45:00Z","event":"Redaction","details":{"pattern_name":"API_KEY","redaction_count":3}}
```

### Event Types

| Type | Description | Details Fields |
|------|-------------|----------------|
| `Delete` | Attribution deleted | `commit`, `user`, `reason` |
| `Export` | Data exported | `commit_count`, `format`, `user` |
| `RetentionApply` | Retention policy applied | `commits`, `user`, `reason` |
| `ConfigChange` | Configuration changed | `user`, `field` |
| `Redaction` | Sensitive data redacted | `pattern_name`, `redaction_count` |

## Configuration Format

`.whogitit.toml`:

```toml
[privacy]
enabled = true
use_builtin_patterns = true
disabled_patterns = ["EMAIL"]
audit_log = true

[[privacy.custom_patterns]]
name = "INTERNAL_ID"
pattern = "INT-[A-Z0-9]{8}"
description = "Internal IDs"

[retention]
max_age_days = 365
auto_purge = false
retain_refs = ["refs/heads/main"]
min_commits = 100
```

See [Configuration](../guide/configuration.md) for full reference.

## Hook Input Format

JSON passed to hooks via stdin:

```json
{
  "tool_name": "Edit",
  "tool_input": {
    "file_path": "/path/to/file.rs",
    "old_string": "...",
    "new_string": "..."
  },
  "transcript_path": "/tmp/claude-transcript-xyz.jsonl"
}
```

For `Write` tool:

```json
{
  "tool_name": "Write",
  "tool_input": {
    "file_path": "/path/to/file.rs",
    "content": "..."
  },
  "transcript_path": "/tmp/claude-transcript-xyz.jsonl"
}
```

## Version History

| Version | Changes |
|---------|---------|
| 2 | Current version. Full content snapshots. |
| 1 | Initial version. Diff-based storage. (deprecated) |

## See Also

- [Architecture](./architecture.md) - System design
- [Git Notes Storage](./git-notes.md) - Notes implementation
