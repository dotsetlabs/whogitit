//! Annotations command - generate annotations for GitHub Checks API
//!
//! This command outputs annotation data that can be consumed by GitHub's Checks API
//! to display line-level AI attribution directly in the "Files changed" tab of PRs.

use anyhow::{Context, Result};
use clap::{Args, ValueEnum};
use colored::Colorize;
use git2::Repository;
use serde::Serialize;

use crate::capture::snapshot::LineSource;
use crate::core::attribution::BlameLineResult;
use crate::core::blame::AIBlamer;
use crate::storage::notes::NotesStore;
use crate::utils::truncate_prompt;

/// Output format for annotations
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum AnnotationsFormat {
    /// GitHub Checks API format
    #[default]
    GithubChecks,
    /// Plain JSON array
    Json,
}

/// Consolidation mode for annotations
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum ConsolidateMode {
    /// Smart consolidation: file-level for new/high-AI files, granular for mixed
    #[default]
    Auto,
    /// One annotation per file
    File,
    /// Granular line-level annotations (original behavior)
    Lines,
}

/// Annotation level (maps to GitHub Checks API annotation_level)
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AnnotationLevel {
    Notice,
    Warning,
    Failure,
}

/// A single annotation for the GitHub Checks API
#[derive(Debug, Clone, Serialize)]
pub struct CheckAnnotation {
    /// File path relative to repository root
    pub path: String,
    /// Starting line number
    pub start_line: u32,
    /// Ending line number (same as start_line for single-line annotations)
    pub end_line: u32,
    /// Annotation level (notice, warning, failure)
    pub annotation_level: AnnotationLevel,
    /// Short title for the annotation
    pub title: String,
    /// Detailed message (shown when expanded)
    pub message: String,
    /// Optional raw details (not rendered as markdown)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_details: Option<String>,
}

/// Annotations command arguments
#[derive(Debug, Args)]
pub struct AnnotationsArgs {
    /// Base commit (exclusive) - defaults to first commit if not specified
    #[arg(long)]
    pub base: Option<String>,

    /// Head commit (inclusive) - defaults to HEAD
    #[arg(long, default_value = "HEAD")]
    pub head: String,

    /// Output format
    #[arg(long, value_enum, default_value_t = AnnotationsFormat::GithubChecks)]
    pub format: AnnotationsFormat,

    /// Consolidation mode: auto (smart), file (per-file), lines (granular)
    #[arg(long, value_enum, default_value_t = ConsolidateMode::Auto)]
    pub consolidate: ConsolidateMode,

    /// AI coverage threshold for auto-consolidation (0.0-1.0, default 0.7)
    #[arg(long, default_value = "0.7")]
    pub consolidate_threshold: f64,

    /// Minimum number of AI lines to create an annotation (reduces noise)
    #[arg(long, default_value = "1")]
    pub min_lines: u32,

    /// Maximum annotations to output (GitHub limits to 50 per update)
    #[arg(long, default_value = "50")]
    pub max_annotations: usize,

    /// Only annotate pure AI lines (not AI-modified)
    #[arg(long)]
    pub ai_only: bool,
}

/// File-level statistics for consolidation decisions
struct FileStats {
    path: String,
    total_lines: usize,
    ai_lines: usize,
    ai_modified_lines: usize,
    human_lines: usize,
    original_lines: usize,
    is_new_file: bool,
    primary_prompt: Option<String>,
    prompt_count: usize,
}

impl FileStats {
    fn ai_coverage(&self) -> f64 {
        if self.total_lines == 0 {
            0.0
        } else {
            (self.ai_lines + self.ai_modified_lines) as f64 / self.total_lines as f64
        }
    }

    fn should_consolidate(&self, threshold: f64) -> bool {
        // Consolidate if:
        // 1. It's a new file (no original lines, all AI), OR
        // 2. High AI coverage AND single prompt (or no prompts to distinguish)
        self.is_new_file || (self.ai_coverage() >= threshold && self.prompt_count <= 1)
    }
}

