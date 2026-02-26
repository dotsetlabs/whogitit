use anyhow::{Context, Result};
use git2::{Oid, Repository, Signature};

use crate::core::attribution::{AIAttribution, SCHEMA_VERSION};

/// Notes reference used for AI attribution storage
pub const NOTES_REF: &str = "refs/notes/whogitit";
/// Warn when a single attribution note grows beyond this size.
const NOTE_SIZE_WARN_BYTES: usize = 512 * 1024;
/// Reject note payloads above this size to avoid pathological note objects.
const NOTE_SIZE_HARD_LIMIT_BYTES: usize = 4 * 1024 * 1024;

/// Git notes storage for AI attribution data
pub struct NotesStore<'a> {
    repo: &'a Repository,
}

impl<'a> NotesStore<'a> {
    pub fn new(repo: &'a Repository) -> Result<Self> {
        Ok(Self { repo })
    }

    /// Store attribution data as a git note on a commit
    pub fn store_attribution(&self, commit_oid: Oid, attribution: &AIAttribution) -> Result<Oid> {
        // Store compact JSON to keep note payloads smaller in large sessions.
        let json = serde_json::to_string(attribution)
            .context("Failed to serialize attribution to JSON")?;
        if let Some(warning) = evaluate_note_payload_size(json.len())? {
            eprintln!("whogitit: Warning - {warning}");
        }

        let sig = self.get_signature()?;

        let note_oid = self
            .repo
            .note(&sig, &sig, Some(NOTES_REF), commit_oid, &json, true)
            .context("Failed to create git note")?;

        Ok(note_oid)
    }

    /// Fetch attribution data from a git note
    pub fn fetch_attribution(&self, commit_oid: Oid) -> Result<Option<AIAttribution>> {
        match self.repo.find_note(Some(NOTES_REF), commit_oid) {
            Ok(note) => {
                if let Some(message) = note.message() {
                    let attribution: AIAttribution = serde_json::from_str(message)
                        .context("Failed to parse attribution JSON")?;
                    warn_on_schema_version_mismatch(commit_oid, attribution.version);
                    Ok(Some(attribution))
                } else {
                    Ok(None)
                }
            }
            Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
            Err(e) => Err(e).context("Failed to read git note"),
        }
    }

    /// Check if a commit has AI attribution
    pub fn has_attribution(&self, commit_oid: Oid) -> bool {
        self.repo.find_note(Some(NOTES_REF), commit_oid).is_ok()
    }

    /// Remove attribution from a commit
    pub fn remove_attribution(&self, commit_oid: Oid) -> Result<()> {
        let sig = self.get_signature()?;
        self.repo
            .note_delete(commit_oid, Some(NOTES_REF), &sig, &sig)
            .context("Failed to delete git note")?;
        Ok(())
    }

    /// Copy attribution from one commit to another
    pub fn copy_attribution(&self, from_oid: Oid, to_oid: Oid) -> Result<()> {
        let note = self
            .repo
            .find_note(Some(NOTES_REF), from_oid)
            .context("Source commit has no attribution note")?;

        let message = note
            .message()
            .ok_or_else(|| anyhow::anyhow!("Note has no content"))?;

        let sig = self.get_signature()?;

        self.repo
            .note(&sig, &sig, Some(NOTES_REF), to_oid, message, false)
            .context("Failed to copy note to target commit")?;

        Ok(())
    }

    /// Get default signature from git config
    fn get_signature(&self) -> Result<Signature<'static>> {
        if let Ok(sig) = self.repo.signature() {
            return Ok(Signature::now(
                sig.name().unwrap_or("whogitit"),
                sig.email().unwrap_or("whogitit@local"),
            )?);
        }

        Ok(Signature::now("whogitit", "whogitit@local")?)
    }

    /// List all commits with AI attribution
    pub fn list_attributed_commits(&self) -> Result<Vec<Oid>> {
        let mut commits = Vec::new();

        if let Ok(notes) = self.repo.notes(Some(NOTES_REF)) {
            for (_, commit_oid) in notes.flatten() {
                commits.push(commit_oid);
            }
        }

        Ok(commits)
    }
}

fn evaluate_note_payload_size(payload_bytes: usize) -> Result<Option<String>> {
    if payload_bytes > NOTE_SIZE_HARD_LIMIT_BYTES {
        anyhow::bail!(
            "Attribution payload is too large for a git note: {} (limit: {}). \
Reduce pending scope (smaller commits) or shorten prompts before committing.",
            format_bytes(payload_bytes),
            format_bytes(NOTE_SIZE_HARD_LIMIT_BYTES)
        );
    }

    if payload_bytes > NOTE_SIZE_WARN_BYTES {
        return Ok(Some(format!(
            "large attribution payload detected: {} (warning threshold: {}). \
Blame/show may be slower for this commit.",
            format_bytes(payload_bytes),
            format_bytes(NOTE_SIZE_WARN_BYTES)
        )));
    }

    Ok(None)
}

