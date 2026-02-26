use clap::ValueEnum;
use colored::Colorize;
use serde::Serialize;

use crate::capture::snapshot::LineSource;
use crate::core::attribution::BlameResult;
use crate::utils::{truncate, truncate_or_pad};

/// Schema version for machine-readable CLI outputs.
pub const MACHINE_OUTPUT_SCHEMA_VERSION: u8 = 1;

/// Output format options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable terminal output with colors
    #[default]
    Pretty,
    /// JSON output for machine consumption
    Json,
}

/// Stable JSON representation of line attribution source for machine output.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LineSourceOutput {
    Original,
    Ai { edit_id: String },
    AiModified { edit_id: String, similarity: f64 },
    Human,
    Unknown,
}

impl From<&LineSource> for LineSourceOutput {
    fn from(source: &LineSource) -> Self {
        match source {
            LineSource::Original => Self::Original,
            LineSource::AI { edit_id } => Self::Ai {
                edit_id: edit_id.clone(),
            },
            LineSource::AIModified {
                edit_id,
                similarity,
            } => Self::AiModified {
                edit_id: edit_id.clone(),
                similarity: *similarity,
            },
            LineSource::Human => Self::Human,
            LineSource::Unknown => Self::Unknown,
        }
    }
}

/// Format blame results for display
pub fn format_blame(result: &BlameResult, format: OutputFormat) -> String {
    match format {
        OutputFormat::Pretty => format_blame_pretty(result),
        OutputFormat::Json => format_blame_json(result),
    }
}

fn format_blame_pretty(result: &BlameResult) -> String {
    let mut output = String::new();

    // Header
    output.push_str(&format!(
        "\n {} {} │ {} │ {} │ {} │ {}\n",
        "LINE".dimmed(),
        " ".repeat(2),
        "COMMIT ".dimmed(),
        "AUTHOR     ".dimmed(),
        "SRC".dimmed(),
        "CODE".dimmed()
    ));
    output.push_str(&format!("{}\n", "─".repeat(85).dimmed()));

    // Lines
    for line in &result.lines {
        let line_num = format!("{:>5}", line.line_number);
        let commit = &line.commit_short;
        let author = truncate_or_pad(&line.author, 10);

        // Source marker with different symbols for different sources
        let source_marker = match &line.source {
            LineSource::AI { .. } => "●".green().bold().to_string(),
            LineSource::AIModified { .. } => "◐".yellow().to_string(),
            LineSource::Human => "+".blue().to_string(),
            LineSource::Original => "─".dimmed().to_string(),
            LineSource::Unknown => "?".dimmed().to_string(),
        };

        // Truncate long lines
        let code = truncate(&line.content, 50);

        let formatted_line = format!(
            "{} │ {} │ {} │  {} │ {}\n",
            line_num.dimmed(),
            commit.yellow(),
            author,
            source_marker,
            code
        );

        output.push_str(&formatted_line);
    }

    // Footer with summary
    let ai_count = result.ai_line_count();
    let ai_modified_count = result.ai_modified_line_count();
    let human_count = result.human_line_count();
    let original_count = result.original_line_count();
    let percentage = result.ai_percentage();

    output.push_str(&format!("{}\n", "─".repeat(85).dimmed()));

    output.push_str(&format!(
        "Legend: {} AI ({}) {} AI-modified ({}) {} Human ({}) {} Original ({})\n",
        "●".green().bold(),
        ai_count,
        "◐".yellow(),
        ai_modified_count,
        "+".blue(),
        human_count,
        "─".dimmed(),
        original_count,
    ));
    output.push_str(&format!(
        "AI involvement: {:.0}% ({} of {} lines)\n",
        percentage,
        ai_count + ai_modified_count,
        result.lines.len()
    ));

    // Show first prompt preview if available
    if let Some(line) = result.lines.iter().find(|l| l.prompt_preview.is_some()) {
        if let Some(preview) = &line.prompt_preview {
            output.push_str(&format!("First AI prompt: \"{}\"\n", preview.dimmed()));
        }
    }

    output
}

fn format_blame_json(result: &BlameResult) -> String {
    let json_output: Vec<serde_json::Value> = result
        .lines
        .iter()
        .map(|line| {
            serde_json::json!({
                "line_number": line.line_number,
                // Deprecated alias retained for compatibility.
                "line": line.line_number,
                "commit": {
                    "id": line.commit_id,
                    "short": line.commit_short,
                    "author": line.author,
                },
                "source": LineSourceOutput::from(&line.source),
                "flags": {
                    "is_ai": line.source.is_ai(),
                    "is_human": line.source.is_human(),
                },
                "prompt": {
                    "index": line.prompt_index,
                    "preview": line.prompt_preview,
                },
                "content": line.content,
            })
        })
        .collect();

    serde_json::to_string_pretty(&serde_json::json!({
        "schema_version": MACHINE_OUTPUT_SCHEMA_VERSION,
        "schema": "whogitit.blame.v1",
        "file": result.path,
        "revision": result.revision,
        "lines": json_output,
        "summary": {
            "total_lines": result.lines.len(),
            "ai_lines": result.pure_ai_line_count(),
            "ai_modified_lines": result.ai_modified_line_count(),
            "human_lines": result.human_line_count(),
            "original_lines": result.original_line_count(),
            "ai_percentage": result.ai_percentage(),
        }
    }))
    .unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::attribution::{BlameLineResult, BlameResult};

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
    }

    #[test]
    fn test_truncate_or_pad() {
        assert_eq!(truncate_or_pad("hi", 5), "hi   ");
        assert_eq!(truncate_or_pad("hello world", 5), "hell…");
    }

    #[test]
    fn test_line_source_output_ai_modified() {
        let source = LineSource::AIModified {
            edit_id: "e1".to_string(),
            similarity: 0.75,
        };
        let output = LineSourceOutput::from(&source);
        assert!(matches!(output, LineSourceOutput::AiModified { .. }));
    }

    #[test]
    fn test_blame_json_has_schema_version_and_structured_source() {
        let result = BlameResult {
            path: "src/main.rs".to_string(),
            revision: "HEAD".to_string(),
            lines: vec![BlameLineResult {
                line_number: 1,
                content: "fn main() {}".to_string(),
                commit_id: "abc1234567".to_string(),
                commit_short: "abc1234".to_string(),
                author: "Test".to_string(),
                source: LineSource::AI {
                    edit_id: "edit-1".to_string(),
                },
                prompt_index: Some(0),
                prompt_preview: Some("prompt".to_string()),
            }],
        };

        let output = format_blame_json(&result);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(
            parsed["schema_version"],
            serde_json::Value::from(MACHINE_OUTPUT_SCHEMA_VERSION)
        );
        assert_eq!(parsed["schema"], "whogitit.blame.v1");
        assert_eq!(parsed["lines"][0]["source"]["type"], "ai");
        assert_eq!(parsed["lines"][0]["source"]["edit_id"], "edit-1");
    }
}
