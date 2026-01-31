use std::collections::HashMap;

use anyhow::{Context, Result};
use git2::{BlameOptions, Repository};

use crate::capture::snapshot::LineSource;
use crate::core::attribution::{AIAttribution, BlameLineResult, BlameResult};
use crate::storage::notes::NotesStore;
use crate::utils::{truncate_prompt, PROMPT_PREVIEW_LEN};

/// AI-aware git blame engine
pub struct AIBlamer<'a> {
    repo: &'a Repository,
    notes_store: NotesStore<'a>,
    /// Cache of attributions by commit ID
    attribution_cache: HashMap<String, Option<AIAttribution>>,
}

impl<'a> AIBlamer<'a> {
    pub fn new(repo: &'a Repository) -> Result<Self> {
        let notes_store = NotesStore::new(repo)?;
        Ok(Self {
            repo,
            notes_store,
            attribution_cache: HashMap::new(),
        })
    }

    /// Run blame on a file and correlate with AI attribution data
    pub fn blame(&mut self, path: &str, revision: Option<&str>) -> Result<BlameResult> {
        let revision_str = revision.unwrap_or("HEAD");

        // Resolve revision to a commit
        let obj = self
            .repo
            .revparse_single(revision_str)
            .with_context(|| format!("Failed to resolve revision: {}", revision_str))?;
        let commit = obj
            .peel_to_commit()
            .with_context(|| format!("Could not peel to commit: {}", revision_str))?;

        // Get the file content at this revision
        let tree = commit.tree()?;
        let entry = tree
            .get_path(std::path::Path::new(path))
            .with_context(|| format!("File not found: {}", path))?;
        let blob = self.repo.find_blob(entry.id())?;
        let content = std::str::from_utf8(blob.content())
            .with_context(|| format!("File is not valid UTF-8: {}", path))?;

        // Run git blame with move/copy detection
        let mut blame_opts = BlameOptions::new();
        blame_opts.track_copies_same_file(true);
        blame_opts.track_copies_same_commit_moves(true);
        blame_opts.newest_commit(commit.id());

        let blame = self
            .repo
            .blame_file(std::path::Path::new(path), Some(&mut blame_opts))
            .with_context(|| format!("Failed to blame file: {}", path))?;

        // Collect unique commits from blame
        let mut unique_commits: Vec<String> = Vec::new();
        for hunk in blame.iter() {
            let commit_id = hunk.final_commit_id().to_string();
            if !unique_commits.contains(&commit_id) {
                unique_commits.push(commit_id);
            }
        }

        // Pre-fetch all attributions for these commits
        self.prefetch_attributions(&unique_commits)?;

        // Process each line
        let lines: Vec<&str> = content.lines().collect();
        let mut results = Vec::new();

        for (idx, line_content) in lines.iter().enumerate() {
            let line_number = (idx + 1) as u32;

            // Find the blame hunk for this line
            if let Some(hunk) = blame.get_line(line_number as usize) {
                let commit_id = hunk.final_commit_id().to_string();
                // Git commit IDs are hex strings (ASCII), so char boundary is always safe
                let commit_short = if commit_id.len() >= 7 {
                    commit_id[..7].to_string()
                } else {
                    commit_id.clone()
                };

                // Get author from the blame hunk signature
                let author = hunk
                    .final_signature()
                    .name()
                    .unwrap_or("Unknown")
                    .to_string();

                // Calculate original line position for attribution lookup
                // Offset = current line - start of this hunk in final file
                let line_offset = line_number.saturating_sub(hunk.final_start_line() as u32);
                let original_line = hunk.orig_start_line() as u32 + line_offset;

                // Look up AI attribution
                let (source, prompt_index, prompt_preview) =
                    self.find_line_attribution(&commit_id, path, original_line);

                results.push(BlameLineResult {
                    line_number,
                    content: line_content.to_string(),
                    commit_id,
                    commit_short,
                    author,
                    source,
                    prompt_index,
                    prompt_preview,
                });
            }
        }

        Ok(BlameResult {
            path: path.to_string(),
            revision: revision_str.to_string(),
            lines: results,
        })
    }

