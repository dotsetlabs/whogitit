use serde::{Deserialize, Serialize};

use crate::capture::snapshot::{FileAttributionResult, LineSource};

/// Schema version for the attribution format (3 = with edit context)
pub const SCHEMA_VERSION: u8 = 3;

/// Core attribution data attached to commits via git notes
///
/// Stores complete three-way diff analysis results, enabling accurate
/// attribution even when users modify AI-generated code before committing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIAttribution {
    /// Schema version
    pub version: u8,
    /// AI session metadata
    pub session: SessionMetadata,
    /// All prompts used in this session
    pub prompts: Vec<PromptInfo>,
    /// Per-file attribution results from three-way analysis
    pub files: Vec<FileAttributionResult>,
}

impl AIAttribution {
    /// Count total AI-generated lines across all files
    pub fn total_ai_lines(&self) -> usize {
        self.files.iter().map(|f| f.summary.ai_lines).sum()
    }

    /// Count total AI-modified lines (AI code edited by human)
    pub fn total_ai_modified_lines(&self) -> usize {
        self.files.iter().map(|f| f.summary.ai_modified_lines).sum()
    }

    /// Count total human-added lines
    pub fn total_human_lines(&self) -> usize {
        self.files.iter().map(|f| f.summary.human_lines).sum()
    }

    /// Count total original lines (unchanged from before AI edits)
    pub fn total_original_lines(&self) -> usize {
        self.files.iter().map(|f| f.summary.original_lines).sum()
    }

    /// Get prompt by index
    pub fn get_prompt(&self, index: u32) -> Option<&PromptInfo> {
        self.prompts.iter().find(|p| p.index == index)
    }
}

/// Information about a prompt in the session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptInfo {
    /// Prompt index within the session
    pub index: u32,
    /// Full prompt text (potentially redacted)
    pub text: String,
    /// Timestamp when prompt was processed
    pub timestamp: String,
    /// Files affected by this prompt
    pub affected_files: Vec<String>,
}

/// Metadata about the AI session that generated the code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// Unique session identifier (UUID)
    pub session_id: String,
    /// Model information
    pub model: ModelInfo,
    /// When the session started (ISO 8601)
    pub started_at: String,
    /// Number of prompts in this session
    pub prompt_count: u32,
    /// Whether plan mode was used in this session
    #[serde(default)]
    pub used_plan_mode: bool,
    /// Number of subagents spawned during this session
    #[serde(default)]
    pub subagent_count: u32,
}

/// Information about the AI model used
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model identifier (e.g., "claude-opus-4-5-20251101")
    pub id: String,
    /// Provider name (e.g., "anthropic")
    pub provider: String,
}

impl ModelInfo {
    pub fn claude(model_id: &str) -> Self {
        Self {
            id: model_id.to_string(),
            provider: "anthropic".to_string(),
        }
    }
}

/// Result of blame operation for a single line
#[derive(Debug, Clone)]
pub struct BlameLineResult {
    /// Line number (1-indexed)
    pub line_number: u32,
    /// The actual line content
    pub content: String,
    /// Git commit that last modified this line
    pub commit_id: String,
    /// Short commit hash (7 chars)
    pub commit_short: String,
    /// Author name
    pub author: String,
    /// Line source (AI, Human, Original, AIModified)
    pub source: LineSource,
    /// If AI-generated, the prompt index
    pub prompt_index: Option<u32>,
    /// Prompt text preview if available
    pub prompt_preview: Option<String>,
}

impl BlameLineResult {
    /// Check if this line was AI-generated (AI or AIModified)
    pub fn is_ai(&self) -> bool {
        self.source.is_ai()
    }

    /// Check if this line was human-written (Human or Original)
    pub fn is_human(&self) -> bool {
        self.source.is_human()
    }
}

/// Result of blame operation for an entire file
#[derive(Debug)]
pub struct BlameResult {
    /// File path
    pub path: String,
    /// Revision blamed against
    pub revision: String,
    /// Per-line results
    pub lines: Vec<BlameLineResult>,
}

impl BlameResult {
    /// Count AI-generated lines (AI + AIModified)
    pub fn ai_line_count(&self) -> usize {
        self.lines.iter().filter(|l| l.source.is_ai()).count()
    }

