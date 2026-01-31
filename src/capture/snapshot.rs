use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::utils::{hex, CONTENT_HASH_BYTES};

/// A point-in-time snapshot of a file's content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentSnapshot {
    /// Full file content at this point
    pub content: String,
    /// SHA-256 hash of content for quick comparison
    pub content_hash: String,
    /// When this snapshot was taken
    pub timestamp: String,
    /// Line count at this snapshot
    pub line_count: usize,
}

impl ContentSnapshot {
    pub fn new(content: &str) -> Self {
        Self {
            content: content.to_string(),
            content_hash: compute_hash(content),
            timestamp: Utc::now().to_rfc3339(),
            line_count: content.lines().count(),
        }
    }

    pub fn empty() -> Self {
        Self::new("")
    }

    pub fn lines(&self) -> Vec<&str> {
        self.content.lines().collect()
    }
}

/// Represents a single AI edit operation on a file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIEdit {
    /// Unique ID for this edit
    pub edit_id: String,
    /// The prompt that triggered this edit
    pub prompt: String,
    /// Prompt index within the session
    pub prompt_index: u32,
    /// Tool used (Edit, Write)
    pub tool: String,
    /// Content BEFORE this edit
    pub before: ContentSnapshot,
    /// Content AFTER this edit
    pub after: ContentSnapshot,
    /// Timestamp of this edit
    pub timestamp: String,
}

impl AIEdit {
    pub fn new(
        prompt: &str,
        prompt_index: u32,
        tool: &str,
        before_content: &str,
        after_content: &str,
    ) -> Self {
        Self {
            edit_id: uuid::Uuid::new_v4().to_string(),
            prompt: prompt.to_string(),
            prompt_index,
            tool: tool.to_string(),
            before: ContentSnapshot::new(before_content),
            after: ContentSnapshot::new(after_content),
            timestamp: Utc::now().to_rfc3339(),
        }
    }
}

/// Tracks the complete edit history for a single file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEditHistory {
    /// File path relative to repo root
    pub path: String,
    /// Original content when tracking started (before any AI edits)
    pub original: ContentSnapshot,
    /// Ordered list of AI edits
    pub edits: Vec<AIEdit>,
    /// Whether file existed before tracking
    pub was_new_file: bool,
}

impl FileEditHistory {
    pub fn new(path: &str, original_content: Option<&str>) -> Self {
        let (original, was_new) = match original_content {
            Some(content) => (ContentSnapshot::new(content), false),
            None => (ContentSnapshot::empty(), true),
        };

        Self {
            path: path.to_string(),
            original,
            edits: Vec::new(),
            was_new_file: was_new,
        }
    }

    /// Add an AI edit to the history
    pub fn add_edit(&mut self, edit: AIEdit) {
        self.edits.push(edit);
    }

    /// Get the content after all AI edits
    pub fn latest_ai_content(&self) -> &ContentSnapshot {
        self.edits
            .last()
            .map(|e| &e.after)
            .unwrap_or(&self.original)
    }

    /// Get all unique prompts used for this file
    pub fn prompts(&self) -> Vec<&str> {
        self.edits.iter().map(|e| e.prompt.as_str()).collect()
    }

    /// Check if content matches any AI snapshot
    pub fn find_matching_edit(&self, content_hash: &str) -> Option<&AIEdit> {
        self.edits
            .iter()
            .find(|e| e.after.content_hash == content_hash)
    }
}

/// Result of line-level attribution analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineAttribution {
    /// Line number (1-indexed)
    pub line_number: u32,
    /// The actual line content
    pub content: String,
    /// Attribution source
    pub source: LineSource,
    /// If AI-generated, which edit created it
    pub edit_id: Option<String>,
    /// If AI-generated, the prompt index
    pub prompt_index: Option<u32>,
    /// Confidence in the attribution (0.0-1.0)
    pub confidence: f64,
}

/// Source of a line
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum LineSource {
    /// Line existed before any AI edits (original/human)
    Original,
    /// Line was added by AI and unchanged
    AI { edit_id: String },
    /// Line was added by AI but modified by human
    AIModified { edit_id: String, similarity: f64 },
    /// Line was added by human after AI edits
    Human,
    /// Unable to determine source
    Unknown,
}

impl LineSource {
    pub fn is_ai(&self) -> bool {
        matches!(self, LineSource::AI { .. } | LineSource::AIModified { .. })
    }

    pub fn is_human(&self) -> bool {
        matches!(self, LineSource::Original | LineSource::Human)
    }
}

/// Result of analyzing a file's final state against its edit history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAttributionResult {
    pub path: String,
    pub lines: Vec<LineAttribution>,
    pub summary: AttributionSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributionSummary {
    pub total_lines: usize,
    pub ai_lines: usize,
    pub ai_modified_lines: usize,
    pub human_lines: usize,
    pub original_lines: usize,
    pub unknown_lines: usize,
}

impl FileAttributionResult {
    pub fn compute_summary(lines: &[LineAttribution]) -> AttributionSummary {
        let mut summary = AttributionSummary {
            total_lines: lines.len(),
            ai_lines: 0,
            ai_modified_lines: 0,
            human_lines: 0,
            original_lines: 0,
            unknown_lines: 0,
        };

        for line in lines {
            match &line.source {
                LineSource::Original => summary.original_lines += 1,
                LineSource::AI { .. } => summary.ai_lines += 1,
                LineSource::AIModified { .. } => summary.ai_modified_lines += 1,
                LineSource::Human => summary.human_lines += 1,
                LineSource::Unknown => summary.unknown_lines += 1,
            }
        }

        summary
    }
}

/// Compute SHA-256 hash of content
pub fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..CONTENT_HASH_BYTES])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_snapshot() {
        let snapshot = ContentSnapshot::new("line1\nline2\nline3");
        assert_eq!(snapshot.line_count, 3);
        assert!(!snapshot.content_hash.is_empty());
    }

    #[test]
    fn test_content_hash_consistency() {
        let content = "hello world";
        let hash1 = compute_hash(content);
        let hash2 = compute_hash(content);
        assert_eq!(hash1, hash2);

        let hash3 = compute_hash("different");
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_file_edit_history() {
        let mut history = FileEditHistory::new("test.rs", Some("original content"));
        assert!(!history.was_new_file);
        assert_eq!(history.original.content, "original content");

        let edit = AIEdit::new("Add function", 0, "Edit", "original content", "new content");
        history.add_edit(edit);

        assert_eq!(history.edits.len(), 1);
        assert_eq!(history.latest_ai_content().content, "new content");
    }

    #[test]
    fn test_new_file_history() {
        let history = FileEditHistory::new("new.rs", None);
        assert!(history.was_new_file);
        assert!(history.original.content.is_empty());
    }
}