/// Check if repository is a shallow clone
fn is_shallow_clone(repo: &Repository) -> bool {
    repo.is_shallow()
}

/// Run the annotations command
pub fn run(args: AnnotationsArgs) -> Result<()> {
    let repo = Repository::discover(".").context("Not in a git repository")?;

    // Warn about shallow clones
    if is_shallow_clone(&repo) {
        eprintln!(
            "{} Running in shallow clone mode - attribution data may be incomplete.",
            "Warning:".yellow()
        );
    }

    let notes_store = NotesStore::new(&repo)?;
    let mut blamer = AIBlamer::new(&repo)?;

    // Resolve head commit
    let head_obj = repo
        .revparse_single(&args.head)
        .with_context(|| format!("Failed to resolve: {}", args.head))?;
    let head_commit = head_obj
        .peel_to_commit()
        .with_context(|| format!("Not a valid commit: {}", args.head))?;

    // Get commits to analyze
    let mut revwalk = repo.revwalk()?;
    revwalk.push(head_commit.id())?;

    // If base is specified, exclude it and its ancestors
    if let Some(base_ref) = &args.base {
        let base_obj = repo
            .revparse_single(base_ref)
            .with_context(|| format!("Failed to resolve base: {}", base_ref))?;
        let base_commit = base_obj
            .peel_to_commit()
            .with_context(|| format!("Not a valid commit: {}", base_ref))?;
        revwalk.hide(base_commit.id())?;
    }

    // Collect all files changed in the commit range with AI attribution
    let mut files_to_annotate: Vec<String> = Vec::new();
    let mut model_info: Option<String> = None;
    let mut session_timestamp: Option<String> = None;

    for oid_result in revwalk {
        let oid = oid_result?;

        if let Ok(Some(attr)) = notes_store.fetch_attribution(oid) {
            // Capture model info from first attributed commit
            if model_info.is_none() {
                model_info = Some(attr.session.model.id.clone());
                session_timestamp = Some(attr.session.started_at.clone());
            }

            for file in &attr.files {
                if !files_to_annotate.contains(&file.path) {
                    // Only add if it has AI lines
                    if file.summary.ai_lines > 0 || file.summary.ai_modified_lines > 0 {
                        files_to_annotate.push(file.path.clone());
                    }
                }
            }
        }
    }

    // Generate annotations for each file
    let mut annotations: Vec<CheckAnnotation> = Vec::new();

    for file_path in &files_to_annotate {
        // Run blame on the file at HEAD
        let blame_result = match blamer.blame(file_path, Some(&args.head)) {
            Ok(result) => result,
            Err(_) => continue, // Skip files that can't be blamed (deleted, etc.)
        };

        // Compute file stats for consolidation decision
        let file_stats = compute_file_stats(file_path, &blame_result.lines);

        // Decide whether to consolidate based on mode
        let should_consolidate = match args.consolidate {
            ConsolidateMode::Auto => file_stats.should_consolidate(args.consolidate_threshold),
            ConsolidateMode::File => true,
            ConsolidateMode::Lines => false,
        };

        if should_consolidate {
            // Create a single file-level annotation
            if let Some(annotation) = create_file_annotation(
                &file_stats,
                model_info.as_deref(),
                session_timestamp.as_deref(),
            ) {
                annotations.push(annotation);
            }
        } else {
            // Create granular line-level annotations
            let line_annotations = create_line_annotations(
                file_path,
                &blame_result.lines,
                args.ai_only,
                args.min_lines,
                model_info.as_deref(),
                session_timestamp.as_deref(),
            );
            annotations.extend(line_annotations);
        }

        // Stop if we've hit the max
        if annotations.len() >= args.max_annotations {
            annotations.truncate(args.max_annotations);
            break;
        }
    }

    // Output based on format
    match args.format {
        AnnotationsFormat::GithubChecks => {
            let output = GithubChecksOutput {
                annotations,
                summary: GithubChecksSummary {
                    files_analyzed: files_to_annotate.len(),
                    model: model_info,
                },
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
            );
        }
        AnnotationsFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&annotations).unwrap_or_else(|_| "[]".to_string())
            );
        }
    }

    Ok(())
}