    /// Pre-fetch attributions for a batch of commits
    fn prefetch_attributions(&mut self, commit_ids: &[String]) -> Result<()> {
        for commit_id in commit_ids {
            if !self.attribution_cache.contains_key(commit_id) {
                let oid = git2::Oid::from_str(commit_id)?;
                let attribution = self.notes_store.fetch_attribution(oid)?;
                self.attribution_cache
                    .insert(commit_id.clone(), attribution);
            }
        }
        Ok(())
    }

    /// Find AI attribution for a specific line
    fn find_line_attribution(
        &self,
        commit_id: &str,
        path: &str,
        line: u32,
    ) -> (LineSource, Option<u32>, Option<String>) {
        if let Some(Some(attribution)) = self.attribution_cache.get(commit_id) {
            // Find file attribution
            if let Some(file_attr) = attribution.files.iter().find(|f| f.path == path) {
                // Find line attribution by line number
                if let Some(line_attr) = file_attr.lines.iter().find(|l| l.line_number == line) {
                    // Get prompt preview if available
                    let prompt_preview = line_attr.prompt_index.and_then(|idx| {
                        attribution
                            .get_prompt(idx)
                            .map(|p| truncate_prompt(&p.text, PROMPT_PREVIEW_LEN))
                    });

                    return (
                        line_attr.source.clone(),
                        line_attr.prompt_index,
                        prompt_preview,
                    );
                }
            }
        }
        // Default to Unknown if no attribution found
        (LineSource::Unknown, None, None)
    }

    /// Get attribution for a specific commit
    pub fn get_commit_attribution(&mut self, commit_id: &str) -> Result<Option<AIAttribution>> {
        if let Some(cached) = self.attribution_cache.get(commit_id) {
            return Ok(cached.clone());
        }

        let oid = git2::Oid::from_str(commit_id)?;
        let attribution = self.notes_store.fetch_attribution(oid)?;
        self.attribution_cache
            .insert(commit_id.to_string(), attribution.clone());
        Ok(attribution)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::snapshot::{AttributionSummary, FileAttributionResult, LineAttribution};
    use crate::core::attribution::{ModelInfo, PromptInfo, SessionMetadata};
    use git2::Signature;
    use std::fs;
    use tempfile::TempDir;

    /// Create a test repository with a single commit
    fn create_test_repo() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();

        // Configure git user
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();

        (dir, repo)
    }

    /// Create a commit with a file
    fn create_commit(repo: &Repository, dir: &TempDir, filename: &str, content: &str) -> git2::Oid {
        let file_path = dir.path().join(filename);
        fs::write(&file_path, content).unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new(filename)).unwrap();
        index.write().unwrap();

        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = Signature::now("Test User", "test@example.com").unwrap();

