use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
use std::os::unix::io::AsRawFd;

#[cfg(unix)]
extern crate libc;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::capture::snapshot::{AIEdit, EditContext, FileEditHistory};
use crate::core::attribution::ModelInfo;
use crate::privacy::redaction::{RedactionEvent, Redactor};

/// Pending change buffer filename (v2 format with full snapshots)
const PENDING_FILE: &str = ".whogitit-pending.json";

/// Default maximum age in hours before a pending buffer is considered stale
/// This can be overridden via config (analysis.max_pending_age_hours)
pub const DEFAULT_MAX_PENDING_AGE_HOURS: i64 = 24;

/// Session metadata for the current AI session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Unique session identifier (UUID)
    pub session_id: String,
    /// Model information
    pub model: ModelInfo,
    /// When the session started (ISO 8601)
    pub started_at: String,
    /// Total number of prompts in this session
    pub prompt_count: u32,
    /// List of all prompts in order (for reference)
    pub prompts: Vec<PromptRecord>,
}

/// Record of a prompt in the session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptRecord {
    /// Prompt index
    pub index: u32,
    /// The prompt text (potentially redacted)
    pub text: String,
    /// Timestamp when prompt was processed
    pub timestamp: String,
    /// Files affected by this prompt
    pub affected_files: Vec<String>,
    /// Redaction audit events (if audit logging enabled)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redaction_events: Vec<RedactionEvent>,
}

/// Buffer of pending changes with full content snapshots (v3)
///
/// This version stores complete file histories to enable accurate
/// three-way diff analysis at commit time, plus redaction audit trail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingBuffer {
    /// Schema version (3 = with redaction audit)
    pub version: u8,
    /// Session information
    pub session: SessionInfo,
    /// Per-file edit histories
    pub file_histories: HashMap<String, FileEditHistory>,
    /// Current prompt index counter
    pub prompt_counter: u32,
    /// Whether redaction audit logging is enabled for this session
    #[serde(default)]
    pub audit_logging_enabled: bool,
    /// Total redaction count across all prompts
    #[serde(default)]
    pub total_redactions: u32,
}

impl PendingBuffer {
    /// Create a new pending buffer for a session
    pub fn new(session_id: &str, model_id: &str) -> Self {
        Self {
            version: 3,
            session: SessionInfo {
                session_id: session_id.to_string(),
                model: ModelInfo::claude(model_id),
                started_at: Utc::now().to_rfc3339(),
                prompt_count: 0,
                prompts: Vec::new(),
            },
            file_histories: HashMap::new(),
            prompt_counter: 0,
            audit_logging_enabled: false,
            total_redactions: 0,
        }
    }

    /// Create a new pending buffer with audit logging enabled
    pub fn new_with_audit(session_id: &str, model_id: &str) -> Self {
        let mut buffer = Self::new(session_id, model_id);
        buffer.audit_logging_enabled = true;
        buffer
    }

    /// Create with a new random session ID
    pub fn new_session(model_id: &str) -> Self {
        let session_id = Uuid::new_v4().to_string();
        Self::new(&session_id, model_id)
    }

