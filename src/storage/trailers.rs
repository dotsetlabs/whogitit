use crate::core::attribution::AIAttribution;

/// Git trailer keys used for AI attribution
pub mod keys {
    pub const AI_SESSION: &str = "AI-Session";
    pub const AI_MODEL: &str = "AI-Model";
    pub const AI_LINES: &str = "AI-Lines";
    pub const AI_MODIFIED: &str = "AI-Modified";
    pub const HUMAN_LINES: &str = "Human-Lines";
    pub const CO_AUTHORED_BY: &str = "Co-Authored-By";
}

/// Generates git trailers from attribution data
pub struct TrailerGenerator;

impl TrailerGenerator {
    /// Generate trailers for a commit message
    pub fn generate(attribution: &AIAttribution) -> Vec<(String, String)> {
        let mut trailers = Vec::new();

        // Session ID (first 12 chars)
        let session_short = if attribution.session.session_id.len() > 12 {
            &attribution.session.session_id[..12]
        } else {
            &attribution.session.session_id
        };
        trailers.push((keys::AI_SESSION.to_string(), session_short.to_string()));

        // Model info
        trailers.push((
            keys::AI_MODEL.to_string(),
            attribution.session.model.id.clone(),
        ));

        // Total AI lines
        let ai_lines = attribution.total_ai_lines();
        trailers.push((keys::AI_LINES.to_string(), ai_lines.to_string()));

        // AI-modified lines (if any)
        let ai_modified = attribution.total_ai_modified_lines();
        if ai_modified > 0 {
            trailers.push((keys::AI_MODIFIED.to_string(), ai_modified.to_string()));
        }

        // Human-added lines (if any)
        let human_lines = attribution.total_human_lines();
        if human_lines > 0 {
            trailers.push((keys::HUMAN_LINES.to_string(), human_lines.to_string()));
        }

        // Co-author based on model
        let co_author = format_co_author(&attribution.session.model.id);
        trailers.push((keys::CO_AUTHORED_BY.to_string(), co_author));

        trailers
    }

    /// Format trailers as a string to append to commit message
    pub fn format_for_message(attribution: &AIAttribution) -> String {
        let trailers = Self::generate(attribution);
        trailers
            .into_iter()
            .map(|(key, value)| format!("{}: {}", key, value))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Append trailers to an existing commit message
    pub fn append_to_message(message: &str, attribution: &AIAttribution) -> String {
        let trailer_block = Self::format_for_message(attribution);
        let trimmed = message.trim_end();

        if has_existing_trailers(trimmed) {
            format!("{}\n{}", trimmed, trailer_block)
        } else {
            format!("{}\n\n{}", trimmed, trailer_block)
        }
    }
}

/// Parse trailers from a commit message
pub struct TrailerParser;

impl TrailerParser {
    /// Extract AI-related trailers from a commit message
    pub fn parse(message: &str) -> ParsedTrailers {
        let mut result = ParsedTrailers::default();

        for line in message.lines().rev() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if !line.contains(": ") {
                break;
            }

            if let Some((key, value)) = line.split_once(": ") {
                match key {
                    k if k == keys::AI_SESSION => result.session = Some(value.to_string()),
                    k if k == keys::AI_MODEL => result.model = Some(value.to_string()),
                    k if k == keys::AI_LINES => {
                        result.ai_lines = value.parse().ok();
                    }
                    k if k == keys::AI_MODIFIED => {
                        result.ai_modified_lines = value.parse().ok();
                    }
                    k if k == keys::HUMAN_LINES => {
                        result.human_lines = value.parse().ok();
                    }
                    _ => {}
                }
            }
        }

        result
    }

    /// Check if a commit message has AI trailers
    pub fn has_ai_trailers(message: &str) -> bool {
        let parsed = Self::parse(message);
        parsed.session.is_some() || parsed.model.is_some()
    }
}

/// Parsed AI trailers from a commit message
#[derive(Debug, Default)]
pub struct ParsedTrailers {
    pub session: Option<String>,
    pub model: Option<String>,
    pub ai_lines: Option<usize>,
    pub ai_modified_lines: Option<usize>,
    pub human_lines: Option<usize>,
}