/// Compute statistics for a file to help with consolidation decisions
fn compute_file_stats(path: &str, lines: &[BlameLineResult]) -> FileStats {
    let mut ai_lines = 0;
    let mut ai_modified_lines = 0;
    let mut human_lines = 0;
    let mut original_lines = 0;
    let mut prompts: Vec<String> = Vec::new();

    for line in lines {
        match &line.source {
            LineSource::AI { .. } => {
                ai_lines += 1;
                if let Some(ref p) = line.prompt_preview {
                    if !prompts.contains(p) {
                        prompts.push(p.clone());
                    }
                }
            }
            LineSource::AIModified { .. } => {
                ai_modified_lines += 1;
                if let Some(ref p) = line.prompt_preview {
                    if !prompts.contains(p) {
                        prompts.push(p.clone());
                    }
                }
            }
            LineSource::Human => human_lines += 1,
            LineSource::Original => original_lines += 1,
            LineSource::Unknown => {}
        }
    }

    let is_new_file = original_lines == 0 && (ai_lines + ai_modified_lines + human_lines) > 0;

    FileStats {
        path: path.to_string(),
        total_lines: lines.len(),
        ai_lines,
        ai_modified_lines,
        human_lines,
        original_lines,
        is_new_file,
        primary_prompt: prompts.first().cloned(),
        prompt_count: prompts.len(),
    }
}

/// Create a single file-level annotation
fn create_file_annotation(
    stats: &FileStats,
    model: Option<&str>,
    timestamp: Option<&str>,
) -> Option<CheckAnnotation> {
    let ai_total = stats.ai_lines + stats.ai_modified_lines;
    if ai_total == 0 {
        return None;
    }

    let title = if stats.is_new_file {
        format!("New file ({} lines) generated by AI", stats.total_lines)
    } else {
        let pct = (stats.ai_coverage() * 100.0).round() as u32;
        format!(
            "{}% AI-generated ({} of {} lines)",
            pct, ai_total, stats.total_lines
        )
    };

    // Build message
    let mut message_lines = Vec::new();

    // Metadata line
    let mut meta_parts = Vec::new();
    if let Some(m) = model {
        meta_parts.push(format!("Model: {}", m));
    }
    if let Some(ts) = timestamp {
        meta_parts.push(format!("Timestamp: {}", ts));
    }
    if !meta_parts.is_empty() {
        message_lines.push(meta_parts.join(" | "));
    }

    // Stats breakdown
    message_lines.push(String::new());
    message_lines.push(format!(
        "**Breakdown:** {} AI, {} AI-modified, {} human, {} original",
        stats.ai_lines, stats.ai_modified_lines, stats.human_lines, stats.original_lines
    ));

    // Prompt
    if let Some(ref prompt) = stats.primary_prompt {
        message_lines.push(String::new());
        message_lines.push(format!("**Prompt:** {}", truncate_prompt(prompt, 200)));
    }

    Some(CheckAnnotation {
        path: stats.path.clone(),
        start_line: 1,
        end_line: stats.total_lines as u32,
        annotation_level: AnnotationLevel::Notice,
        title,
        message: message_lines.join("\n"),
        raw_details: stats.primary_prompt.clone(),
    })
}

