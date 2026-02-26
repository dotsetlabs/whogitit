use std::collections::HashSet;
use std::env;
use std::path::Path;

use anyhow::{Context, Result};
use git2::{Delta, DiffFindOptions, DiffOptions, Repository};
use serde::{Deserialize, Serialize};

use crate::capture::pending::{PendingBuffer, PendingStore, PromptRecord};
use crate::capture::threeway::ThreeWayAnalyzer;
use crate::core::attribution::{AIAttribution, PromptInfo, SessionMetadata};
use crate::privacy::{Redactor, RetentionConfig, WhogititConfig};
use crate::retention::apply_retention_policy;
use crate::storage::audit::AuditLog;
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
    /// Whether old_content was provided (distinguish empty from missing)
    #[serde(default)]
    pub old_content_present: bool,
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
    /// Similarity threshold for AI-modified detection
    similarity_threshold: f64,
    /// Maximum pending buffer age in hours
    max_pending_age_hours: i64,
    /// Retention configuration
    retention_config: RetentionConfig,
}

impl CaptureHook {
    /// Create a new capture hook for a repository
    pub fn new(repo_path: &Path) -> Result<Self> {
        let repo_root = repo_path.to_path_buf();

        // Load config and build redactor
        let config = match WhogititConfig::load(&repo_root) {
            Ok(config) => config,
            Err(err) => {
                eprintln!(
                    "whogitit: Warning - failed to load config, using defaults: {}",
                    err
                );
                WhogititConfig::default()
            }
        };
        let redactor = config.privacy.build_redactor();
        let audit_enabled = config.privacy.audit_log;
        let similarity_threshold = config.analysis.similarity_threshold;
        let max_pending_age_hours = config.analysis.max_pending_age_hours as i64;
        let retention_config = config.retention.unwrap_or_default();

        Ok(Self {
            repo_root,
            redactor,
            audit_enabled,
            similarity_threshold,
            max_pending_age_hours,
            retention_config,
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
        let mut buffer = match store.load_with_max_age(self.max_pending_age_hours)? {
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
                    let mut buffer = PendingBuffer::new(&current_session, &Self::get_model_id());
                    buffer.audit_logging_enabled = self.audit_enabled;
                    buffer
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

        let rel_path = std::path::Path::new(&relative_path);

        // Reject absolute paths (including Windows prefixes)
        if rel_path.is_absolute()
            || rel_path
                .components()
                .any(|c| matches!(c, std::path::Component::Prefix(_)))
        {
            anyhow::bail!(
                "Path '{}' is outside repository root '{}'. Use a repository-relative path.",
                relative_path,
                self.repo_root.display()
            );
        }

        // Check for path traversal attempts
        if rel_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            anyhow::bail!(
                "Path traversal detected in file path: '{}'. Paths containing '..' are not allowed.",
                relative_path
            );
        }

        if input.new_content.is_empty() && input.tool != "Delete" {
            eprintln!("whogitit: Warning - empty new_content for non-delete operation");
        }

        // Determine old content: use provided value, or fall back to git HEAD
        let old_content = if input.old_content_present {
            Some(input.old_content.unwrap_or_default())
        } else if let Some(content) = input.old_content.clone() {
            Some(content)
        } else {
            // Try to get content from git HEAD for existing files
            self.get_content_from_git_head(&relative_path)
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

        // Log redaction audit events (if enabled)
        if self.audit_enabled {
            if let Some(prompt) = buffer.session.prompts.last() {
                if !prompt.redaction_events.is_empty() {
                    let audit_log = AuditLog::new(&self.repo_root);
                    let mut counts: std::collections::HashMap<String, u32> =
                        std::collections::HashMap::new();
                    for event in &prompt.redaction_events {
                        *counts.entry(event.pattern_name.clone()).or_insert(0) += 1;
                    }
                    for (pattern, count) in counts {
                        if let Err(e) = audit_log.log_redaction(&pattern, count) {
                            eprintln!("whogitit: Warning - failed to log redaction: {}", e);
                        }
                    }
                }
            }
        }

        // Save buffer with atomic write
        store.save(&buffer)?;

        Ok(())
    }

    /// Get file content from git HEAD (the last committed version)
    ///
    /// Returns None for new files or if git operations fail.
    /// Logs warnings for unexpected failures to aid debugging.
    fn get_content_from_git_head(&self, path: &str) -> Option<String> {
        let repo = match Repository::open(&self.repo_root) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "whogitit: Warning - failed to open repository at '{}': {}",
                    self.repo_root.display(),
                    e
                );
                return None;
            }
        };

        let head = match repo.head() {
            Ok(h) => h,
            Err(e) => {
                // HEAD not existing is normal for new repos with no commits
                if e.code() != git2::ErrorCode::UnbornBranch {
                    eprintln!("whogitit: Warning - failed to get HEAD: {}", e);
                }
                return None;
            }
        };

        let commit = match head.peel_to_commit() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("whogitit: Warning - failed to peel HEAD to commit: {}", e);
                return None;
            }
        };

        let tree = match commit.tree() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("whogitit: Warning - failed to get commit tree: {}", e);
                return None;
            }
        };

        // File not existing in HEAD is normal for new files - don't warn
        let entry = match tree.get_path(std::path::Path::new(path)) {
            Ok(e) => e,
            Err(_) => return None, // New file, not in HEAD
        };

        let blob = match repo.find_blob(entry.id()) {
            Ok(b) => b,
            Err(e) => {
                eprintln!(
                    "whogitit: Warning - failed to read blob for '{}': {}",
                    path, e
                );
                return None;
            }
        };

        // Non-UTF8 content is valid for binary files - don't treat as error
        match std::str::from_utf8(blob.content()) {
            Ok(content) => Some(content.to_string()),
            Err(_) => None, // Binary file
        }
    }

    /// Handle post-commit: perform three-way analysis, attach notes, and clean up
    pub fn on_post_commit(&self) -> Result<Option<AIAttribution>> {
        let store = PendingStore::new(&self.repo_root);

        // Load pending buffer
        let mut buffer = match store.load()? {
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

        let tree = head.tree()?;

        // Build rename map (old -> new) to preserve attribution across moves
        let rename_map = build_rename_map(&repo, &head)?;
        let changed_paths = build_changed_paths(&repo, &head)?;

        // Preserve all prompt records before we split processed vs remaining histories.
        let all_prompts = buffer.session.prompts.clone();

        let mut file_results = Vec::new();
        let mut remaining_histories = std::collections::HashMap::new();
        let mut processed_prompt_indices = HashSet::new();
        let mut remaining_prompt_indices = HashSet::new();
        let mut used_plan_mode = false;
        let mut subagent_count = 0u32;

        for (path, history) in buffer.file_histories.drain() {
            let Some(committed_path) = resolve_committed_path(&path, &changed_paths, &rename_map)
            else {
                for edit in &history.edits {
                    remaining_prompt_indices.insert(edit.prompt_index);
                }
                remaining_histories.insert(path, history);
                continue;
            };

            // Get the committed content for this file
            let committed_content = match tree.get_path(std::path::Path::new(&committed_path)) {
                Ok(entry) => {
                    let blob = repo.find_blob(entry.id())?;
                    String::from_utf8_lossy(blob.content()).to_string()
                }
                Err(_) => {
                    // File was part of commit metadata but does not exist in final tree
                    // (for example, deleted file). Consume it from pending state.
                    continue;
                }
            };

            // Perform three-way analysis
            let mut result = ThreeWayAnalyzer::analyze_with_diff_with_threshold(
                &history,
                &committed_content,
                self.similarity_threshold,
            );
            if committed_path != path {
                result.path = committed_path;
            }
            file_results.push(result);

            for edit in &history.edits {
                processed_prompt_indices.insert(edit.prompt_index);
                if edit.context.plan_mode {
                    used_plan_mode = true;
                }
                if edit.context.agent_depth > 0 {
                    subagent_count += 1;
                }
            }
        }

        // Nothing attributable for this commit; only update pending state.
        if file_results.is_empty() {
            if remaining_histories.is_empty() {
                store.delete()?;
            } else {
                buffer.file_histories = remaining_histories;
                buffer.session.prompts =
                    filter_prompt_records(&all_prompts, &remaining_prompt_indices);
                buffer.session.prompt_count = buffer.session.prompts.len() as u32;
                buffer.prompt_counter = next_prompt_index(&buffer.session.prompts);
                buffer.total_redactions = buffer
                    .session
                    .prompts
                    .iter()
                    .map(|p| p.redaction_events.len() as u32)
                    .sum();
                store.save(&buffer)?;
            }
            return Ok(None);
        }

        let attribution_prompts = filter_prompt_records(&all_prompts, &processed_prompt_indices);

        // Create attribution with full analysis
        let attribution = AIAttribution {
            version: 3,
            session: SessionMetadata {
                session_id: buffer.session.session_id.clone(),
                model: buffer.session.model.clone(),
                started_at: buffer.session.started_at.clone(),
                prompt_count: attribution_prompts.len() as u32,
                used_plan_mode,
                subagent_count,
            },
            prompts: attribution_prompts
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

        if self.retention_config.auto_purge {
            if let Err(e) = apply_retention_policy(
                &repo,
                &self.retention_config,
                true,
                "Auto purge (post-commit)",
                self.audit_enabled,
            ) {
                eprintln!("whogitit: Warning - auto purge failed: {}", e);
            }
        }

        // Persist any remaining pending edits only after attribution note is safely stored.
        if remaining_histories.is_empty() {
            store.delete()?;
        } else {
            buffer.file_histories = remaining_histories;
            buffer.session.prompts = filter_prompt_records(&all_prompts, &remaining_prompt_indices);
            buffer.session.prompt_count = buffer.session.prompts.len() as u32;
            buffer.prompt_counter = next_prompt_index(&buffer.session.prompts);
            buffer.total_redactions = buffer
                .session
                .prompts
                .iter()
                .map(|p| p.redaction_events.len() as u32)
                .sum();
            store.save(&buffer)?;
        }

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
        let input_path = Path::new(path);
        if !input_path.is_absolute() {
            return Ok(path.to_string());
        }

        // Fast path: exact prefix match against the repo root.
        if let Ok(relative) = input_path.strip_prefix(&self.repo_root) {
            return Ok(relative.to_string_lossy().to_string());
        }

        // Handle aliased absolute paths (e.g. /var vs /private/var on macOS)
        // by canonicalizing both paths before prefix comparison.
        let canonical_repo =
            canonicalize_for_prefix(&self.repo_root).unwrap_or_else(|| self.repo_root.clone());
        if let Some(canonical_input) = canonicalize_for_prefix(input_path) {
            if let Ok(relative) = canonical_input.strip_prefix(&canonical_repo) {
                return Ok(relative.to_string_lossy().to_string());
            }
        }

        anyhow::bail!(
            "Absolute path '{}' could not be mapped under repository root '{}'.",
            path,
            self.repo_root.display()
        )
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
                let is_stale = buffer.is_stale_hours(self.max_pending_age_hours);
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
                    max_pending_age_hours: self.max_pending_age_hours,
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
                max_pending_age_hours: self.max_pending_age_hours,
            }),
        }
    }

    /// Clear pending changes without committing
    pub fn clear_pending(&self) -> Result<()> {
        let store = PendingStore::new(&self.repo_root);
        store.delete()
    }
}

