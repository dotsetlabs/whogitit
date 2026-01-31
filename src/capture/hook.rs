use std::env;
use std::path::Path;

use anyhow::{Context, Result};
use git2::Repository;
use serde::{Deserialize, Serialize};

use crate::capture::pending::{PendingBuffer, PendingStore};
use crate::capture::threeway::ThreeWayAnalyzer;
use crate::core::attribution::{AIAttribution, PromptInfo, SessionMetadata};
use crate::privacy::{Redactor, WhogititConfig};
use crate::storage::notes::NotesStore;

/// Environment variable for session ID
const ENV_SESSION_ID: &str = "WHOGITIT_SESSION_ID";
/// Environment variable for model ID
const ENV_MODEL_ID: &str = "WHOGITIT_MODEL_ID";
/// Default model if not specified
const DEFAULT_MODEL: &str = "claude-opus-4-5-20251101";

/// Context from Claude Code transcript
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HookContext {
    /// Whether the edit was made in plan mode
    #[serde(default)]
    pub plan_mode: bool,
    /// Whether this is from a subagent
    #[serde(default)]
    pub is_subagent: bool,
    /// Agent nesting depth (0=main, 1+=subagent)
    #[serde(default)]
    pub agent_depth: u8,
    /// Subagent ID if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent_id: Option<String>,
}

/// Input from Claude Code hook for file changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookInput {
    /// The tool being called (Edit, Write)
    pub tool: String,
    /// File path being modified
    pub file_path: String,
    /// The current user prompt/context
    pub prompt: String,
    /// Old file content (None for new files)
    pub old_content: Option<String>,
    /// New file content
    pub new_content: String,
    /// Context from transcript (plan mode, subagent, etc.)
    #[serde(default)]
    pub context: Option<HookContext>,
}

/// Claude Code hook handler
pub struct CaptureHook {
    /// Repository root path
    repo_root: std::path::PathBuf,
    /// Privacy redactor
    redactor: Redactor,
    /// Whether audit logging is enabled
    audit_enabled: bool,
}

impl CaptureHook {
    /// Create a new capture hook for a repository
    pub fn new(repo_path: &Path) -> Result<Self> {
        let repo_root = repo_path.to_path_buf();

        // Load config and build redactor
        let config = WhogititConfig::load(&repo_root).unwrap_or_default();
        let redactor = config.privacy.build_redactor();
        let audit_enabled = config.privacy.audit_log;

        Ok(Self {
            repo_root,
            redactor,
            audit_enabled,
        })
    }

    /// Get or create session ID
    fn get_session_id() -> String {
        env::var(ENV_SESSION_ID).unwrap_or_else(|_| uuid::Uuid::new_v4().to_string())
    }

    /// Get model ID from environment
    fn get_model_id() -> String {
        env::var(ENV_MODEL_ID).unwrap_or_else(|_| DEFAULT_MODEL.to_string())
    }

    /// Handle a file change from Claude Code
    pub fn on_file_change(&self, input: HookInput) -> Result<()> {
        let store = PendingStore::new(&self.repo_root);

        // Load or create pending buffer
        let mut buffer = match store.load()? {
            Some(b) => {
                // Check if we should start a new session
                // (different session ID in env means new session)
                let current_session = Self::get_session_id();
                if b.session.session_id != current_session && env::var(ENV_SESSION_ID).is_ok() {
                    // New session ID explicitly set, start fresh
                    // But first, warn about uncommitted changes
                    if b.has_changes() {
                        eprintln!(
                            "whogitit: Warning - discarding {} uncommitted edits from previous session",
                            b.total_edits()
                        );
                    }
                    PendingBuffer::new(&current_session, &Self::get_model_id())
                } else {
                    b
                }
            }
            None => {
                let mut buffer = PendingBuffer::new(&Self::get_session_id(), &Self::get_model_id());
                buffer.audit_logging_enabled = self.audit_enabled;
                buffer
            }
        };

        // Make path relative to repo root
        let relative_path = self.make_relative_path(&input.file_path)?;

        // Validate input
        if relative_path.is_empty() {
            anyhow::bail!("Empty file path");
        }
        if input.new_content.is_empty() && input.tool != "Delete" {
            eprintln!("whogitit: Warning - empty new_content for non-delete operation");
        }

        // Determine old content: use provided value, or fall back to git HEAD
        let old_content = match input.old_content {
            Some(content) => Some(content),
            None => {
                // Try to get content from git HEAD for existing files
                self.get_content_from_git_head(&relative_path)
            }
        };

        // Build edit context from hook input
        let edit_context =
            input
                .context
                .as_ref()
                .map(|ctx| crate::capture::snapshot::EditContext {
                    plan_mode: ctx.plan_mode,
                    subagent_id: ctx.subagent_id.clone(),
                    agent_depth: ctx.agent_depth,
                    plan_step: None,
                });

        // Record the edit with full content snapshots
        buffer.record_edit_with_context(
            &relative_path,
            old_content.as_deref(),
            &input.new_content,
            &input.tool,
            &input.prompt,
            Some(&self.redactor),
            edit_context,
        );

        // Save buffer with atomic write
        store.save(&buffer)?;

        Ok(())
    }