    /// Count pure AI lines (not modified by human)
    pub fn pure_ai_line_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| matches!(l.source, LineSource::AI { .. }))
            .count()
    }

    /// Count AI-modified lines
    pub fn ai_modified_line_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| matches!(l.source, LineSource::AIModified { .. }))
            .count()
    }

    /// Count human-added lines
    pub fn human_line_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| matches!(l.source, LineSource::Human))
            .count()
    }

    /// Count original lines (unchanged)
    pub fn original_line_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| matches!(l.source, LineSource::Original))
            .count()
    }

    /// Calculate percentage of AI-generated lines
    pub fn ai_percentage(&self) -> f64 {
        if self.lines.is_empty() {
            0.0
        } else {
            (self.ai_line_count() as f64 / self.lines.len() as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::snapshot::{AttributionSummary, LineAttribution};

    #[test]
    fn test_ai_attribution_totals() {
        let attribution = AIAttribution {
            version: 2,
            session: SessionMetadata {
                session_id: "test-123".to_string(),
                model: ModelInfo::claude("claude-opus-4-5-20251101"),
                started_at: "2026-01-30T10:00:00Z".to_string(),
                prompt_count: 1,
                used_plan_mode: false,
                subagent_count: 0,
            },
            prompts: vec![],
            files: vec![FileAttributionResult {
                path: "test.rs".to_string(),
                lines: vec![],
                summary: AttributionSummary {
                    total_lines: 10,
                    ai_lines: 5,
                    ai_modified_lines: 2,
                    human_lines: 2,
                    original_lines: 1,
                    unknown_lines: 0,
                },
            }],
        };

        assert_eq!(attribution.total_ai_lines(), 5);
        assert_eq!(attribution.total_ai_modified_lines(), 2);
        assert_eq!(attribution.total_human_lines(), 2);
        assert_eq!(attribution.total_original_lines(), 1);
    }

    #[test]
    fn test_blame_result_counts() {
        let result = BlameResult {
            path: "test.rs".to_string(),
            revision: "HEAD".to_string(),
            lines: vec![
                BlameLineResult {
                    line_number: 1,
                    content: "line1".to_string(),
                    commit_id: "abc123".to_string(),
                    commit_short: "abc123".to_string(),
                    author: "Test".to_string(),
                    source: LineSource::AI {
                        edit_id: "e1".to_string(),
                    },
                    prompt_index: Some(0),
                    prompt_preview: None,
                },
                BlameLineResult {
                    line_number: 2,
                    content: "line2".to_string(),
                    commit_id: "abc123".to_string(),
                    commit_short: "abc123".to_string(),
                    author: "Test".to_string(),
                    source: LineSource::Human,
                    prompt_index: None,
                    prompt_preview: None,
                },
                BlameLineResult {
                    line_number: 3,
                    content: "line3".to_string(),
                    commit_id: "abc123".to_string(),
                    commit_short: "abc123".to_string(),
                    author: "Test".to_string(),
                    source: LineSource::Original,
                    prompt_index: None,
                    prompt_preview: None,
                },
            ],
        };

        assert_eq!(result.ai_line_count(), 1);
        assert_eq!(result.human_line_count(), 1);
        assert_eq!(result.original_line_count(), 1);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let attribution = AIAttribution {
            version: 2,
            session: SessionMetadata {
                session_id: "test-123".to_string(),
                model: ModelInfo::claude("claude-opus-4-5-20251101"),
                started_at: "2026-01-30T10:00:00Z".to_string(),
                prompt_count: 1,
                used_plan_mode: false,
                subagent_count: 0,
            },
            prompts: vec![PromptInfo {
                index: 0,
                text: "Add main function".to_string(),
                timestamp: "2026-01-30T10:00:00Z".to_string(),
                affected_files: vec!["test.rs".to_string()],
            }],
            files: vec![FileAttributionResult {
                path: "test.rs".to_string(),
                lines: vec![LineAttribution {
                    line_number: 1,
                    content: "fn main() {}".to_string(),
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

        let json = serde_json::to_string(&attribution).unwrap();
        let parsed: AIAttribution = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, 2);
        assert_eq!(parsed.session.session_id, "test-123");
        assert_eq!(parsed.files.len(), 1);
        assert_eq!(parsed.prompts.len(), 1);
    }

    #[test]
    fn test_blame_result_ai_percentage() {
        let result = BlameResult {
            path: "test.rs".to_string(),
            revision: "HEAD".to_string(),
            lines: vec![
                create_test_line(
                    1,
                    LineSource::AI {
                        edit_id: "e1".to_string(),
                    },
                ),
                create_test_line(
                    2,
                    LineSource::AI {
                        edit_id: "e1".to_string(),
                    },
                ),
                create_test_line(3, LineSource::Human),
                create_test_line(4, LineSource::Original),
            ],
        };

        // 2 AI out of 4 = 50%
        assert!((result.ai_percentage() - 50.0).abs() < 0.001);
    }

    #[test]
    fn test_blame_result_ai_percentage_empty() {
        let result = BlameResult {
            path: "test.rs".to_string(),
            revision: "HEAD".to_string(),
            lines: vec![],
        };

        assert!((result.ai_percentage() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_blame_result_ai_percentage_all_ai() {
        let result = BlameResult {
            path: "test.rs".to_string(),
            revision: "HEAD".to_string(),
            lines: vec![
                create_test_line(
                    1,
                    LineSource::AI {
                        edit_id: "e1".to_string(),
                    },
                ),
                create_test_line(
                    2,
                    LineSource::AIModified {
                        edit_id: "e1".to_string(),
                        similarity: 0.8,
                    },
                ),
            ],
        };

        // Both are AI (AI + AIModified)
        assert!((result.ai_percentage() - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_blame_result_pure_ai_vs_modified() {
        let result = BlameResult {
            path: "test.rs".to_string(),
            revision: "HEAD".to_string(),
            lines: vec![
                create_test_line(
                    1,
                    LineSource::AI {
                        edit_id: "e1".to_string(),
                    },
                ),
                create_test_line(
                    2,
                    LineSource::AI {
                        edit_id: "e1".to_string(),
                    },
                ),
                create_test_line(
                    3,
                    LineSource::AIModified {
                        edit_id: "e2".to_string(),
                        similarity: 0.9,
                    },
                ),
            ],
        };

        assert_eq!(result.pure_ai_line_count(), 2);
        assert_eq!(result.ai_modified_line_count(), 1);
        assert_eq!(result.ai_line_count(), 3); // Total AI involvement
    }

    #[test]
    fn test_get_prompt() {
        let attribution = AIAttribution {
            version: 2,
            session: SessionMetadata {
                session_id: "test-123".to_string(),
                model: ModelInfo::claude("claude-opus-4-5-20251101"),
                started_at: "2026-01-30T10:00:00Z".to_string(),
                prompt_count: 2,
                used_plan_mode: false,
                subagent_count: 0,
            },
            prompts: vec![
                PromptInfo {
                    index: 0,
                    text: "First prompt".to_string(),
                    timestamp: "2026-01-30T10:00:00Z".to_string(),
                    affected_files: vec!["file1.rs".to_string()],
                },
                PromptInfo {
                    index: 1,
                    text: "Second prompt".to_string(),
                    timestamp: "2026-01-30T10:01:00Z".to_string(),
                    affected_files: vec!["file2.rs".to_string()],
                },
            ],
            files: vec![],
        };

        assert!(attribution.get_prompt(0).is_some());
        assert_eq!(attribution.get_prompt(0).unwrap().text, "First prompt");

        assert!(attribution.get_prompt(1).is_some());
        assert_eq!(attribution.get_prompt(1).unwrap().text, "Second prompt");

        assert!(attribution.get_prompt(2).is_none());
        assert!(attribution.get_prompt(99).is_none());
    }

    #[test]
    fn test_model_info_claude() {
        let model = ModelInfo::claude("claude-opus-4-5-20251101");
        assert_eq!(model.id, "claude-opus-4-5-20251101");
        assert_eq!(model.provider, "anthropic");
    }

    #[test]
    fn test_attribution_multiple_files() {
        let attribution = AIAttribution {
            version: 2,
            session: SessionMetadata {
                session_id: "multi-file".to_string(),
                model: ModelInfo::claude("claude-opus-4-5-20251101"),
                started_at: "2026-01-30T10:00:00Z".to_string(),
                prompt_count: 1,
                used_plan_mode: false,
                subagent_count: 0,
            },
            prompts: vec![],
            files: vec![
                FileAttributionResult {
                    path: "file1.rs".to_string(),
                    lines: vec![],
                    summary: AttributionSummary {
                        total_lines: 10,
                        ai_lines: 5,
                        ai_modified_lines: 2,
                        human_lines: 2,
                        original_lines: 1,
                        unknown_lines: 0,
                    },
                },
                FileAttributionResult {
                    path: "file2.rs".to_string(),
                    lines: vec![],
                    summary: AttributionSummary {
                        total_lines: 20,
                        ai_lines: 10,
                        ai_modified_lines: 3,
                        human_lines: 5,
                        original_lines: 2,
                        unknown_lines: 0,
                    },
                },
            ],
        };

        // Aggregates across all files
        assert_eq!(attribution.total_ai_lines(), 15); // 5 + 10
        assert_eq!(attribution.total_ai_modified_lines(), 5); // 2 + 3
        assert_eq!(attribution.total_human_lines(), 7); // 2 + 5
        assert_eq!(attribution.total_original_lines(), 3); // 1 + 2
    }

    // Helper function
    fn create_test_line(line_num: u32, source: LineSource) -> BlameLineResult {
        BlameLineResult {
            line_number: line_num,
            content: format!("line{}", line_num),
            commit_id: "abc123".to_string(),
            commit_short: "abc123".to_string(),
            author: "Test".to_string(),
            source,
            prompt_index: None,
            prompt_preview: None,
        }
    }
}
