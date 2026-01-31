use std::collections::HashMap;

use anyhow::{Context, Result};
use git2::{BlameOptions, Repository};

use crate::capture::snapshot::LineSource;
use crate::core::attribution::{AIAttribution, BlameLineResult, BlameResult};
use crate::storage::notes::NotesStore;

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
                let commit_short = commit_id[..7.min(commit_id.len())].to_string();

                // Get author from the blame hunk signature
                let author = hunk
                    .final_signature()
                    .name()
                    .unwrap_or("Unknown")
                    .to_string();

                // Original line in the original file (for attribution lookup)
                let original_line =
                    hunk.orig_start_line() as u32 + (line_number - hunk.final_start_line() as u32);

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
                if let Some(line_attr) = file_attr
                    .lines
                    .iter()
                    .find(|l| l.line_number == line)
                {
                    // Get prompt preview if available
                    let prompt_preview = line_attr.prompt_index.and_then(|idx| {
                        attribution
                            .get_prompt(idx)
                            .map(|p| truncate_prompt(&p.text, 60))
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

/// Truncate prompt text for preview
fn truncate_prompt(text: &str, max_len: usize) -> String {
    let trimmed = text.trim();
    if trimmed.len() <= max_len {
        trimmed.to_string()
    } else {
        format!("{}...", &trimmed[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    // Integration tests would go here with fixture repos
}