/// Canonicalize a path for prefix comparison.
///
/// If the full path doesn't exist yet, this resolves the deepest existing ancestor
/// and re-appends the missing suffix so new files can still be matched reliably.
fn canonicalize_for_prefix(path: &Path) -> Option<std::path::PathBuf> {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return Some(canonical);
    }

    let mut current = path;
    let mut missing_components = Vec::new();

    while !current.exists() {
        let file_name = current.file_name()?;
        missing_components.push(file_name.to_os_string());
        current = current.parent()?;
    }

    let mut canonical_base = std::fs::canonicalize(current).ok()?;
    for component in missing_components.iter().rev() {
        canonical_base.push(component);
    }

    Some(canonical_base)
}

fn build_rename_map(
    repo: &Repository,
    head: &git2::Commit,
) -> Result<std::collections::HashMap<String, String>> {
    let mut map = std::collections::HashMap::new();

    let new_tree = head.tree()?;

    for parent_idx in 0..head.parent_count() {
        let parent = match head.parent(parent_idx) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let old_tree = parent.tree()?;

        let mut opts = DiffOptions::new();
        let mut diff = repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), Some(&mut opts))?;

        let mut find_opts = DiffFindOptions::new();
        find_opts.renames_from_rewrites(true);
        diff.find_similar(Some(&mut find_opts))?;

        for delta in diff.deltas() {
            if delta.status() == Delta::Renamed {
                let old_path = delta
                    .old_file()
                    .path()
                    .map(|p| p.to_string_lossy().to_string());
                let new_path = delta
                    .new_file()
                    .path()
                    .map(|p| p.to_string_lossy().to_string());
                if let (Some(old_path), Some(new_path)) = (old_path, new_path) {
                    map.entry(old_path).or_insert(new_path);
                }
            }
        }
    }

    Ok(map)
}

