use sha2::{Digest, Sha256};
use similar::{ChangeTag, TextDiff};

use crate::utils::{hex, DIFF_HASH_BYTES};

/// A hunk of changed lines in a diff
#[derive(Debug, Clone)]
pub struct DiffHunk {
    /// Starting line in the new file (1-indexed)
    pub new_start: u32,
    /// Number of lines in the new file
    pub new_count: u32,
    /// The actual new lines content
    pub content: Vec<String>,
}

impl DiffHunk {
    /// Compute SHA-256 hash of the content
    pub fn content_hash(&self) -> String {
        let mut hasher = Sha256::new();
        for line in &self.content {
            hasher.update(line.as_bytes());
            hasher.update(b"\n");
        }
        let result = hasher.finalize();
        hex::encode(&result[..DIFF_HASH_BYTES])
    }
}

/// Result of computing a diff
#[derive(Debug)]
pub struct DiffResult {
    /// Added/modified hunks
    pub hunks: Vec<DiffHunk>,
    /// Total lines added
    pub lines_added: u32,
    /// Total lines removed
    pub lines_removed: u32,
}

/// Compute line-level diff between old and new content
pub fn compute_diff(old_content: &str, new_content: &str) -> DiffResult {
    let diff = TextDiff::from_lines(old_content, new_content);

    let mut hunks = Vec::new();
    let mut lines_added = 0u32;
    let mut lines_removed = 0u32;

    let mut current_hunk: Option<DiffHunk> = None;
    let mut new_line_num = 0u32;

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                // Flush current hunk if exists
                if let Some(hunk) = current_hunk.take() {
                    if !hunk.content.is_empty() {
                        hunks.push(hunk);
                    }
                }
                new_line_num += 1;
            }
            ChangeTag::Insert => {
                new_line_num += 1;
                lines_added += 1;

                let line = change.value().trim_end_matches('\n').to_string();

                if let Some(ref mut hunk) = current_hunk {
                    // Extend existing hunk if contiguous
                    if hunk.new_start + hunk.new_count == new_line_num {
                        hunk.new_count += 1;
                        hunk.content.push(line);
                    } else {
                        // Start new hunk
                        let old_hunk = current_hunk.take().unwrap();
                        if !old_hunk.content.is_empty() {
                            hunks.push(old_hunk);
                        }
                        current_hunk = Some(DiffHunk {
                            new_start: new_line_num,
                            new_count: 1,
                            content: vec![line],
                        });
                    }
                } else {
                    // Start new hunk
                    current_hunk = Some(DiffHunk {
                        new_start: new_line_num,
                        new_count: 1,
                        content: vec![line],
                    });
                }
            }
            ChangeTag::Delete => {
                lines_removed += 1;
                // Deletions don't advance new line number
            }
        }
    }

    // Flush final hunk
    if let Some(hunk) = current_hunk {
        if !hunk.content.is_empty() {
            hunks.push(hunk);
        }
    }

    DiffResult {
        hunks,
        lines_added,
        lines_removed,
    }
}

/// Compute diff for a newly created file (all lines are additions)
pub fn compute_create_diff(content: &str) -> DiffResult {
    compute_diff("", content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_addition() {
        let old = "line1\nline2\n";
        let new = "line1\nline2\nline3\n";

        let result = compute_diff(old, new);

        assert_eq!(result.lines_added, 1);
        assert_eq!(result.lines_removed, 0);
        assert_eq!(result.hunks.len(), 1);
        assert_eq!(result.hunks[0].new_start, 3);
        assert_eq!(result.hunks[0].new_count, 1);
        assert_eq!(result.hunks[0].content, vec!["line3"]);
    }

    #[test]
    fn test_insertion_in_middle() {
        let old = "line1\nline3\n";
        let new = "line1\nline2\nline3\n";

        let result = compute_diff(old, new);

        assert_eq!(result.lines_added, 1);
        assert_eq!(result.hunks.len(), 1);
        assert_eq!(result.hunks[0].new_start, 2);
    }

    #[test]
    fn test_multiple_hunks() {
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nX\nb\nc\nY\nd\ne\n";

        let result = compute_diff(old, new);

        assert_eq!(result.lines_added, 2);
        assert_eq!(result.hunks.len(), 2);
        assert_eq!(result.hunks[0].new_start, 2);
        assert_eq!(result.hunks[0].content, vec!["X"]);
        assert_eq!(result.hunks[1].new_start, 5);
        assert_eq!(result.hunks[1].content, vec!["Y"]);
    }

    #[test]
    fn test_create_diff() {
        let content = "line1\nline2\nline3\n";
        let result = compute_create_diff(content);

        assert_eq!(result.lines_added, 3);
        assert_eq!(result.hunks.len(), 1);
        assert_eq!(result.hunks[0].new_start, 1);
        assert_eq!(result.hunks[0].new_count, 3);
    }

    #[test]
    fn test_content_hash() {
        let hunk = DiffHunk {
            new_start: 1,
            new_count: 2,
            content: vec!["hello".to_string(), "world".to_string()],
        };

        let hash = hunk.content_hash();
        assert_eq!(hash.len(), 16); // 8 bytes = 16 hex chars

        // Same content should produce same hash
        let hunk2 = DiffHunk {
            new_start: 100,
            new_count: 2,
            content: vec!["hello".to_string(), "world".to_string()],
        };
        assert_eq!(hunk.content_hash(), hunk2.content_hash());
    }

    #[test]
    fn test_replacement() {
        let old = "old line\n";
        let new = "new line\n";

        let result = compute_diff(old, new);

        assert_eq!(result.lines_added, 1);
        assert_eq!(result.lines_removed, 1);
        assert_eq!(result.hunks.len(), 1);
        assert_eq!(result.hunks[0].content, vec!["new line"]);
    }
}