/// Create granular line-level annotations for a file
fn create_line_annotations(
    file_path: &str,
    lines: &[BlameLineResult],
    ai_only: bool,
    min_lines: u32,
    model: Option<&str>,
    timestamp: Option<&str>,
) -> Vec<CheckAnnotation> {
    let groups = group_ai_lines(lines, ai_only);
    let mut annotations = Vec::new();

    for group in groups {
        let line_count = group.end_line - group.start_line + 1;
        if line_count < min_lines {
            continue;
        }

        let title = match group.source_type {
            GroupSourceType::AI => format!(
                "AI Generated ({} line{})",
                line_count,
                if line_count > 1 { "s" } else { "" }
            ),
            GroupSourceType::AIModified => format!(
                "AI Modified ({} line{})",
                line_count,
                if line_count > 1 { "s" } else { "" }
            ),
        };

        // Build message
        let mut message_lines = Vec::new();

        // Metadata
        let mut meta_parts = Vec::new();
        if let Some(m) = model {
            meta_parts.push(format!("Model: {}", m));
        }
        if let Some(ts) = timestamp {
            meta_parts.push(format!("Timestamp: {}", ts));
        }
        if !meta_parts.is_empty() {
            message_lines.push(meta_parts.join(" | "));
        }

        // Prompt
        if let Some(ref prompt) = group.prompt_preview {
            message_lines.push(String::new());
            message_lines.push(format!("**Prompt:** {}", prompt));
        }

        let message = if message_lines.is_empty() {
            match group.source_type {
                GroupSourceType::AI => {
                    "These lines were generated by AI and committed unchanged.".to_string()
                }
                GroupSourceType::AIModified => {
                    "These lines were generated by AI and then modified by a human.".to_string()
                }
            }
        } else {
            message_lines.join("\n")
        };

        annotations.push(CheckAnnotation {
            path: file_path.to_string(),
            start_line: group.start_line,
            end_line: group.end_line,
            annotation_level: AnnotationLevel::Notice,
            title,
            message,
            raw_details: group.prompt_preview.clone(),
        });
    }

    annotations
}

/// Output format for GitHub Checks API
#[derive(Debug, Serialize)]
struct GithubChecksOutput {
    annotations: Vec<CheckAnnotation>,
    summary: GithubChecksSummary,
}

#[derive(Debug, Serialize)]
struct GithubChecksSummary {
    files_analyzed: usize,
    model: Option<String>,
}

/// Grouped annotations for a contiguous range of AI lines
#[derive(Debug)]
struct AnnotationGroup {
    start_line: u32,
    end_line: u32,
    source_type: GroupSourceType,
    prompt_preview: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum GroupSourceType {
    AI,
    AIModified,
}

/// Group consecutive AI lines into annotation ranges
fn group_ai_lines(lines: &[BlameLineResult], ai_only: bool) -> Vec<AnnotationGroup> {
    let mut groups: Vec<AnnotationGroup> = Vec::new();
    let mut current_group: Option<AnnotationGroup> = None;

    for line in lines {
        let source_type = match &line.source {
            LineSource::AI { .. } => Some(GroupSourceType::AI),
            LineSource::AIModified { .. } if !ai_only => Some(GroupSourceType::AIModified),
            _ => None,
        };

        if let Some(stype) = source_type {
            match &mut current_group {
                Some(group)
                    if group.source_type == stype && group.end_line + 1 == line.line_number =>
                {
                    // Extend current group
                    group.end_line = line.line_number;
                    // Update prompt if we don't have one yet
                    if group.prompt_preview.is_none() {
                        group.prompt_preview = line.prompt_preview.clone();
                    }
                }
                _ => {
                    // Start a new group (save current if exists)
                    if let Some(g) = current_group.take() {
                        groups.push(g);
                    }
                    current_group = Some(AnnotationGroup {
                        start_line: line.line_number,
                        end_line: line.line_number,
                        source_type: stype,
                        prompt_preview: line
                            .prompt_preview
                            .clone()
                            .map(|p| truncate_prompt(&p, 200)),
                    });
                }
            }
        } else {
            // Non-AI line - close current group if any
            if let Some(g) = current_group.take() {
                groups.push(g);
            }
        }
    }

    // Don't forget the last group
    if let Some(g) = current_group {
        groups.push(g);
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::snapshot::LineSource;

    fn make_line(line_number: u32, source: LineSource) -> BlameLineResult {
        BlameLineResult {
            line_number,
            content: format!("line {}", line_number),
            commit_id: "abc123".to_string(),
            commit_short: "abc123".to_string(),
            author: "Test".to_string(),
            source,
            prompt_index: Some(0),
            prompt_preview: Some("Test prompt".to_string()),
        }
    }

    #[test]
    fn test_group_consecutive_ai_lines() {
        let lines = vec![
            make_line(
                1,
                LineSource::AI {
                    edit_id: "e1".to_string(),
                },
            ),
            make_line(
                2,
                LineSource::AI {
                    edit_id: "e1".to_string(),
                },
            ),
            make_line(
                3,
                LineSource::AI {
                    edit_id: "e1".to_string(),
                },
            ),
            make_line(4, LineSource::Human),
            make_line(
                5,
                LineSource::AI {
                    edit_id: "e2".to_string(),
                },
            ),
        ];

        let groups = group_ai_lines(&lines, false);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].start_line, 1);
        assert_eq!(groups[0].end_line, 3);
        assert_eq!(groups[1].start_line, 5);
        assert_eq!(groups[1].end_line, 5);
    }