fn build_changed_paths(repo: &Repository, head: &git2::Commit) -> Result<HashSet<String>> {
    let mut changed = HashSet::new();
    let new_tree = head.tree()?;

    if head.parent_count() == 0 {
        collect_changed_paths(repo, None, &new_tree, &mut changed)?;
        return Ok(changed);
    }

    for parent_idx in 0..head.parent_count() {
        let parent = match head.parent(parent_idx) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let old_tree = parent.tree()?;
        collect_changed_paths(repo, Some(&old_tree), &new_tree, &mut changed)?;
    }

    Ok(changed)
}

fn collect_changed_paths(
    repo: &Repository,
    old_tree: Option<&git2::Tree<'_>>,
    new_tree: &git2::Tree<'_>,
    changed: &mut HashSet<String>,
) -> Result<()> {
    let mut opts = DiffOptions::new();
    let diff = repo.diff_tree_to_tree(old_tree, Some(new_tree), Some(&mut opts))?;

    for delta in diff.deltas() {
        if let Some(path) = delta.old_file().path() {
            changed.insert(path.to_string_lossy().to_string());
        }
        if let Some(path) = delta.new_file().path() {
            changed.insert(path.to_string_lossy().to_string());
        }
    }

    Ok(())
}

fn resolve_committed_path(
    path: &str,
    changed_paths: &HashSet<String>,
    rename_map: &std::collections::HashMap<String, String>,
) -> Option<String> {
    if let Some(new_path) = rename_map.get(path) {
        if changed_paths.contains(path) || changed_paths.contains(new_path) {
            return Some(new_path.clone());
        }
    }

    if changed_paths.contains(path) {
        return Some(path.to_string());
    }

    None
}