    /// Get file content from git HEAD (the last committed version)
    fn get_content_from_git_head(&self, path: &str) -> Option<String> {
        let repo = Repository::open(&self.repo_root).ok()?;
        let head = repo.head().ok()?.peel_to_commit().ok()?;
        let tree = head.tree().ok()?;
        let entry = tree.get_path(std::path::Path::new(path)).ok()?;
        let blob = repo.find_blob(entry.id()).ok()?;
        let content = std::str::from_utf8(blob.content()).ok()?;
        Some(content.to_string())
    }

    /// Handle post-commit: perform three-way analysis, attach notes, and clean up
    pub fn on_post_commit(&self) -> Result<Option<AIAttribution>> {
        let store = PendingStore::new(&self.repo_root);

        // Load pending buffer
        let buffer = match store.load()? {
            Some(b) if b.has_changes() => b,
            _ => return Ok(None),
        };

        // Open repo and get HEAD commit
        let repo = Repository::open(&self.repo_root).context("Failed to open repository")?;
        let head = repo
            .head()
            .context("Failed to get HEAD")?
            .peel_to_commit()
            .context("Failed to get HEAD commit")?;

        // Analyze each file with three-way diff against committed content
        let mut file_results = Vec::new();
        let tree = head.tree()?;

        for (path, history) in &buffer.file_histories {
            // Get the committed content for this file
            let committed_content = match tree.get_path(std::path::Path::new(path)) {
                Ok(entry) => {
                    let blob = repo.find_blob(entry.id())?;
                    String::from_utf8_lossy(blob.content()).to_string()
                }
                Err(_) => {
                    // File might have been deleted or not staged
                    continue;
                }
            };

            // Perform three-way analysis
            let result = ThreeWayAnalyzer::analyze_with_diff(history, &committed_content);
            file_results.push(result);
        }

        // Compute extended session metadata
        let used_plan_mode = buffer
            .file_histories
            .values()
            .flat_map(|h| h.edits.iter())
            .any(|e| e.context.plan_mode);

        let subagent_count = buffer
            .file_histories
            .values()
            .flat_map(|h| h.edits.iter())
            .filter(|e| e.context.agent_depth > 0)
            .count() as u32;

        // Create attribution with full analysis
        let attribution = AIAttribution {
            version: 3,
            session: SessionMetadata {
                session_id: buffer.session.session_id.clone(),
                model: buffer.session.model.clone(),
                started_at: buffer.session.started_at.clone(),
                prompt_count: buffer.session.prompt_count,
                used_plan_mode,
                subagent_count,
            },
            prompts: buffer
                .session
                .prompts
                .iter()
                .map(|p| PromptInfo {
                    index: p.index,
                    text: p.text.clone(),
                    timestamp: p.timestamp.clone(),
                    affected_files: p.affected_files.clone(),
                })
                .collect(),
            files: file_results,
        };

        // Store as git note
        let notes_store = NotesStore::new(&repo)?;
        notes_store.store_attribution(head.id(), &attribution)?;

        // Clean up pending file
        store.delete()?;

        // Log summary
        let total_ai = attribution
            .files
            .iter()
            .map(|f| f.summary.ai_lines + f.summary.ai_modified_lines)
            .sum::<usize>();
        let total_human = attribution
            .files
            .iter()
            .map(|f| f.summary.human_lines)
            .sum::<usize>();

        eprintln!(
            "whogitit: Attached attribution - {} AI lines, {} human lines across {} files",
            total_ai,
            total_human,
            attribution.files.len()
        );

        Ok(Some(attribution))
    }

    /// Make a path relative to the repo root
    fn make_relative_path(&self, path: &str) -> Result<String> {
        let abs_path = if Path::new(path).is_absolute() {
            Path::new(path).to_path_buf()
        } else {
            self.repo_root.join(path)
        };

        let relative = abs_path
            .strip_prefix(&self.repo_root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| path.to_string());

        Ok(relative)
    }

    /// Get current pending status
    pub fn status(&self) -> Result<PendingStatus> {
        let store = PendingStore::new(&self.repo_root);

        // Use quiet load to avoid spurious warnings during status check
        match store.load_quiet()? {
            Some(buffer) => {
                let session_id = buffer.session.session_id.clone();
                let file_count = buffer.file_count();
                let line_count = buffer.total_lines();
                let edit_count = buffer.total_edits();
                let prompt_count = buffer.session.prompt_count;
                let has_pending = buffer.has_changes();
                let is_stale = buffer.is_stale();
                let age = buffer.age_string();
                Ok(PendingStatus {
                    has_pending,
                    session_id: Some(session_id),
                    file_count,
                    line_count,
                    edit_count,
                    prompt_count,
                    is_stale,
                    age,
                })
            }
            None => Ok(PendingStatus {
                has_pending: false,
                session_id: None,
                file_count: 0,
                line_count: 0,
                edit_count: 0,
                prompt_count: 0,
                is_stale: false,
                age: String::new(),
            }),
        }
    }