    /// Record an AI edit with full content snapshots
    ///
    /// This stores:
    /// - The original file content (if first edit to this file)
    /// - The content before this specific edit
    /// - The content after this edit
    /// - The prompt that triggered the edit
    /// - Redaction audit events (if audit logging enabled)
    pub fn record_edit(
        &mut self,
        path: &str,
        old_content: Option<&str>,
        new_content: &str,
        tool: &str,
        prompt: &str,
        redactor: Option<&Redactor>,
    ) {
        // Redact prompt if redactor provided, with audit if enabled
        let (redacted_prompt, redaction_events) = match redactor {
            Some(r) if self.audit_logging_enabled => {
                let result = r.redact_with_audit(prompt);
                self.total_redactions += result.redaction_count as u32;
                (result.text, result.events)
            }
            Some(r) => (r.redact(prompt), Vec::new()),
            None => (prompt.to_string(), Vec::new()),
        };

        let prompt_index = self.prompt_counter;
        self.prompt_counter += 1;
        self.session.prompt_count = self.prompt_counter;

        // Record the prompt with optional redaction events
        self.session.prompts.push(PromptRecord {
            index: prompt_index,
            text: redacted_prompt.clone(),
            timestamp: Utc::now().to_rfc3339(),
            affected_files: vec![path.to_string()],
            redaction_events,
        });

        // Get or create file history
        let history = self
            .file_histories
            .entry(path.to_string())
            .or_insert_with(|| FileEditHistory::new(path, old_content));

        // Determine before content
        let before_content = if history.edits.is_empty() {
            // First edit - use original or provided old_content
            old_content.unwrap_or("")
        } else {
            // Subsequent edit - use the after content from last edit
            &history.latest_ai_content().content
        };

        // Create the edit record
        let edit = AIEdit::new(
            &redacted_prompt,
            prompt_index,
            tool,
            before_content,
            new_content,
        );

        history.add_edit(edit);
    }

    /// Record an AI edit with context (plan mode, subagent, etc.)
    #[allow(clippy::too_many_arguments)]
    pub fn record_edit_with_context(
        &mut self,
        path: &str,
        old_content: Option<&str>,
        new_content: &str,
        tool: &str,
        prompt: &str,
        redactor: Option<&Redactor>,
        context: Option<EditContext>,
    ) {
        // Redact prompt if redactor provided, with audit if enabled
        let (redacted_prompt, redaction_events) = match redactor {
            Some(r) if self.audit_logging_enabled => {
                let result = r.redact_with_audit(prompt);
                self.total_redactions += result.redaction_count as u32;
                (result.text, result.events)
            }
            Some(r) => (r.redact(prompt), Vec::new()),
            None => (prompt.to_string(), Vec::new()),
        };

        let prompt_index = self.prompt_counter;
        self.prompt_counter += 1;
        self.session.prompt_count = self.prompt_counter;

        // Record the prompt with optional redaction events
        self.session.prompts.push(PromptRecord {
            index: prompt_index,
            text: redacted_prompt.clone(),
            timestamp: Utc::now().to_rfc3339(),
            affected_files: vec![path.to_string()],
            redaction_events,
        });

        // Get or create file history
        let history = self
            .file_histories
            .entry(path.to_string())
            .or_insert_with(|| FileEditHistory::new(path, old_content));

        // Determine before content
        let before_content = if history.edits.is_empty() {
            old_content.unwrap_or("")
        } else {
            &history.latest_ai_content().content
        };

        // Create the edit record with context
        let edit = match context {
            Some(ctx) => AIEdit::with_context(
                &redacted_prompt,
                prompt_index,
                tool,
                before_content,
                new_content,
                ctx,
            ),
            None => AIEdit::new(
                &redacted_prompt,
                prompt_index,
                tool,
                before_content,
                new_content,
            ),
        };

        history.add_edit(edit);
    }

    /// Get file history for a path
    pub fn get_file_history(&self, path: &str) -> Option<&FileEditHistory> {
        self.file_histories.get(path)
    }

    /// Get all file paths with pending changes
    pub fn files(&self) -> Vec<&str> {
        self.file_histories.keys().map(|s| s.as_str()).collect()
    }

    /// Check if there are pending changes
    pub fn has_changes(&self) -> bool {
        !self.file_histories.is_empty()
    }

    /// Get total number of AI edits across all files
    pub fn total_edits(&self) -> usize {
        self.file_histories.values().map(|h| h.edits.len()).sum()
    }

    /// Get total number of files with changes
    pub fn file_count(&self) -> usize {
        self.file_histories.len()
    }