fn filter_prompt_records(
    prompts: &[PromptRecord],
    prompt_indices: &HashSet<u32>,
) -> Vec<PromptRecord> {
    prompts
        .iter()
        .filter(|p| prompt_indices.contains(&p.index))
        .cloned()
        .collect()
}

fn next_prompt_index(prompts: &[PromptRecord]) -> u32 {
    prompts
        .iter()
        .map(|p| p.index)
        .max()
        .map(|idx| idx.saturating_add(1))
        .unwrap_or(0)
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
    /// Whether the pending buffer is stale (older than configured hours)
    pub is_stale: bool,
    /// Human-readable age of the pending buffer
    pub age: String,
    /// Configured maximum pending buffer age in hours
    pub max_pending_age_hours: i64,
}

/// Hook entry point for Claude Code integration
pub fn run_capture_hook() -> Result<()> {
    // Read input from stdin
    let input: HookInput = serde_json::from_reader(std::io::stdin())
        .context("Failed to read hook input from stdin")?;

    // Find repo root
    let repo_root = find_repo_root()?;

    // Only capture in repos that have been initialized with `whogitit init`
    if !is_repo_initialized(&repo_root) {
        return Ok(());
    }

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

/// Check if the repository has been initialized with `whogitit init`
/// by looking for the whogitit marker in the post-commit hook
fn is_repo_initialized(repo_root: &std::path::Path) -> bool {
    let post_commit = repo_root.join(".git/hooks/post-commit");
    if let Ok(content) = std::fs::read_to_string(&post_commit) {
        content.contains("whogitit")
    } else {
        false
    }
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
            old_content_present: false,
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
            old_content_present: false,
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
            old_content_present: true,
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
            old_content_present: false,
            new_content: "content\n".to_string(),
            context: None,
        })
        .unwrap();

        assert!(hook.status().unwrap().has_pending);

        // Clear
        hook.clear_pending().unwrap();
        assert!(!hook.status().unwrap().has_pending);
    }

    #[test]
    fn test_post_commit_rename_preserves_attribution_path() {
        let (dir, repo) = create_test_repo();
        let repo_root = dir.path();

        // Create and commit initial file
        let old_path = repo_root.join("old.rs");
        std::fs::write(&old_path, "line1\n").unwrap();

        {
            let mut index = repo.index().unwrap();
            index.add_path(std::path::Path::new("old.rs")).unwrap();
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            let sig = Signature::now("Test", "test@test.com").unwrap();
            let head = repo.head().unwrap().peel_to_commit().unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "Add old.rs", &tree, &[&head])
                .unwrap();
        }

        let hook = CaptureHook::new(repo_root).unwrap();
        hook.on_file_change(HookInput {
            tool: "Edit".to_string(),
            file_path: "old.rs".to_string(),
            prompt: "Add line".to_string(),
            old_content: Some("line1\n".to_string()),
            old_content_present: true,
            new_content: "line1\nline2\n".to_string(),
            context: None,
        })
        .unwrap();

        // Rename file and commit
        let new_path = repo_root.join("new.rs");
        std::fs::rename(&old_path, &new_path).unwrap();

        {
            let mut index = repo.index().unwrap();
            index.remove_path(std::path::Path::new("old.rs")).unwrap();
            index.add_path(std::path::Path::new("new.rs")).unwrap();
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            let sig = Signature::now("Test", "test@test.com").unwrap();
            let head = repo.head().unwrap().peel_to_commit().unwrap();
            repo.commit(
                Some("HEAD"),
                &sig,
                &sig,
                "Rename old.rs to new.rs",
                &tree,
                &[&head],
            )
            .unwrap();
        }

        let attribution = hook.on_post_commit().unwrap().unwrap();
        assert_eq!(attribution.files.len(), 1);
        assert_eq!(attribution.files[0].path, "new.rs");
    }

    #[test]
    fn test_post_commit_preserves_pending_for_uncommitted_files() {
        let (dir, repo) = create_test_repo();
        let repo_root = dir.path();

        // Add baseline files and commit them.
        std::fs::write(repo_root.join("a.rs"), "a0\n").unwrap();
        std::fs::write(repo_root.join("b.rs"), "b0\n").unwrap();
        {
            let mut index = repo.index().unwrap();
            index.add_path(std::path::Path::new("a.rs")).unwrap();
            index.add_path(std::path::Path::new("b.rs")).unwrap();
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            let sig = Signature::now("Test", "test@test.com").unwrap();
            let head = repo.head().unwrap().peel_to_commit().unwrap();
            repo.commit(
                Some("HEAD"),
                &sig,
                &sig,
                "Add baseline files",
                &tree,
                &[&head],
            )
            .unwrap();
        }

        let hook = CaptureHook::new(repo_root).unwrap();

        // Capture edits for both files.
        hook.on_file_change(HookInput {
            tool: "Edit".to_string(),
            file_path: "a.rs".to_string(),
            prompt: "Update a".to_string(),
            old_content: Some("a0\n".to_string()),
            old_content_present: true,
            new_content: "a1\n".to_string(),
            context: None,
        })
        .unwrap();

        hook.on_file_change(HookInput {
            tool: "Edit".to_string(),
            file_path: "b.rs".to_string(),
            prompt: "Update b".to_string(),
            old_content: Some("b0\n".to_string()),
            old_content_present: true,
            new_content: "b1\n".to_string(),
            context: None,
        })
        .unwrap();

        std::fs::write(repo_root.join("a.rs"), "a1\n").unwrap();
        std::fs::write(repo_root.join("b.rs"), "b1\n").unwrap();

        // Commit only a.rs.
        {
            let mut index = repo.index().unwrap();
            index.add_path(std::path::Path::new("a.rs")).unwrap();
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            let sig = Signature::now("Test", "test@test.com").unwrap();
            let head = repo.head().unwrap().peel_to_commit().unwrap();
            repo.commit(
                Some("HEAD"),
                &sig,
                &sig,
                "Commit only a.rs",
                &tree,
                &[&head],
            )
            .unwrap();
        }

        let attribution = hook.on_post_commit().unwrap().unwrap();
        assert_eq!(attribution.files.len(), 1);
        assert_eq!(attribution.files[0].path, "a.rs");

        // b.rs should remain pending for a later commit.
        let store = PendingStore::new(repo_root);
        let remaining = store.load_quiet().unwrap().unwrap();
        assert!(remaining.get_file_history("a.rs").is_none());
        assert!(remaining.get_file_history("b.rs").is_some());

        let status = hook.status().unwrap();
        assert!(status.has_pending);
        assert_eq!(status.file_count, 1);
    }

    #[cfg(unix)]
    #[test]
    fn test_make_relative_path_accepts_symlinked_absolute_path() {
        let (dir, _repo) = create_test_repo();
        let repo_root = dir.path();
        let hook = CaptureHook::new(repo_root).unwrap();

        let alias_parent = TempDir::new().unwrap();
        let alias_root = alias_parent.path().join("repo-alias");
        std::os::unix::fs::symlink(repo_root, &alias_root).unwrap();

        let file_via_alias = alias_root.join("src").join("main.rs");
        std::fs::create_dir_all(file_via_alias.parent().unwrap()).unwrap();
        std::fs::write(&file_via_alias, "fn main() {}\n").unwrap();

        let relative = hook
            .make_relative_path(file_via_alias.to_str().unwrap())
            .unwrap();
        assert_eq!(relative, "src/main.rs");
    }

    #[cfg(unix)]
    #[test]
    fn test_make_relative_path_accepts_nonexistent_file_under_symlinked_root() {
        let (dir, _repo) = create_test_repo();
        let repo_root = dir.path();
        let hook = CaptureHook::new(repo_root).unwrap();

        let alias_parent = TempDir::new().unwrap();
        let alias_root = alias_parent.path().join("repo-alias");
        std::os::unix::fs::symlink(repo_root, &alias_root).unwrap();

        let nested_dir = alias_root.join("newdir");
        std::fs::create_dir_all(&nested_dir).unwrap();
        let missing_file_via_alias = nested_dir.join("created_later.rs");

        let relative = hook
            .make_relative_path(missing_file_via_alias.to_str().unwrap())
            .unwrap();
        assert_eq!(relative, "newdir/created_later.rs");
    }

    #[test]
    fn test_is_repo_initialized() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git/hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();

        // Not initialized - no hook file
        assert!(!is_repo_initialized(dir.path()));

        // Not initialized - hook exists but no whogitit marker
        std::fs::write(hooks_dir.join("post-commit"), "#!/bin/bash\necho hello").unwrap();
        assert!(!is_repo_initialized(dir.path()));

        // Initialized - hook contains whogitit
        std::fs::write(
            hooks_dir.join("post-commit"),
            "#!/bin/bash\nwhogitit commit",
        )
        .unwrap();
        assert!(is_repo_initialized(dir.path()));
    }
}
