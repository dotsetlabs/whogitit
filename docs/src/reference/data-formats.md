# Data Formats

This document describes the JSON schemas used by whogitit.

## Versioning

whogitit uses two versioning mechanisms:

- `version` in git-note attribution payloads (`AIAttribution`)
- `schema_version` in machine-readable CLI command output

Current values:

- `AIAttribution.version = 3`
- CLI machine output `schema_version = 1`

## AIAttribution (Git Notes)

Attribution attached to commits in `refs/notes/whogitit`:

```json
{
  "version": 3,
  "session": {
    "session_id": "7f3a4b2c-9d1e-8a7b-c3d4-e5f6a7b8c9d0",
    "model": {
      "id": "claude-opus-4-5-20251101",
      "provider": "anthropic"
    },
    "started_at": "2026-01-30T14:23:17Z",
    "prompt_count": 2,
    "used_plan_mode": false,
    "subagent_count": 0
  },
  "prompts": [
    {
      "index": 0,
      "text": "Add user authentication with bcrypt...",
      "timestamp": "2026-01-30T14:23:45Z",
      "affected_files": ["src/auth.rs", "src/main.rs"]
    }
  ],
  "files": [
    {
      "path": "src/auth.rs",
      "lines": [
        {
          "line_number": 1,
          "content": "use anyhow::Result;",
          "source": {
            "type": "AI",
            "edit_id": "8f5c3d6a-4f95-4fa9-8d11-2d54f12e6f01"
          },
          "edit_id": "8f5c3d6a-4f95-4fa9-8d11-2d54f12e6f01",
          "prompt_index": 0,
          "confidence": 1.0
        }
      ],
      "summary": {
        "total_lines": 45,
        "ai_lines": 25,
        "ai_modified_lines": 3,
        "human_lines": 2,
        "original_lines": 15,
        "unknown_lines": 0
      }
    }
  ]
}
```

### AIAttribution fields

| Field | Type | Description |
|-------|------|-------------|
| `version` | number | Attribution schema version (current: 3) |
| `session` | object | Session metadata |
| `prompts` | array | Prompt records |
| `files` | array | Per-file attribution |

### Line source in git notes

Line source is serialized as a tagged enum:

| `source.type` | Extra fields |
|---------------|--------------|
| `Original` | none |
| `AI` | `edit_id` |
| `AIModified` | `edit_id`, `similarity` |
| `Human` | none |
| `Unknown` | none |

## PendingBuffer

Temporary attribution buffer stored in `.whogitit-pending.json`:

```json
{
  "version": 3,
  "session": {
    "session_id": "7f3a4b2c-9d1e-8a7b-c3d4-e5f6a7b8c9d0",
    "model": {
      "id": "claude-opus-4-5-20251101",
      "provider": "anthropic"
    },
    "started_at": "2026-01-30T14:23:17Z",
    "prompt_count": 1,
    "prompts": [
      {
        "index": 0,
        "text": "Refactor auth middleware",
        "timestamp": "2026-01-30T14:23:45Z",
        "affected_files": ["src/auth.rs"],
        "redaction_events": []
      }
    ]
  },
  "file_histories": {
    "src/auth.rs": {
      "path": "src/auth.rs",
      "original": {
        "content": "old content",
        "content_hash": "4f9e5f2c...",
        "timestamp": "2026-01-30T14:23:44Z",
        "line_count": 10
      },
      "edits": [
        {
          "edit_id": "8f5c3d6a-4f95-4fa9-8d11-2d54f12e6f01",
          "prompt": "Refactor auth middleware",
          "prompt_index": 0,
          "tool": "Edit",
          "before": { "content": "old content", "content_hash": "4f9e5f2c...", "timestamp": "2026-01-30T14:23:44Z", "line_count": 10 },
          "after": { "content": "new content", "content_hash": "12ab34cd...", "timestamp": "2026-01-30T14:23:45Z", "line_count": 12 },
          "timestamp": "2026-01-30T14:23:45Z",
          "context": {
            "plan_mode": false,
            "agent_depth": 0
          }
        }
      ],
      "was_new_file": false
    }
  },
  "prompt_counter": 1,
  "audit_logging_enabled": false,
  "total_redactions": 0
}
```

## Machine CLI Output Schemas

Machine output is versioned with:

```json
{
  "schema_version": 1,
  "schema": "whogitit.<command>.v1"
}
```

### `blame --format json` (`whogitit.blame.v1`)

Top-level fields:

- `schema_version`, `schema`
- `file`, `revision`
- `lines[]`
- `summary`

`lines[].source` uses a stable, lowercase tagged format:

| `source.type` | Extra fields |
|---------------|--------------|
| `original` | none |
| `ai` | `edit_id` |
| `ai_modified` | `edit_id`, `similarity` |
| `human` | none |
| `unknown` | none |

### `prompt --format json` (`whogitit.prompt.v1`)

Top-level fields:

- `schema_version`, `schema`
- `query` (input reference + resolved file/line/revision)
- `line` (resolved line attribution)
- `commit`
- `prompt` (nullable)
- `session`

### `show --format json` (`whogitit.show.v1`)

Top-level fields:

- `schema_version`, `schema`
- `has_attribution`
- `commit`, `commit_short`
- `attribution_version` (present when attribution exists)
- `session`, `prompts`, `files`
- `summary` (totals)

### `summary --format json` (`whogitit.summary.v1`)

Top-level fields:

- `schema_version`, `schema`
- `commits_analyzed`, `commits_with_ai`
- `additions` (AI/AI-modified/human totals)
- `ai_percentage`
- `files`
- `models`

### `export --format json`

`export` uses `export_version`:

```json
{
  "export_version": 1,
  "exported_at": "2026-01-30T15:00:00Z",
  "date_range": {
    "since": "2026-01-01",
    "until": "2026-01-31"
  },
  "commits": [],
  "summary": {}
}
```

### `annotations --format json`

Top-level fields:

- `schema_version`, `schema` (`whogitit.annotations.v1`)
- `annotations[]`
- `summary`

## Audit Log Format

Each line in `.whogitit/audit.jsonl` is a JSON object.

Example:

```json
{"timestamp":"2026-01-30T14:23:15Z","event":"config_change","field":"retention.max_age_days","reason":"Set retention to 365 days","user":"greg","prev_hash":"1a2b3c4d5e6f70811a2b3c4d5e6f7081","event_hash":"5e6f7a8b9c0d1e2f5e6f7a8b9c0d1e2f"}
```

`prev_hash` and `event_hash` are 32-hex-character chain links (128 bits) used for tamper-evident integrity checks.

Current event types:

- `delete`
- `export`
- `retention_apply`
- `config_change`
- `redaction`
