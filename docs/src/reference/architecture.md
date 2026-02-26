# Architecture

This document describes the internal architecture of whogitit.

## System Overview

```text
┌─────────────────────────────────────────────────────────────────────────┐
│                           Claude Code Session                            │
│                                                                          │
│    User Prompt ──► Claude ──► Edit/Write Tool ──► File Modified          │
│                                     │                                    │
└─────────────────────────────────────┼────────────────────────────────────┘
                                      │
                    ┌─────────────────┴─────────────────┐
                    │                                   │
                    ▼                                   ▼
            ┌──────────────┐                    ┌──────────────┐
            │ PreToolUse   │                    │ PostToolUse  │
            │    Hook      │                    │    Hook      │
            └──────┬───────┘                    └──────┬───────┘
                   │                                   │
                   │ Save "before"                     │ Save "after"
                   │ snapshot                          │ + prompt
                   │                                   │
                   ▼                                   ▼
            ┌─────────────────────────────────────────────────┐
            │              Pending Buffer                      │
            │         (.whogitit-pending.json)                 │
            │                                                  │
            │  • File snapshots (before/after each edit)      │
            │  • Session metadata                              │
            │  • Prompts from transcript                       │
            └─────────────────────────────────────────────────┘
                                      │
                                      │ git commit
                                      ▼
            ┌─────────────────────────────────────────────────┐
            │              Post-Commit Hook                    │
            │                                                  │
            │  1. Read pending buffer                          │
            │  2. Read committed file content                  │
            │  3. Three-way diff analysis                      │
            │  4. Generate AIAttribution                       │
            │  5. Store as git note                            │
            │  6. Clear pending buffer                         │
            └─────────────────────────────────────────────────┘
                                      │
                                      ▼
            ┌─────────────────────────────────────────────────┐
            │                 Git Notes                        │
            │         (refs/notes/whogitit)                    │
            │                                                  │
            │  Commit A ──► AIAttribution JSON                 │
            │  Commit B ──► AIAttribution JSON                 │
            │  Commit C ──► AIAttribution JSON                 │
            └─────────────────────────────────────────────────┘
```

## Module Structure

```text
src/
├── capture/           # Hook handlers and pending buffer
│   ├── hook.rs        # CaptureHook - PreToolUse/PostToolUse handling
│   ├── pending.rs     # PendingBuffer - temporary storage
│   ├── snapshot.rs    # Data structures for file snapshots
│   ├── threeway.rs    # Three-way diff algorithm
│   └── diff.rs        # Diff utilities
│
├── core/              # Attribution data models
│   ├── attribution.rs # AIAttribution, PromptInfo, SessionMetadata
│   └── blame.rs       # AIBlamer - combines git blame with notes
│
├── storage/           # Persistence layer
│   ├── notes.rs       # NotesStore - git notes read/write
│   ├── trailers.rs    # Git trailer generation
│   └── audit.rs       # AuditLog, AuditEvent
│
├── privacy/           # Data protection
│   ├── redaction.rs   # Redactor - pattern-based redaction
│   └── config.rs      # Configuration loading
│
├── cli/               # Command implementations
│   ├── blame.rs       # whogitit blame
│   ├── show.rs        # whogitit show
│   ├── prompt.rs      # whogitit prompt
│   ├── summary.rs     # whogitit summary
│   ├── export.rs      # whogitit export
│   ├── retention.rs   # whogitit retention
│   ├── audit.rs       # whogitit audit
│   ├── redact.rs      # whogitit redact-test
│   ├── copy.rs        # whogitit copy-notes
│   └── output.rs      # Output formatting
│
├── lib.rs             # Library exports
└── main.rs            # CLI entry point
```

## Core Components

### CaptureHook

Handles Claude Code hook events:

```rust,ignore
pub struct CaptureHook {
    repo_root: PathBuf,
    pending_store: PendingStore,
}

impl CaptureHook {
    pub fn handle_pre_tool_use(&self, input: &HookInput) -> Result<()>;
    pub fn handle_post_tool_use(&self, input: &HookInput) -> Result<()>;
}
```

- **PreToolUse**: Saves file content before AI edit
- **PostToolUse**: Saves file content after AI edit, extracts prompt from transcript

### PendingBuffer

Accumulates changes during a session:

```rust,ignore
pub struct PendingBuffer {
    pub version: u8,
    pub session: SessionMetadata,
    pub file_histories: HashMap<String, FileEditHistory>,
    pub prompt_counter: u32,
}
```

Stored as JSON in `.whogitit-pending.json`.

### ThreeWayAnalyzer

Core attribution algorithm:

