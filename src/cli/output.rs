use clap::ValueEnum;
use colored::Colorize;

use crate::capture::snapshot::LineSource;
use crate::core::attribution::BlameResult;
use crate::utils::{truncate, truncate_or_pad};

/// Output format options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable terminal output with colors
    #[default]
    Pretty,
    /// JSON output for machine consumption
    Json,
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
                "line": line.line_number,
                "commit": line.commit_id,
                "author": line.author,
                "source": format!("{:?}", line.source),
                "is_ai": line.source.is_ai(),
                "is_human": line.source.is_human(),
                "prompt_index": line.prompt_index,
                "prompt_preview": line.prompt_preview,
                "content": line.content,
            })
        })
        .collect();

    serde_json::to_string_pretty(&serde_json::json!({
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
}
