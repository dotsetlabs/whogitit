use anyhow::{Context, Result};
use git2::{Oid, Repository, Signature};

use crate::core::attribution::AIAttribution;

/// Notes reference used for AI attribution storage
pub const NOTES_REF: &str = "refs/notes/whogitit";

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
        let json = serde_json::to_string_pretty(attribution)
            .context("Failed to serialize attribution to JSON")?;

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
            version: 2,
            session: SessionMetadata {
                session_id: "test-session".to_string(),
                model: ModelInfo::claude("claude-opus-4-5-20251101"),
                started_at: "2026-01-30T10:00:00Z".to_string(),
                prompt_count: 1,
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
        assert_eq!(fetched.version, 2);
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
}