```rust,ignore
pub struct ThreeWayAnalyzer;

impl ThreeWayAnalyzer {
    pub fn analyze(
        original: &str,
        ai_snapshots: &[ContentSnapshot],
        final_content: &str,
    ) -> Vec<LineAttribution>;
}
```

Compares:
1. Original content (before AI session)
2. AI snapshots (after each AI edit)
3. Final content (at commit time)

Produces per-line attribution.

### AIAttribution

Final attribution data structure:

```rust,ignore
pub struct AIAttribution {
    pub version: u8,
    pub session: SessionMetadata,
    pub prompts: Vec<PromptInfo>,
    pub files: Vec<FileAttributionResult>,
}

pub struct FileAttributionResult {
    pub path: String,
    pub lines: Vec<LineAttribution>,
    pub summary: AttributionSummary,
}

pub struct LineAttribution {
    pub line_number: u32,
    pub source: LineSource,
    pub prompt_index: Option<u32>,
}
```

### NotesStore

Git notes persistence:

```rust,ignore
pub struct NotesStore<'repo> {
    repo: &'repo Repository,
}

impl NotesStore {
    pub fn store_attribution(&self, commit: Oid, attr: &AIAttribution) -> Result<()>;
    pub fn fetch_attribution(&self, commit: Oid) -> Result<Option<AIAttribution>>;
    pub fn copy_attribution(&self, from: Oid, to: Oid) -> Result<()>;
    pub fn list_attributed_commits(&self) -> Result<Vec<Oid>>;
}
```

### Redactor

Privacy protection:

```rust,ignore
pub struct Redactor {
    patterns: Vec<CompiledPattern>,
}

impl Redactor {
    pub fn redact(&self, text: &str) -> String;
    pub fn redact_with_audit(&self, text: &str) -> RedactionResult;
}
```

## Data Flow Details

### 1. Capture Phase

```text
Claude Code Edit
       │
       ▼
┌─────────────────────────────────────────┐
│           whogitit-capture.sh           │
│                                         │
│  • Reads hook JSON from stdin           │
│  • Determines hook phase (pre/post)     │
│  • Extracts file path and tool name     │
│  • Calls whogitit capture --stdin       │
└─────────────────────────────────────────┘
       │
       ▼
┌─────────────────────────────────────────┐
│           CaptureHook                    │
│                                         │
│  Pre: Read current file → save snapshot │
│  Post: Read file → save snapshot        │
│        Read transcript → extract prompt │
│        Update pending buffer            │
└─────────────────────────────────────────┘
```

### 2. Commit Phase

```text
git commit
       │
       ▼
┌─────────────────────────────────────────┐
│         post-commit hook                 │
│                                         │
│  Calls: whogitit post-commit            │
└─────────────────────────────────────────┘
       │
       ▼
┌─────────────────────────────────────────┐
│         CaptureHook                      │
│                                         │
│  1. Load pending buffer                 │
│  2. Get commit info (SHA, files)        │
│  3. For each file:                      │
│     a. Get committed content            │
│     b. Get original + AI snapshots      │
│     c. Run ThreeWayAnalyzer             │
│  4. Build AIAttribution                 │
│  5. Redact prompts                      │
│  6. Store as git note                   │
│  7. Clear pending buffer                │
└─────────────────────────────────────────┘
```

### 3. Query Phase

```text
whogitit blame file.rs
       │
       ▼
┌─────────────────────────────────────────┐
│           AIBlamer                       │
│                                         │
│  1. Run git blame on file               │
│  2. For each blamed commit:             │
│     a. Fetch AIAttribution from note    │
│     b. Look up line attribution         │
│  3. Merge git blame + AI attribution    │
│  4. Return enriched blame               │
└─────────────────────────────────────────┘
```

## Three-Way Diff Algorithm

The three-way diff is the core of accurate attribution:

```text
Original (O)     AI Edit (A)      Final (F)       Attribution
============     ===========      =========       ===========
line 1           line 1           line 1          Original (in O, unchanged)
line 2           NEW LINE         NEW LINE        AI (in A, not in O, unchanged in F)
                 line 2           MODIFIED        AIModified (in A, modified in F)
line 3           line 3           MY LINE         Human (not in O or A, in F)
                                  line 3          Original (in O, unchanged)
```

Algorithm steps:

1. **Diff O→A**: Find lines added by AI
2. **Diff A→F**: Find lines modified by human
3. **Diff O→F**: Find lines added by human (not via AI)
4. **Classify each line in F**:
   - In O and unchanged → Original
   - Added in A, unchanged in F → AI
   - Added in A, modified in F → AIModified
   - Not in O or A → Human

## See Also

- [Data Formats](./data-formats.md) - JSON schemas
- [Git Notes Storage](./git-notes.md) - Notes implementation
- [Hook System](./hooks.md) - Hook details