        // Check if we have a parent commit
        let parents: Vec<git2::Commit> = if let Ok(head) = repo.head() {
            vec![head.peel_to_commit().unwrap()]
        } else {
            vec![]
        };

        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();

        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &format!("Add {}", filename),
            &tree,
            &parent_refs,
        )
        .unwrap()
    }

    #[test]
    fn test_blame_file_without_attribution() {
        let (dir, repo) = create_test_repo();

        // Create a simple file
        create_commit(
            &repo,
            &dir,
            "test.rs",
            "fn main() {\n    println!(\"hello\");\n}\n",
        );

        // Run blame
        let mut blamer = AIBlamer::new(&repo).unwrap();
        let result = blamer.blame("test.rs", None).unwrap();

        assert_eq!(result.path, "test.rs");
        assert_eq!(result.revision, "HEAD");
        assert_eq!(result.lines.len(), 3);

        // Without attribution, all lines should be Unknown
        for line in &result.lines {
            assert!(matches!(line.source, LineSource::Unknown));
            assert!(line.prompt_index.is_none());
            assert!(line.prompt_preview.is_none());
        }

        // Verify line content
        assert!(result.lines[0].content.contains("fn main"));
        assert!(result.lines[1].content.contains("println"));
        assert!(result.lines[2].content.contains("}"));
    }

    #[test]
    fn test_blame_with_attribution() {
        let (dir, repo) = create_test_repo();

        // Create a commit
        let commit_id = create_commit(
            &repo,
            &dir,
            "test.rs",
            "fn hello() {\n    println!(\"hi\");\n}\n",
        );

        // Store attribution for this commit
        let notes_store = NotesStore::new(&repo).unwrap();
        let attribution = AIAttribution {
            version: 2,
            session: SessionMetadata {
                session_id: "test-session".to_string(),
                model: ModelInfo::claude("test-model"),
                started_at: "2026-01-30T10:00:00Z".to_string(),
                prompt_count: 1,
            },
            prompts: vec![PromptInfo {
                index: 0,
                text: "Create hello function with greeting".to_string(),
                timestamp: "2026-01-30T10:00:00Z".to_string(),
                affected_files: vec!["test.rs".to_string()],
            }],
            files: vec![FileAttributionResult {
                path: "test.rs".to_string(),
                lines: vec![
                    LineAttribution {
                        line_number: 1,
                        content: "fn hello() {".to_string(),
                        source: LineSource::AI {
                            edit_id: "e1".to_string(),
                        },
                        edit_id: Some("e1".to_string()),
                        prompt_index: Some(0),
                        confidence: 1.0,
                    },
                    LineAttribution {
                        line_number: 2,
                        content: "    println!(\"hi\");".to_string(),
                        source: LineSource::AI {
                            edit_id: "e1".to_string(),
                        },
                        edit_id: Some("e1".to_string()),
                        prompt_index: Some(0),
                        confidence: 1.0,
                    },
                    LineAttribution {
                        line_number: 3,
                        content: "}".to_string(),
                        source: LineSource::AI {
                            edit_id: "e1".to_string(),
                        },
                        edit_id: Some("e1".to_string()),
                        prompt_index: Some(0),
                        confidence: 1.0,
                    },
                ],
                summary: AttributionSummary {
                    total_lines: 3,
                    ai_lines: 3,
                    ai_modified_lines: 0,
                    human_lines: 0,
                    original_lines: 0,
                    unknown_lines: 0,
                },
            }],
        };

        notes_store
            .store_attribution(commit_id, &attribution)
            .unwrap();

        // Run blame
        let mut blamer = AIBlamer::new(&repo).unwrap();
        let result = blamer.blame("test.rs", None).unwrap();

        // All lines should now have AI attribution
        for line in &result.lines {
            assert!(
                line.source.is_ai(),
                "Line {} should be AI",
                line.line_number
            );
            assert_eq!(line.prompt_index, Some(0));
            assert!(line.prompt_preview.is_some());
            assert!(line.prompt_preview.as_ref().unwrap().contains("hello"));
        }
    }

    #[test]
    fn test_get_commit_attribution_caching() {
        let (dir, repo) = create_test_repo();

        let commit_id = create_commit(&repo, &dir, "test.rs", "fn test() {}\n");

        // Store attribution
        let notes_store = NotesStore::new(&repo).unwrap();
        let attribution = AIAttribution {
            version: 2,
            session: SessionMetadata {
                session_id: "cache-test".to_string(),
                model: ModelInfo::claude("test-model"),
                started_at: "2026-01-30T10:00:00Z".to_string(),
                prompt_count: 1,
            },
            prompts: vec![],
            files: vec![],
        };
        notes_store
            .store_attribution(commit_id, &attribution)
            .unwrap();

        // Create blamer and fetch attribution twice
        let mut blamer = AIBlamer::new(&repo).unwrap();

        let commit_str = commit_id.to_string();

        // First fetch - should go to notes store
        let result1 = blamer.get_commit_attribution(&commit_str).unwrap();
        assert!(result1.is_some());
        assert_eq!(result1.unwrap().session.session_id, "cache-test");

        // Second fetch - should come from cache
        let result2 = blamer.get_commit_attribution(&commit_str).unwrap();
        assert!(result2.is_some());
        assert_eq!(result2.unwrap().session.session_id, "cache-test");

        // Verify it was cached
        assert!(blamer.attribution_cache.contains_key(&commit_str));
    }
}