    /// Estimate total AI-generated lines (rough count)
    pub fn total_lines(&self) -> u32 {
        self.file_histories
            .values()
            .map(|h| {
                h.edits
                    .iter()
                    .map(|e| {
                        // Count lines added in each edit
                        let before_lines = e.before.line_count;
                        let after_lines = e.after.line_count;
                        after_lines.saturating_sub(before_lines) as u32
                    })
                    .sum::<u32>()
            })
            .sum()
    }

    /// Clear all pending changes
    pub fn clear(&mut self) {
        self.file_histories.clear();
        self.session.prompts.clear();
    }

    /// Get a prompt by index
    pub fn get_prompt(&self, index: u32) -> Option<&PromptRecord> {
        self.session.prompts.iter().find(|p| p.index == index)
    }

    /// Check if this buffer is stale (older than specified hours)
    pub fn is_stale(&self) -> bool {
        self.is_stale_hours(DEFAULT_MAX_PENDING_AGE_HOURS)
    }

    /// Check if this buffer is stale with a custom hour threshold
    pub fn is_stale_hours(&self, max_hours: i64) -> bool {
        if let Ok(started) = DateTime::parse_from_rfc3339(&self.session.started_at) {
            let age = Utc::now().signed_duration_since(started);
            age > Duration::hours(max_hours)
        } else {
            // If we can't parse the timestamp, consider it stale
            true
        }
    }

    /// Get the age of this buffer in human-readable format
    pub fn age_string(&self) -> String {
        if let Ok(started) = DateTime::parse_from_rfc3339(&self.session.started_at) {
            let age = Utc::now().signed_duration_since(started);
            if age.num_hours() > 0 {
                format!("{} hours ago", age.num_hours())
            } else if age.num_minutes() > 0 {
                format!("{} minutes ago", age.num_minutes())
            } else {
                "just now".to_string()
            }
        } else {
            "unknown".to_string()
        }
    }

    /// Validate buffer integrity
    pub fn validate(&self) -> Result<(), String> {
        // Check version (accept v2 for backwards compatibility, v3 for new features)
        if self.version != 2 && self.version != 3 {
            return Err(format!("Unsupported buffer version: {}", self.version));
        }

        // Check session ID is valid UUID
        if Uuid::parse_str(&self.session.session_id).is_err() {
            return Err("Invalid session ID format".to_string());
        }

        // Check prompt count matches prompts
        if self.session.prompt_count != self.session.prompts.len() as u32 {
            return Err(format!(
                "Prompt count mismatch: {} vs {}",
                self.session.prompt_count,
                self.session.prompts.len()
            ));
        }

        // Check each file history has at least one edit
        for (path, history) in &self.file_histories {
            if history.edits.is_empty() {
                return Err(format!("File '{}' has no edits", path));
            }
        }

        Ok(())
    }
}

/// Lock file name for concurrent access protection
const LOCK_FILE: &str = ".whogitit-pending.lock";

/// Acquire an exclusive file lock (Unix only)
/// Returns a guard that releases the lock when dropped
#[cfg(unix)]
fn acquire_lock(lock_path: &Path) -> Result<File> {
    use std::io::ErrorKind;

    // Create or open lock file
    let lock_file = File::create(lock_path).context("Failed to create lock file")?;

    // Try to acquire exclusive lock with timeout
    // Use non-blocking first to detect contention
    let fd = lock_file.as_raw_fd();

    // First try non-blocking
    let result = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };

    if result != 0 {
        let err = std::io::Error::last_os_error();
        if err.kind() == ErrorKind::WouldBlock {
            eprintln!(
                "whogitit: Warning - another process is accessing the pending buffer, waiting..."
            );
            // Now do a blocking lock
            let result = unsafe { libc::flock(fd, libc::LOCK_EX) };
            if result != 0 {
                return Err(std::io::Error::last_os_error())
                    .context("Failed to acquire lock on pending buffer");
            }
        } else {
            return Err(err).context("Failed to acquire lock on pending buffer");
        }
    }

    Ok(lock_file)
}