    #[test]
    fn test_group_mixed_ai_and_modified() {
        let lines = vec![
            make_line(
                1,
                LineSource::AI {
                    edit_id: "e1".to_string(),
                },
            ),
            make_line(
                2,
                LineSource::AIModified {
                    edit_id: "e1".to_string(),
                    similarity: 0.8,
                },
            ),
            make_line(
                3,
                LineSource::AI {
                    edit_id: "e1".to_string(),
                },
            ),
        ];

        // With ai_only=false, AI and AIModified are separate groups
        let groups = group_ai_lines(&lines, false);
        assert_eq!(groups.len(), 3);

        // With ai_only=true, AIModified is skipped
        let groups_ai_only = group_ai_lines(&lines, true);
        assert_eq!(groups_ai_only.len(), 2);
    }

    #[test]
    fn test_annotation_serialization() {
        let annotation = CheckAnnotation {
            path: "src/main.rs".to_string(),
            start_line: 1,
            end_line: 5,
            annotation_level: AnnotationLevel::Notice,
            title: "AI Generated (5 lines)".to_string(),
            message: "Model: claude-opus-4-5-20251101".to_string(),
            raw_details: Some("Prompt: Add main function".to_string()),
        };

        let json = serde_json::to_string(&annotation).unwrap();
        assert!(json.contains("\"annotation_level\":\"notice\""));
        assert!(json.contains("\"start_line\":1"));
        assert!(json.contains("\"end_line\":5"));
    }

    #[test]
    fn test_file_stats_new_file() {
        let lines = vec![
            make_line(
                1,
                LineSource::AI {
                    edit_id: "e1".to_string(),
                },
            ),
            make_line(
                2,
                LineSource::AI {
                    edit_id: "e1".to_string(),
                },
            ),
        ];

        let stats = compute_file_stats("test.rs", &lines);
        assert!(stats.is_new_file);
        assert_eq!(stats.ai_lines, 2);
        assert_eq!(stats.original_lines, 0);
        assert!(stats.should_consolidate(0.7));
    }

    #[test]
    fn test_file_stats_mixed_file() {
        let lines = vec![
            make_line(1, LineSource::Original),
            make_line(2, LineSource::Original),
            make_line(3, LineSource::Original),
            make_line(
                4,
                LineSource::AI {
                    edit_id: "e1".to_string(),
                },
            ),
        ];

        let stats = compute_file_stats("test.rs", &lines);
        assert!(!stats.is_new_file);
        assert_eq!(stats.ai_coverage(), 0.25);
        assert!(!stats.should_consolidate(0.7)); // Below threshold
    }

    #[test]
    fn test_file_stats_high_ai_coverage() {
        let lines = vec![
            make_line(1, LineSource::Original),
            make_line(
                2,
                LineSource::AI {
                    edit_id: "e1".to_string(),
                },
            ),
            make_line(
                3,
                LineSource::AI {
                    edit_id: "e1".to_string(),
                },
            ),
            make_line(
                4,
                LineSource::AI {
                    edit_id: "e1".to_string(),
                },
            ),
        ];

        let stats = compute_file_stats("test.rs", &lines);
        assert!(!stats.is_new_file);
        assert_eq!(stats.ai_coverage(), 0.75);
        assert!(stats.should_consolidate(0.7)); // Above threshold, single prompt
    }
}