/// Format co-author string based on model
fn format_co_author(model_id: &str) -> String {
    let model_name = if model_id.contains("opus") {
        "Claude Opus 4.5"
    } else if model_id.contains("sonnet") {
        "Claude Sonnet"
    } else if model_id.contains("haiku") {
        "Claude Haiku"
    } else {
        "Claude"
    };

    format!("{} <noreply@anthropic.com>", model_name)
}

/// Check if message has existing trailers at the end
fn has_existing_trailers(message: &str) -> bool {
    let lines: Vec<&str> = message.lines().collect();
    if lines.is_empty() {
        return false;
    }

    for line in lines.iter().rev().take(5) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((key, _)) = line.split_once(": ") {
            if key.chars().all(|c| c.is_alphanumeric() || c == '-') {
                return true;
            }
        }
        break;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::snapshot::{AttributionSummary, FileAttributionResult};
    use crate::core::attribution::{ModelInfo, SessionMetadata};

    fn test_attribution() -> AIAttribution {
        AIAttribution {
            version: 2,
            session: SessionMetadata {
                session_id: "abc123-def456-ghi789".to_string(),
                model: ModelInfo::claude("claude-opus-4-5-20251101"),
                started_at: "2026-01-30T10:00:00Z".to_string(),
                prompt_count: 3,
            },
            prompts: vec![],
            files: vec![FileAttributionResult {
                path: "test.rs".to_string(),
                lines: vec![],
                summary: AttributionSummary {
                    total_lines: 20,
                    ai_lines: 10,
                    ai_modified_lines: 3,
                    human_lines: 5,
                    original_lines: 2,
                    unknown_lines: 0,
                },
            }],
        }
    }

    #[test]
    fn test_generate_trailers() {
        let attribution = test_attribution();
        let trailers = TrailerGenerator::generate(&attribution);

        assert!(trailers
            .iter()
            .any(|(k, v)| k == "AI-Session" && v == "abc123-def45"));
        assert!(trailers
            .iter()
            .any(|(k, v)| k == "AI-Model" && v == "claude-opus-4-5-20251101"));
        assert!(trailers.iter().any(|(k, v)| k == "AI-Lines" && v == "10"));
        assert!(trailers.iter().any(|(k, v)| k == "AI-Modified" && v == "3"));
        assert!(trailers.iter().any(|(k, v)| k == "Human-Lines" && v == "5"));
        assert!(trailers
            .iter()
            .any(|(k, v)| k == "Co-Authored-By" && v.contains("Claude Opus 4.5")));
    }

    #[test]
    fn test_format_for_message() {
        let attribution = test_attribution();
        let formatted = TrailerGenerator::format_for_message(&attribution);

        assert!(formatted.contains("AI-Session: abc123-def45"));
        assert!(formatted.contains("AI-Model: claude-opus-4-5-20251101"));
        assert!(formatted.contains("Co-Authored-By: Claude Opus 4.5"));
    }

    #[test]
    fn test_append_to_message() {
        let attribution = test_attribution();
        let message = "Add new feature\n\nThis adds the feature.";
        let result = TrailerGenerator::append_to_message(message, &attribution);

        assert!(result.starts_with("Add new feature"));
        assert!(result.contains("\n\nAI-Session:"));
    }

    #[test]
    fn test_parse_trailers() {
        let message = "Add feature\n\nAI-Session: abc123\nAI-Model: claude-opus-4-5-20251101\nAI-Lines: 42\nAI-Modified: 5";
        let parsed = TrailerParser::parse(message);

        assert_eq!(parsed.session, Some("abc123".to_string()));
        assert_eq!(parsed.model, Some("claude-opus-4-5-20251101".to_string()));
        assert_eq!(parsed.ai_lines, Some(42));
        assert_eq!(parsed.ai_modified_lines, Some(5));
    }

    #[test]
    fn test_has_ai_trailers() {
        let with_trailers = "Commit\n\nAI-Session: abc123";
        let without_trailers = "Commit\n\nJust a regular commit.";

        assert!(TrailerParser::has_ai_trailers(with_trailers));
        assert!(!TrailerParser::has_ai_trailers(without_trailers));
    }
}