/// No-op lock acquisition for non-Unix platforms
#[cfg(not(unix))]
fn acquire_lock(_lock_path: &Path) -> Result<File> {
    // On non-Unix platforms, create a marker file but don't actually lock
    // This provides some protection via file existence check
    File::create(_lock_path).context("Failed to create lock file")
}

/// Release a file lock
#[cfg(unix)]
fn release_lock(lock_file: &File) {
    let fd = lock_file.as_raw_fd();
    unsafe {
        libc::flock(fd, libc::LOCK_UN);
    }
}

/// No-op lock release for non-Unix platforms
#[cfg(not(unix))]
fn release_lock(_lock_file: &File) {
    // No-op on non-Unix
}

/// Manager for persisting pending buffer to disk
pub struct PendingStore {
    /// Path to the pending file
    file_path: PathBuf,
    /// Path to the repo root
    repo_root: PathBuf,
    /// Path to the lock file
    lock_path: PathBuf,
}

impl PendingStore {
    /// Create a store for the given repo root
    pub fn new(repo_root: &Path) -> Self {
        Self {
            file_path: repo_root.join(PENDING_FILE),
            repo_root: repo_root.to_path_buf(),
            lock_path: repo_root.join(LOCK_FILE),
        }
    }

    /// Load pending buffer from disk, with stale detection
    pub fn load(&self) -> Result<Option<PendingBuffer>> {
        self.load_with_max_age(DEFAULT_MAX_PENDING_AGE_HOURS)
    }

    /// Load pending buffer with a custom stale threshold
    pub fn load_with_max_age(&self, max_pending_age_hours: i64) -> Result<Option<PendingBuffer>> {
        if !self.file_path.exists() {
            return Ok(None);
        }

        // Acquire lock for concurrent access protection
        let lock_file = acquire_lock(&self.lock_path)?;

        let content =
            fs::read_to_string(&self.file_path).context("Failed to read pending buffer file")?;

        // Release lock before returning
        release_lock(&lock_file);

        // Try to parse as v2 format
        match serde_json::from_str::<PendingBuffer>(&content) {
            Ok(buffer) => {
                // Validate buffer integrity
                if let Err(e) = buffer.validate() {
                    eprintln!(
                        "whogitit: Warning - pending buffer validation failed: {}",
                        e
                    );
                    eprintln!("whogitit: The pending buffer may be corrupted. Run 'whogitit clear' to reset.");
                }

                // Warn if buffer is stale
                if buffer.is_stale_hours(max_pending_age_hours) {
                    eprintln!(
                        "whogitit: Warning - pending buffer is stale (started {})",
                        buffer.age_string()
                    );
                    eprintln!("whogitit: Consider running 'whogitit clear' if these changes are no longer relevant.");
                }

                Ok(Some(buffer))
            }
            Err(e) => {
                eprintln!("whogitit: Warning - failed to parse pending buffer: {}", e);

                // Create a backup of the corrupted file for recovery
                let backup_name = format!(
                    ".whogitit-pending.corrupted.{}",
                    chrono::Utc::now().format("%Y%m%d-%H%M%S")
                );
                let backup_path = self.repo_root.join(&backup_name);
                if let Err(backup_err) = fs::copy(&self.file_path, &backup_path) {
                    eprintln!(
                        "whogitit: Warning - failed to backup corrupted file: {}",
                        backup_err
                    );
                } else {
                    eprintln!(
                        "whogitit: Corrupted file backed up to: {}",
                        backup_path.display()
                    );

                    // Set restrictive permissions on backup
                    #[cfg(unix)]
                    {
                        if let Ok(metadata) = fs::metadata(&backup_path) {
                            let mut perms = metadata.permissions();
                            perms.set_mode(0o600);
                            let _ = fs::set_permissions(&backup_path, perms);
                        }
                    }
                }

                eprintln!("whogitit: Run 'whogitit clear' to reset and start fresh.");
                // Return None to allow fresh start
                Ok(None)
            }
        }
    }