    /// Clear pending changes without committing
    pub fn clear_pending(&self) -> Result<()> {
        let store = PendingStore::new(&self.repo_root);
        store.delete()
    }
}

/// Status of pending changes
#[derive(Debug)]
pub struct PendingStatus {
    pub has_pending: bool,
    pub session_id: Option<String>,
    pub file_count: usize,
    pub line_count: u32,
    pub edit_count: usize,
    pub prompt_count: u32,
    /// Whether the pending buffer is stale (older than 24 hours)
    pub is_stale: bool,
    /// Human-readable age of the pending buffer
    pub age: String,
}

/// Hook entry point for Claude Code integration
pub fn run_capture_hook() -> Result<()> {
    // Read input from stdin
    let input: HookInput = serde_json::from_reader(std::io::stdin())
        .context("Failed to read hook input from stdin")?;

    // Find repo root
    let repo_root = find_repo_root()?;

    // Process the change
    let hook = CaptureHook::new(&repo_root)?;
    hook.on_file_change(input)?;

    Ok(())
}

/// Find the git repository root from current directory
fn find_repo_root() -> Result<std::path::PathBuf> {
    let current = env::current_dir()?;
    let repo = Repository::discover(&current).context("Not in a git repository")?;

    repo.workdir()
        .map(|p| p.to_path_buf())
        .context("Repository has no working directory")
}

/// Git post-commit hook entry point
pub fn run_post_commit_hook() -> Result<()> {
    let repo_root = find_repo_root()?;
    let hook = CaptureHook::new(&repo_root)?;

    hook.on_post_commit()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Signature;
    use tempfile::TempDir;

    fn create_test_repo() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();

        // Create initial commit
        {
            let sig = Signature::now("Test", "test@test.com").unwrap();
            let tree_id = repo.index().unwrap().write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "Initial", &tree, &[])
                .unwrap();
        }

        (dir, repo)
    }

    #[test]
    fn test_capture_hook_on_file_change() {
        let (dir, _repo) = create_test_repo();
        let hook = CaptureHook::new(dir.path()).unwrap();

        let input = HookInput {
            tool: "Write".to_string(),
            file_path: "test.rs".to_string(),
            prompt: "Create a test file".to_string(),
            old_content: None,
            new_content: "fn test() {}\n".to_string(),
            context: None,
        };

        hook.on_file_change(input).unwrap();

        let status = hook.status().unwrap();
        assert!(status.has_pending);
        assert_eq!(status.file_count, 1);
        assert_eq!(status.edit_count, 1);
        assert_eq!(status.prompt_count, 1);
    }

    #[test]
    fn test_capture_hook_multiple_edits() {
        let (dir, _repo) = create_test_repo();
        let hook = CaptureHook::new(dir.path()).unwrap();

        // First edit
        hook.on_file_change(HookInput {
            tool: "Write".to_string(),
            file_path: "test.rs".to_string(),
            prompt: "Create file".to_string(),
            old_content: None,
            new_content: "line1\n".to_string(),
            context: None,
        })
        .unwrap();

        // Second edit to same file
        hook.on_file_change(HookInput {
            tool: "Edit".to_string(),
            file_path: "test.rs".to_string(),
            prompt: "Add line".to_string(),
            old_content: Some("line1\n".to_string()),
            new_content: "line1\nline2\n".to_string(),
            context: None,
        })
        .unwrap();

        let status = hook.status().unwrap();
        assert_eq!(status.file_count, 1);
        assert_eq!(status.edit_count, 2);
        assert_eq!(status.prompt_count, 2);
    }

    #[test]
    fn test_capture_hook_status_empty() {
        let (dir, _repo) = create_test_repo();
        let hook = CaptureHook::new(dir.path()).unwrap();

        let status = hook.status().unwrap();
        assert!(!status.has_pending);
        assert_eq!(status.file_count, 0);
    }

    #[test]
    fn test_capture_hook_clear() {
        let (dir, _repo) = create_test_repo();
        let hook = CaptureHook::new(dir.path()).unwrap();

        // Add a change
        hook.on_file_change(HookInput {
            tool: "Write".to_string(),
            file_path: "test.rs".to_string(),
            prompt: "test".to_string(),
            old_content: None,
            new_content: "content\n".to_string(),
            context: None,
        })
        .unwrap();

        assert!(hook.status().unwrap().has_pending);

        // Clear
        hook.clear_pending().unwrap();
        assert!(!hook.status().unwrap().has_pending);
    }
}