fn format_bytes(bytes: usize) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    let bytes_f64 = bytes as f64;

    if bytes_f64 >= MIB {
        format!("{:.2} MiB", bytes_f64 / MIB)
    } else if bytes_f64 >= KIB {
        format!("{:.1} KiB", bytes_f64 / KIB)
    } else {
        format!("{bytes} B")
    }
}

fn warn_on_schema_version_mismatch(commit_oid: Oid, note_version: u8) {
    if note_version == SCHEMA_VERSION {
        return;
    }

    if note_version < SCHEMA_VERSION {
        eprintln!(
            "whogitit: Warning - commit {} uses attribution schema v{} (current is v{}); continuing in compatibility mode.",
            commit_oid, note_version, SCHEMA_VERSION
        );
    } else {
        eprintln!(
            "whogitit: Warning - commit {} uses newer attribution schema v{} (this build supports v{}); some fields may be ignored.",
            commit_oid, note_version, SCHEMA_VERSION
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::snapshot::{
        AttributionSummary, FileAttributionResult, LineAttribution, LineSource,
    };
    use crate::core::attribution::{ModelInfo, PromptInfo, SessionMetadata};
    use tempfile::TempDir;

    fn create_test_repo() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();

        {
            let sig = Signature::now("Test", "test@test.com").unwrap();
            let tree_id = {
                let mut index = repo.index().unwrap();
                index.write_tree().unwrap()
            };
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
                .unwrap();
        }

        (dir, repo)
    }

    #[test]
    fn test_store_and_fetch_attribution() {
        let (_dir, repo) = create_test_repo();
        let store = NotesStore::new(&repo).unwrap();

        let head = repo.head().unwrap().peel_to_commit().unwrap();

        let attribution = AIAttribution {
            version: SCHEMA_VERSION,
            session: SessionMetadata {
                session_id: "test-session".to_string(),
                model: ModelInfo::claude("claude-opus-4-5-20251101"),
                started_at: "2026-01-30T10:00:00Z".to_string(),
                prompt_count: 1,
                used_plan_mode: false,
                subagent_count: 0,
            },
            prompts: vec![PromptInfo {
                index: 0,
                text: "Test prompt".to_string(),
                timestamp: "2026-01-30T10:00:00Z".to_string(),
                affected_files: vec!["test.rs".to_string()],
            }],
            files: vec![FileAttributionResult {
                path: "test.rs".to_string(),
                lines: vec![LineAttribution {
                    line_number: 1,
                    content: "fn test() {}".to_string(),
                    source: LineSource::AI {
                        edit_id: "e1".to_string(),
                    },
                    edit_id: Some("e1".to_string()),
                    prompt_index: Some(0),
                    confidence: 1.0,
                }],
                summary: AttributionSummary {
                    total_lines: 1,
                    ai_lines: 1,
                    ai_modified_lines: 0,
                    human_lines: 0,
                    original_lines: 0,
                    unknown_lines: 0,
                },
            }],
        };

        store.store_attribution(head.id(), &attribution).unwrap();

        assert!(store.has_attribution(head.id()));

        let fetched = store.fetch_attribution(head.id()).unwrap().unwrap();
        assert_eq!(fetched.version, SCHEMA_VERSION);
        assert_eq!(fetched.session.session_id, "test-session");
        assert_eq!(fetched.files.len(), 1);
        assert_eq!(fetched.prompts.len(), 1);
    }

    #[test]
    fn test_fetch_nonexistent_attribution() {
        let (_dir, repo) = create_test_repo();
        let store = NotesStore::new(&repo).unwrap();

        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let result = store.fetch_attribution(head.id()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_has_attribution() {
        let (_dir, repo) = create_test_repo();
        let store = NotesStore::new(&repo).unwrap();

        let head = repo.head().unwrap().peel_to_commit().unwrap();

        // Before storing - should return false
        assert!(!store.has_attribution(head.id()));

        // Store attribution
        let attribution = create_minimal_attribution("test-has");
        store.store_attribution(head.id(), &attribution).unwrap();

        // After storing - should return true
        assert!(store.has_attribution(head.id()));
    }

    #[test]
    fn test_remove_attribution() {
        let (_dir, repo) = create_test_repo();
        let store = NotesStore::new(&repo).unwrap();

        let head = repo.head().unwrap().peel_to_commit().unwrap();

        // Store attribution
        let attribution = create_minimal_attribution("test-remove");
        store.store_attribution(head.id(), &attribution).unwrap();
        assert!(store.has_attribution(head.id()));

        // Remove attribution
        store.remove_attribution(head.id()).unwrap();

        // After removal - should not have attribution
        assert!(!store.has_attribution(head.id()));
        assert!(store.fetch_attribution(head.id()).unwrap().is_none());
    }

    #[test]
    fn test_list_attributed_commits_empty() {
        let (_dir, repo) = create_test_repo();
        let store = NotesStore::new(&repo).unwrap();

        // No commits with attribution
        let commits = store.list_attributed_commits().unwrap();
        assert!(commits.is_empty());
    }

    #[test]
    fn test_list_attributed_commits() {
        let (dir, repo) = create_test_repo();
        let store = NotesStore::new(&repo).unwrap();

        // Get first commit
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let first_commit = head.id();

        // Create another commit
        let sig = Signature::now("Test", "test@test.com").unwrap();
        std::fs::write(dir.path().join("test.txt"), "test content").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("test.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let second_commit = repo
            .commit(
                Some("HEAD"),
                &sig,
                &sig,
                "Second commit",
                &tree,
                &[&repo.find_commit(first_commit).unwrap()],
            )
            .unwrap();

        // Store attribution on both commits
        let attr1 = create_minimal_attribution("session-1");
        let attr2 = create_minimal_attribution("session-2");
        store.store_attribution(first_commit, &attr1).unwrap();
        store.store_attribution(second_commit, &attr2).unwrap();

        // List should return both commits
        let commits = store.list_attributed_commits().unwrap();
        assert_eq!(commits.len(), 2);
        assert!(commits.contains(&first_commit));
        assert!(commits.contains(&second_commit));
    }

    #[test]
    fn test_update_attribution() {
        let (_dir, repo) = create_test_repo();
        let store = NotesStore::new(&repo).unwrap();

        let head = repo.head().unwrap().peel_to_commit().unwrap();

        // Store initial attribution
        let attr1 = create_minimal_attribution("session-v1");
        store.store_attribution(head.id(), &attr1).unwrap();

        let fetched1 = store.fetch_attribution(head.id()).unwrap().unwrap();
        assert_eq!(fetched1.session.session_id, "session-v1");

        // Update with new attribution (overwrite mode)
        let attr2 = create_minimal_attribution("session-v2");
        store.store_attribution(head.id(), &attr2).unwrap();

        let fetched2 = store.fetch_attribution(head.id()).unwrap().unwrap();
        assert_eq!(fetched2.session.session_id, "session-v2");
    }

    #[test]
    fn test_notes_ref_constant() {
        assert_eq!(NOTES_REF, "refs/notes/whogitit");
    }

    #[test]
    fn test_evaluate_note_payload_size_within_threshold() {
        let warning = evaluate_note_payload_size(1024).unwrap();
        assert!(warning.is_none());
    }

    #[test]
    fn test_evaluate_note_payload_size_warns_before_limit() {
        let warning = evaluate_note_payload_size(NOTE_SIZE_WARN_BYTES + 1).unwrap();
        assert!(warning
            .unwrap()
            .contains("large attribution payload detected"));
    }

    #[test]
    fn test_evaluate_note_payload_size_rejects_oversized_payload() {
        let err = evaluate_note_payload_size(NOTE_SIZE_HARD_LIMIT_BYTES + 1).unwrap_err();
        assert!(err.to_string().contains("too large for a git note"));
    }

    #[test]
    fn test_copy_attribution() {
        let (dir, repo) = create_test_repo();
        let store = NotesStore::new(&repo).unwrap();

        // Get first commit
        let first_commit = repo.head().unwrap().peel_to_commit().unwrap().id();

        // Create a second commit
        let sig = Signature::now("Test", "test@test.com").unwrap();
        std::fs::write(dir.path().join("test.txt"), "test content").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("test.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let second_commit = repo
            .commit(
                Some("HEAD"),
                &sig,
                &sig,
                "Second commit",
                &tree,
                &[&repo.find_commit(first_commit).unwrap()],
            )
            .unwrap();

        // Store attribution on first commit
        let attribution = create_minimal_attribution("copy-test");
        store.store_attribution(first_commit, &attribution).unwrap();
        assert!(store.has_attribution(first_commit));
        assert!(!store.has_attribution(second_commit));

        // Copy attribution to second commit
        store.copy_attribution(first_commit, second_commit).unwrap();

        // Verify both commits now have attribution
        assert!(store.has_attribution(first_commit));
        assert!(store.has_attribution(second_commit));

        // Verify content is identical
        let original = store.fetch_attribution(first_commit).unwrap().unwrap();
        let copied = store.fetch_attribution(second_commit).unwrap().unwrap();
        assert_eq!(original.session.session_id, copied.session.session_id);
    }

    #[test]
    fn test_copy_attribution_source_not_found() {
        let (_dir, repo) = create_test_repo();
        let store = NotesStore::new(&repo).unwrap();

        let head = repo.head().unwrap().peel_to_commit().unwrap().id();

        // Try to copy from a commit without attribution
        let result = store.copy_attribution(head, head);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("no attribution note"));
    }

    // Helper function to create minimal attribution for tests
    fn create_minimal_attribution(session_id: &str) -> AIAttribution {
        AIAttribution {
            version: SCHEMA_VERSION,
            session: SessionMetadata {
                session_id: session_id.to_string(),
                model: ModelInfo::claude("test-model"),
                started_at: "2026-01-30T10:00:00Z".to_string(),
                prompt_count: 0,
                used_plan_mode: false,
                subagent_count: 0,
            },
            prompts: vec![],
            files: vec![],
        }
    }
}