    /// Load buffer without warnings (for status checks)
    pub fn load_quiet(&self) -> Result<Option<PendingBuffer>> {
        if !self.file_path.exists() {
            return Ok(None);
        }

        let content =
            fs::read_to_string(&self.file_path).context("Failed to read pending buffer file")?;

        match serde_json::from_str::<PendingBuffer>(&content) {
            Ok(buffer) => Ok(Some(buffer)),
            Err(_) => Ok(None),
        }
    }

    /// Save pending buffer to disk atomically
    ///
    /// Uses write-to-temp-then-rename pattern to prevent corruption
    /// if the process is interrupted during write.
    /// Also uses file locking for concurrent access protection.
    pub fn save(&self, buffer: &PendingBuffer) -> Result<()> {
        // Validate before saving
        if let Err(e) = buffer.validate() {
            anyhow::bail!("Cannot save invalid buffer: {}", e);
        }

        // Acquire lock for concurrent access protection
        let lock_file = acquire_lock(&self.lock_path)?;

        let content =
            serde_json::to_string_pretty(buffer).context("Failed to serialize pending buffer")?;

        // Write to temporary file first
        let temp_path = self.repo_root.join(".whogitit-pending.tmp");

        let mut temp_file =
            File::create(&temp_path).context("Failed to create temporary pending buffer file")?;

        temp_file
            .write_all(content.as_bytes())
            .context("Failed to write to temporary pending buffer file")?;

        temp_file
            .sync_all()
            .context("Failed to sync temporary pending buffer file")?;

        drop(temp_file);

        // Atomically rename temp file to final path
        fs::rename(&temp_path, &self.file_path)
            .context("Failed to rename temporary pending buffer file")?;

        // Set restrictive permissions (0600) - pending buffer contains sensitive data
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&self.file_path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&self.file_path, perms)
                .context("Failed to set permissions on pending buffer file")?;
        }

        // Release lock
        release_lock(&lock_file);

        Ok(())
    }

    /// Delete the pending buffer file
    pub fn delete(&self) -> Result<()> {
        // Also clean up any leftover temp file
        let temp_path = self.repo_root.join(".whogitit-pending.tmp");
        if temp_path.exists() {
            let _ = fs::remove_file(&temp_path);
        }

        if self.file_path.exists() {
            fs::remove_file(&self.file_path).context("Failed to delete pending buffer file")?;
        }
        Ok(())
    }

    /// Check if pending file exists
    pub fn exists(&self) -> bool {
        self.file_path.exists()
    }

    /// Get the file path
    pub fn path(&self) -> &Path {
        &self.file_path
    }

    /// Create a backup of the current pending buffer
    pub fn backup(&self) -> Result<Option<PathBuf>> {
        if !self.file_path.exists() {
            return Ok(None);
        }

        let backup_name = format!(
            ".whogitit-pending.backup.{}",
            Utc::now().format("%Y%m%d-%H%M%S")
        );
        let backup_path = self.repo_root.join(backup_name);

        fs::copy(&self.file_path, &backup_path)
            .context("Failed to create backup of pending buffer")?;

        // Set restrictive permissions (0600) on backup - contains sensitive data
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&backup_path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&backup_path, perms)
                .context("Failed to set permissions on backup file")?;
        }

        Ok(Some(backup_path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_record_edit_new_file() {
        let mut buffer = PendingBuffer::new("test-session", "claude-opus-4-5-20251101");

        buffer.record_edit(
            "src/new.rs",
            None, // New file
            "fn new_function() {}\n",
            "Write",
            "Create new file",
            None,
        );

        assert!(buffer.has_changes());
        assert_eq!(buffer.file_count(), 1);

        let history = buffer.get_file_history("src/new.rs").unwrap();
        assert!(history.was_new_file);
        assert_eq!(history.edits.len(), 1);
        assert_eq!(history.edits[0].prompt, "Create new file");
        assert_eq!(history.edits[0].after.content, "fn new_function() {}\n");
    }

    #[test]
    fn test_record_edit_existing_file() {
        let mut buffer = PendingBuffer::new("test-session", "claude-opus-4-5-20251101");

        buffer.record_edit(
            "src/main.rs",
            Some("fn main() {}\n"),
            "fn main() {\n    println!(\"Hello\");\n}\n",
            "Edit",
            "Add println statement",
            None,
        );

        let history = buffer.get_file_history("src/main.rs").unwrap();
        assert!(!history.was_new_file);
        assert_eq!(history.original.content, "fn main() {}\n");
        assert_eq!(history.edits[0].before.content, "fn main() {}\n");
        assert!(history.edits[0].after.content.contains("println"));
    }

    #[test]
    fn test_multiple_edits_same_file() {
        let mut buffer = PendingBuffer::new("test-session", "claude-opus-4-5-20251101");

        // First edit
        buffer.record_edit(
            "test.rs",
            Some("line1\n"),
            "line1\nline2\n",
            "Edit",
            "Add line2",
            None,
        );

        // Second edit
        buffer.record_edit(
            "test.rs",
            None, // Not needed for subsequent edits
            "line1\nline2\nline3\n",
            "Edit",
            "Add line3",
            None,
        );

        let history = buffer.get_file_history("test.rs").unwrap();
        assert_eq!(history.edits.len(), 2);
        assert_eq!(history.original.content, "line1\n");
        assert_eq!(history.edits[0].prompt_index, 0);
        assert_eq!(history.edits[1].prompt_index, 1);

        // Second edit's before should be first edit's after
        assert_eq!(history.edits[1].before.content, "line1\nline2\n");
        assert_eq!(history.edits[1].after.content, "line1\nline2\nline3\n");
    }

    #[test]
    fn test_prompt_tracking() {
        let mut buffer = PendingBuffer::new("test-session", "claude-opus-4-5-20251101");

        buffer.record_edit("a.rs", None, "a\n", "Write", "prompt 1", None);
        buffer.record_edit("b.rs", None, "b\n", "Write", "prompt 2", None);

        assert_eq!(buffer.session.prompt_count, 2);
        assert_eq!(buffer.session.prompts.len(), 2);
        assert_eq!(buffer.session.prompts[0].text, "prompt 1");
        assert_eq!(buffer.session.prompts[1].text, "prompt 2");
    }

    #[test]
    fn test_store_roundtrip() {
        let dir = TempDir::new().unwrap();
        let store = PendingStore::new(dir.path());

        // Use a valid UUID for session ID
        let session_id = Uuid::new_v4().to_string();
        let mut buffer = PendingBuffer::new(&session_id, "claude-opus-4-5-20251101");
        buffer.record_edit(
            "test.rs",
            Some("before\n"),
            "after\n",
            "Edit",
            "test prompt",
            None,
        );

        store.save(&buffer).unwrap();
        assert!(store.exists());

        let loaded = store.load_quiet().unwrap().unwrap();
        assert_eq!(loaded.session.session_id, session_id);
        assert_eq!(loaded.file_count(), 1);

        let history = loaded.get_file_history("test.rs").unwrap();
        assert_eq!(history.original.content, "before\n");
        assert_eq!(history.edits[0].after.content, "after\n");

        store.delete().unwrap();
        assert!(!store.exists());
    }

    #[test]
    fn test_redaction() {
        use crate::privacy::Redactor;

        let mut buffer = PendingBuffer::new("test-session", "claude-opus-4-5-20251101");
        let redactor = Redactor::default_patterns();

        buffer.record_edit(
            "config.rs",
            None,
            "api_key = \"secret\"\n",
            "Write",
            "Set api_key = sk-12345 for auth",
            Some(&redactor),
        );

        let history = buffer.get_file_history("config.rs").unwrap();
        assert!(!history.edits[0].prompt.contains("sk-12345"));
        assert!(history.edits[0].prompt.contains("[REDACTED]"));
    }
}
