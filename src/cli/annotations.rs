//! Annotations command - generate annotations for GitHub Checks API
//!
//! This command outputs annotation data that can be consumed by GitHub's Checks API
//! to display line-level AI attribution directly in the "Files changed" tab of PRs.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use clap::{Args, ValueEnum};
use colored::Colorize;
use git2::Repository;
use serde::Serialize;

use crate::capture::snapshot::LineSource;
use crate::cli::output::MACHINE_OUTPUT_SCHEMA_VERSION;
use crate::core::attribution::BlameLineResult;
use crate::core::blame::AIBlamer;
use crate::storage::notes::NotesStore;
use crate::utils::truncate_prompt;

const ANNOTATIONS_MACHINE_SCHEMA: &str = "whogitit.annotations.v1";

/// Output format for annotations
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum AnnotationsFormat {
    /// GitHub Checks API format
    #[default]
    GithubChecks,
    /// Machine-readable JSON output
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

/// Sort mode for file ordering
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum SortMode {
    /// Sort by AI coverage (highest first)
    #[default]
    Coverage,
    /// Sort by AI line count (highest first)
    Lines,
    /// Sort alphabetically by path
    Alpha,
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

    /// Minimum AI lines for a file to be annotated (filters insignificant files)
    #[arg(long, default_value = "3")]
    pub min_ai_lines: u32,

    /// Minimum AI percentage for a file to be annotated (0.0-100.0)
    #[arg(long, default_value = "5.0")]
    pub min_ai_percent: f64,

    /// Only annotate lines within the PR diff
    #[arg(long)]
    pub diff_only: bool,

    /// Group AI and AIModified together in annotations
    #[arg(long)]
    pub group_ai_types: bool,

    /// Sort files by: coverage (AI %), lines (AI count), alpha (path)
    #[arg(long, value_enum, default_value_t = SortMode::Coverage)]
    pub sort_by: SortMode,

    /// Maximum prompts for auto-consolidation (files with more prompts get granular annotations)
    #[arg(long, default_value = "3")]
    pub consolidate_prompt_limit: usize,
}

/// Summary of a prompt with line count
#[derive(Debug, Clone)]
struct PromptSummary {
    /// Prompt preview text
    preview: String,
    /// Full prompt text for raw_details
    full_text: String,
    /// Number of lines affected by this prompt
    line_count: usize,
}

/// File-level statistics for consolidation decisions
#[derive(Debug)]
struct FileStats {
    path: String,
    total_lines: usize,
    ai_lines: usize,
    ai_modified_lines: usize,
    human_lines: usize,
    original_lines: usize,
    is_new_file: bool,
    /// All prompts with their line counts, sorted by line count descending
    prompts: Vec<PromptSummary>,
}

impl FileStats {
    fn ai_coverage(&self) -> f64 {
        if self.total_lines == 0 {
            0.0
        } else {
            (self.ai_lines + self.ai_modified_lines) as f64 / self.total_lines as f64
        }
    }

    fn ai_total(&self) -> usize {
        self.ai_lines + self.ai_modified_lines
    }

    fn should_consolidate(&self, threshold: f64, prompt_limit: usize) -> bool {
        // Consolidate if:
        // 1. It's a new file (no original lines, all AI), OR
        // 2. High AI coverage AND few prompts (within prompt limit)
        self.is_new_file || (self.ai_coverage() >= threshold && self.prompts.len() <= prompt_limit)
    }
}

/// Annotation candidate with priority scoring
struct AnnotationCandidate {
    annotation: CheckAnnotation,
    score: f64,
}

/// Compute priority score for an annotation
fn compute_annotation_score(stats: &FileStats, is_in_diff: bool) -> f64 {
    let mut score = 0.0;

    // Up to 40 points for AI coverage
    score += stats.ai_coverage() * 40.0;

    // Up to 30 points for AI line count (capped at 100 lines)
    score += (stats.ai_total().min(100) as f64) * 0.3;

    // Bonus for new files (AI created the entire file)
    if stats.is_new_file {
        score += 15.0;
    }

    // Bonus for being in PR diff
    if is_in_diff {
        score += 15.0;
    }

    score
}

/// Check if repository is a shallow clone
fn is_shallow_clone(repo: &Repository) -> bool {
    repo.is_shallow()
}

/// Get changed line ranges from git diff for --diff-only filtering
fn get_diff_ranges(
    repo: &Repository,
    base: &str,
    head: &str,
) -> Result<HashMap<String, Vec<(u32, u32)>>> {
    let base_obj = repo.revparse_single(base)?;
    let head_obj = repo.revparse_single(head)?;

    let base_tree = base_obj.peel_to_tree()?;
    let head_tree = head_obj.peel_to_tree()?;

    let diff = repo.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)?;

    let mut ranges: HashMap<String, Vec<(u32, u32)>> = HashMap::new();

    diff.foreach(
        &mut |_delta, _progress| true,
        None,
        Some(&mut |delta, hunk| {
            if let Some(path) = delta.new_file().path() {
                let path_str = path.to_string_lossy().to_string();
                let start = hunk.new_start();
                let end = start + hunk.new_lines().saturating_sub(1);
                ranges.entry(path_str).or_default().push((start, end));
            }
            true
        }),
        None,
    )?;

    Ok(ranges)
}

/// Check if a line range overlaps with diff ranges
fn overlaps_diff(start: u32, end: u32, diff_ranges: Option<&Vec<(u32, u32)>>) -> bool {
    match diff_ranges {
        None => true, // No diff filtering
        Some(ranges) => ranges.iter().any(|(ds, de)| {
            // Ranges overlap if start <= de and end >= ds
            start <= *de && end >= *ds
        }),
    }
}

/// Format timestamp range for display
fn format_session_range(earliest: Option<&str>, latest: Option<&str>) -> Option<String> {
    match (earliest, latest) {
        (Some(e), Some(l)) if e != l => {
            // Extract just the date portion
            let e_date = e.split('T').next().unwrap_or(e);
            let l_date = l.split('T').next().unwrap_or(l);
            if e_date != l_date {
                Some(format!("{} to {}", e_date, l_date))
            } else {
                Some(e_date.to_string())
            }
        }
        (Some(e), _) => {
            let e_date = e.split('T').next().unwrap_or(e);
            Some(e_date.to_string())
        }
        _ => None,
    }
}

/// Run the annotations command
pub fn run(args: AnnotationsArgs) -> Result<()> {
    let repo = Repository::discover(".").context("Not in a git repository")?;

    // Determine effective consolidation mode for shallow clones
    let is_shallow = is_shallow_clone(&repo);
    let effective_consolidate = if is_shallow {
        eprintln!(
            "{} Shallow clone detected - using file-level annotations only.",
            "Warning:".yellow()
        );
        ConsolidateMode::File
    } else {
        args.consolidate
    };

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

    // Calculate diff ranges if --diff-only is enabled
    let diff_ranges: Option<HashMap<String, Vec<(u32, u32)>>> = if args.diff_only {
        if let Some(base_ref) = &args.base {
            match get_diff_ranges(&repo, base_ref, &args.head) {
                Ok(ranges) => Some(ranges),
                Err(e) => {
                    eprintln!(
                        "{} Could not compute diff ranges: {}. Disabling --diff-only.",
                        "Warning:".yellow(),
                        e
                    );
                    None
                }
            }
        } else {
            eprintln!(
                "{} --diff-only requires --base to be specified. Ignoring --diff-only.",
                "Warning:".yellow()
            );
            None
        }
    } else {
        None
    };

    // Collect all files changed in the commit range with AI attribution
    // Also track all models and timestamps
    let mut files_to_annotate: Vec<String> = Vec::new();
    let mut file_ai_stats: HashMap<String, (usize, f64)> = HashMap::new(); // path -> (ai_lines, ai_percent)
    let mut models_used: HashSet<String> = HashSet::new();
    let mut earliest_timestamp: Option<String> = None;
    let mut latest_timestamp: Option<String> = None;

    for oid_result in revwalk {
        let oid = oid_result?;

        if let Ok(Some(attr)) = notes_store.fetch_attribution(oid) {
            // Track model
            models_used.insert(attr.session.model.id.clone());

            // Track timestamps
            let ts = &attr.session.started_at;
            match &earliest_timestamp {
                None => earliest_timestamp = Some(ts.clone()),
                Some(e) if ts < e => earliest_timestamp = Some(ts.clone()),
                _ => {}
            }
            match &latest_timestamp {
                None => latest_timestamp = Some(ts.clone()),
                Some(l) if ts > l => latest_timestamp = Some(ts.clone()),
                _ => {}
            }

            for file in &attr.files {
                let ai_total = file.summary.ai_lines + file.summary.ai_modified_lines;
                let total = file.summary.total_lines;
                let ai_percent = if total > 0 {
                    (ai_total as f64 / total as f64) * 100.0
                } else {
                    0.0
                };

                // Apply significance filters
                if (ai_total as u32) < args.min_ai_lines {
                    continue;
                }
                if ai_percent < args.min_ai_percent {
                    continue;
                }

                if !files_to_annotate.contains(&file.path) {
                    files_to_annotate.push(file.path.clone());
                    file_ai_stats.insert(file.path.clone(), (ai_total, ai_percent));
                }
            }
        }
    }

    // Sort files by selected criteria
    files_to_annotate.sort_by(|a, b| {
        let (ai_a, pct_a) = file_ai_stats.get(a).copied().unwrap_or((0, 0.0));
        let (ai_b, pct_b) = file_ai_stats.get(b).copied().unwrap_or((0, 0.0));

        match args.sort_by {
            SortMode::Coverage => pct_b
                .partial_cmp(&pct_a)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.cmp(b)),
            SortMode::Lines => ai_b.cmp(&ai_a).then_with(|| a.cmp(b)),
            SortMode::Alpha => a.cmp(b),
        }
    });

    // Prepare model info for annotations
    let models: Vec<String> = models_used.into_iter().collect();
    let session_range =
        format_session_range(earliest_timestamp.as_deref(), latest_timestamp.as_deref());

    // Generate annotations for each file, collecting candidates for prioritization
    let mut candidates: Vec<AnnotationCandidate> = Vec::new();

    for file_path in &files_to_annotate {
        // Check if file is in diff (for scoring and --diff-only filtering)
        let file_diff_ranges = diff_ranges.as_ref().and_then(|dr| dr.get(file_path));
        let is_in_diff = file_diff_ranges.is_some() || diff_ranges.is_none();

        // Run blame on the file at HEAD
        let blame_result = match blamer.blame(file_path, Some(&args.head)) {
            Ok(result) => result,
            Err(_) => continue, // Skip files that can't be blamed (deleted, etc.)
        };

        // Compute file stats for consolidation decision
        let file_stats = compute_file_stats(file_path, &blame_result.lines);

        // Decide whether to consolidate based on mode
        let should_consolidate = match effective_consolidate {
            ConsolidateMode::Auto => file_stats
                .should_consolidate(args.consolidate_threshold, args.consolidate_prompt_limit),
            ConsolidateMode::File => true,
            ConsolidateMode::Lines => false,
        };

        let score = compute_annotation_score(&file_stats, is_in_diff);

        if should_consolidate {
            // Create a single file-level annotation
            if let Some(annotation) =
                create_file_annotation(&file_stats, &models, session_range.as_deref())
            {
                // For file-level, check if the file itself is in diff
                if diff_ranges.is_none() || is_in_diff {
                    candidates.push(AnnotationCandidate { annotation, score });
                }
            }
        } else {
            // Create granular line-level annotations
            let line_annotations = create_line_annotations(
                file_path,
                &blame_result.lines,
                args.ai_only,
                args.group_ai_types,
                args.min_lines,
                &models,
                session_range.as_deref(),
            );

            for annotation in line_annotations {
                // Filter by diff ranges if --diff-only is enabled
                if overlaps_diff(annotation.start_line, annotation.end_line, file_diff_ranges) {
                    candidates.push(AnnotationCandidate { annotation, score });
                }
            }
        }
    }

    // Sort candidates by score descending
    candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));

    // Truncate to max_annotations
    let annotations: Vec<CheckAnnotation> = candidates
        .into_iter()
        .take(args.max_annotations)
        .map(|c| c.annotation)
        .collect();

    let summary = GithubChecksSummary {
        files_analyzed: files_to_annotate.len(),
        models,
        session_range,
    };

    // Output based on format
    match args.format {
        AnnotationsFormat::GithubChecks => {
            let output = GithubChecksOutput {
                annotations,
                summary,
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
            );
        }
        AnnotationsFormat::Json => {
            let output = AnnotationsJsonOutput {
                schema_version: MACHINE_OUTPUT_SCHEMA_VERSION,
                schema: ANNOTATIONS_MACHINE_SCHEMA,
                annotations,
                summary,
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
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

    // Track prompts by index to avoid duplicate counting from truncated text
    let mut prompt_line_counts: HashMap<u32, usize> = HashMap::new();
    let mut prompt_previews: HashMap<u32, String> = HashMap::new();

    for line in lines {
        match &line.source {
            LineSource::AI { .. } => {
                ai_lines += 1;
                if let Some(idx) = line.prompt_index {
                    *prompt_line_counts.entry(idx).or_insert(0) += 1;
                    if let Some(ref preview) = line.prompt_preview {
                        prompt_previews
                            .entry(idx)
                            .or_insert_with(|| preview.clone());
                    }
                }
            }
            LineSource::AIModified { .. } => {
                ai_modified_lines += 1;
                if let Some(idx) = line.prompt_index {
                    *prompt_line_counts.entry(idx).or_insert(0) += 1;
                    if let Some(ref preview) = line.prompt_preview {
                        prompt_previews
                            .entry(idx)
                            .or_insert_with(|| preview.clone());
                    }
                }
            }
            LineSource::Human => human_lines += 1,
            LineSource::Original => original_lines += 1,
            LineSource::Unknown => {}
        }
    }

    let is_new_file = original_lines == 0 && (ai_lines + ai_modified_lines + human_lines) > 0;

    // Build sorted prompt summaries (by line count, descending)
    let mut prompts: Vec<PromptSummary> = prompt_line_counts
        .into_iter()
        .map(|(idx, count)| {
            let preview = prompt_previews.get(&idx).cloned().unwrap_or_default();
            PromptSummary {
                preview: preview.clone(),
                full_text: preview, // Note: We only have the preview here; full text would need blame enhancement
                line_count: count,
            }
        })
        .collect();
    prompts.sort_by(|a, b| b.line_count.cmp(&a.line_count));

    FileStats {
        path: path.to_string(),
        total_lines: lines.len(),
        ai_lines,
        ai_modified_lines,
        human_lines,
        original_lines,
        is_new_file,
        prompts,
    }
}

/// Create a single file-level annotation
fn create_file_annotation(
    stats: &FileStats,
    models: &[String],
    session_range: Option<&str>,
) -> Option<CheckAnnotation> {
    let ai_total = stats.ai_total();
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
    if !models.is_empty() {
        if models.len() == 1 {
            meta_parts.push(format!("Model: {}", models[0]));
        } else {
            meta_parts.push(format!("Models: {}", models.join(", ")));
        }
    }
    if let Some(range) = session_range {
        meta_parts.push(format!("Session: {}", range));
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

    // Prompts - show multiple prompts with line counts
    if stats.prompts.len() == 1 {
        let prompt = &stats.prompts[0];
        message_lines.push(String::new());
        message_lines.push(format!(
            "**Prompt:** {}",
            truncate_prompt(&prompt.preview, 200)
        ));
    } else if stats.prompts.len() > 1 {
        message_lines.push(String::new());
        message_lines.push(format!("**Prompts:** {} prompts used", stats.prompts.len()));
        for (i, prompt) in stats.prompts.iter().take(3).enumerate() {
            message_lines.push(format!(
                "{}. {} ({} lines)",
                i + 1,
                truncate_prompt(&prompt.preview, 100),
                prompt.line_count
            ));
        }
        if stats.prompts.len() > 3 {
            message_lines.push(format!("   ...and {} more", stats.prompts.len() - 3));
        }
    }

    // Build raw_details with all prompt texts
    let raw_details = if stats.prompts.is_empty() {
        None
    } else if stats.prompts.len() == 1 {
        Some(stats.prompts[0].full_text.clone())
    } else {
        Some(
            stats
                .prompts
                .iter()
                .enumerate()
                .map(|(i, p)| format!("Prompt {}: {}", i + 1, p.full_text))
                .collect::<Vec<_>>()
                .join("\n\n"),
        )
    };

    Some(CheckAnnotation {
        path: stats.path.clone(),
        start_line: 1,
        end_line: stats.total_lines as u32,
        annotation_level: AnnotationLevel::Notice,
        title,
        message: message_lines.join("\n"),
        raw_details,
    })
}

/// Create granular line-level annotations for a file
fn create_line_annotations(
    file_path: &str,
    lines: &[BlameLineResult],
    ai_only: bool,
    group_ai_types: bool,
    min_lines: u32,
    models: &[String],
    session_range: Option<&str>,
) -> Vec<CheckAnnotation> {
    let groups = group_ai_lines(lines, ai_only, group_ai_types);
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
            GroupSourceType::AIRelated => {
                // Show breakdown when grouping AI types together
                if group.ai_modified_count > 0 {
                    format!(
                        "AI Related ({} lines: {} AI, {} AI-modified)",
                        line_count, group.ai_count, group.ai_modified_count
                    )
                } else {
                    format!(
                        "AI Generated ({} line{})",
                        line_count,
                        if line_count > 1 { "s" } else { "" }
                    )
                }
            }
        };

        // Build message
        let mut message_lines = Vec::new();

        // Metadata
        let mut meta_parts = Vec::new();
        if !models.is_empty() {
            if models.len() == 1 {
                meta_parts.push(format!("Model: {}", models[0]));
            } else {
                meta_parts.push(format!("Models: {}", models.join(", ")));
            }
        }
        if let Some(range) = session_range {
            meta_parts.push(format!("Session: {}", range));
        }
        if !meta_parts.is_empty() {
            message_lines.push(meta_parts.join(" | "));
        }

        // Prompt
        if let Some(ref prompt) = group.prompt_preview {
            message_lines.push(String::new());
            message_lines.push(format!("**Prompt:** {}", truncate_prompt(prompt, 200)));
        }

        let message = if message_lines.is_empty() {
            match group.source_type {
                GroupSourceType::AI => {
                    "These lines were generated by AI and committed unchanged.".to_string()
                }
                GroupSourceType::AIModified => {
                    "These lines were generated by AI and then modified by a human.".to_string()
                }
                GroupSourceType::AIRelated => {
                    "These lines were generated or modified by AI.".to_string()
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

#[derive(Debug, Clone, Serialize)]
struct GithubChecksSummary {
    files_analyzed: usize,
    /// All models used across the analyzed commits
    models: Vec<String>,
    /// Session time range (e.g., "2024-01-15 to 2024-01-20")
    #[serde(skip_serializing_if = "Option::is_none")]
    session_range: Option<String>,
}

/// Stable machine output for `annotations --format json`.
#[derive(Debug, Serialize)]
struct AnnotationsJsonOutput {
    schema_version: u8,
    schema: &'static str,
    annotations: Vec<CheckAnnotation>,
    summary: GithubChecksSummary,
}

/// Grouped annotations for a contiguous range of AI lines
#[derive(Debug)]
struct AnnotationGroup {
    start_line: u32,
    end_line: u32,
    source_type: GroupSourceType,
    prompt_preview: Option<String>,
    /// When grouping AI types, track individual counts
    ai_count: usize,
    ai_modified_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum GroupSourceType {
    AI,
    AIModified,
    /// Combined AI and AIModified (when --group-ai-types is used)
    AIRelated,
}

/// Group consecutive AI lines into annotation ranges
fn group_ai_lines(
    lines: &[BlameLineResult],
    ai_only: bool,
    group_ai_types: bool,
) -> Vec<AnnotationGroup> {
    let mut groups: Vec<AnnotationGroup> = Vec::new();
    let mut current_group: Option<AnnotationGroup> = None;

    for line in lines {
        // Determine source type and whether it's AI or AIModified
        let (source_type, is_pure_ai) = match &line.source {
            LineSource::AI { .. } => {
                if group_ai_types {
                    (Some(GroupSourceType::AIRelated), true)
                } else {
                    (Some(GroupSourceType::AI), true)
                }
            }
            LineSource::AIModified { .. } if !ai_only => {
                if group_ai_types {
                    (Some(GroupSourceType::AIRelated), false)
                } else {
                    (Some(GroupSourceType::AIModified), false)
                }
            }
            _ => (None, false),
        };

        if let Some(stype) = source_type {
            match &mut current_group {
                Some(group)
                    if group.source_type == stype && group.end_line + 1 == line.line_number =>
                {
                    // Extend current group
                    group.end_line = line.line_number;
                    if is_pure_ai {
                        group.ai_count += 1;
                    } else {
                        group.ai_modified_count += 1;
                    }
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
                        prompt_preview: line.prompt_preview.clone(),
                        ai_count: if is_pure_ai { 1 } else { 0 },
                        ai_modified_count: if is_pure_ai { 0 } else { 1 },
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

        let groups = group_ai_lines(&lines, false, false);

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

        // With ai_only=false, group_ai_types=false: AI and AIModified are separate groups
        let groups = group_ai_lines(&lines, false, false);
        assert_eq!(groups.len(), 3);

        // With ai_only=true: AIModified is skipped
        let groups_ai_only = group_ai_lines(&lines, true, false);
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
        assert!(stats.should_consolidate(0.7, 3)); // New files always consolidate
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
        assert!(!stats.should_consolidate(0.7, 3)); // Below threshold
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
        assert!(stats.should_consolidate(0.7, 3)); // Above threshold, single prompt
    }

    #[test]
    fn test_group_ai_types_combined() {
        // When group_ai_types=true, AI and AIModified should merge into one group
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

        // Without grouping: 3 separate groups
        let groups_ungrouped = group_ai_lines(&lines, false, false);
        assert_eq!(groups_ungrouped.len(), 3);

        // With grouping: 1 combined group
        let groups_grouped = group_ai_lines(&lines, false, true);
        assert_eq!(groups_grouped.len(), 1);
        assert_eq!(groups_grouped[0].source_type, GroupSourceType::AIRelated);
        assert_eq!(groups_grouped[0].ai_count, 2);
        assert_eq!(groups_grouped[0].ai_modified_count, 1);
    }

    #[test]
    fn test_annotation_scoring() {
        // High AI coverage file
        let high_coverage = FileStats {
            path: "high.rs".to_string(),
            total_lines: 100,
            ai_lines: 90,
            ai_modified_lines: 5,
            human_lines: 3,
            original_lines: 2,
            is_new_file: false,
            prompts: vec![],
        };

        // Low AI coverage file
        let low_coverage = FileStats {
            path: "low.rs".to_string(),
            total_lines: 100,
            ai_lines: 10,
            ai_modified_lines: 0,
            human_lines: 50,
            original_lines: 40,
            is_new_file: false,
            prompts: vec![],
        };

        let high_score = compute_annotation_score(&high_coverage, true);
        let low_score = compute_annotation_score(&low_coverage, false);

        // High coverage + in diff should score higher than low coverage + not in diff
        assert!(high_score > low_score);
    }

    #[test]
    fn test_new_file_bonus_in_scoring() {
        let new_file = FileStats {
            path: "new.rs".to_string(),
            total_lines: 50,
            ai_lines: 50,
            ai_modified_lines: 0,
            human_lines: 0,
            original_lines: 0,
            is_new_file: true,
            prompts: vec![],
        };

        let existing_file = FileStats {
            path: "existing.rs".to_string(),
            total_lines: 50,
            ai_lines: 50,
            ai_modified_lines: 0,
            human_lines: 0,
            original_lines: 0,
            is_new_file: false,
            prompts: vec![],
        };

        let new_score = compute_annotation_score(&new_file, false);
        let existing_score = compute_annotation_score(&existing_file, false);

        // New file should get bonus points
        assert!(new_score > existing_score);
    }

    #[test]
    fn test_diff_overlap() {
        // Test that overlaps_diff works correctly
        let ranges = vec![(10, 20), (30, 40)];

        // Line range fully within a diff range
        assert!(overlaps_diff(12, 18, Some(&ranges)));

        // Line range overlapping start of diff
        assert!(overlaps_diff(5, 15, Some(&ranges)));

        // Line range overlapping end of diff
        assert!(overlaps_diff(18, 25, Some(&ranges)));

        // Line range outside all diff ranges
        assert!(!overlaps_diff(22, 28, Some(&ranges)));

        // No diff ranges means everything overlaps
        assert!(overlaps_diff(100, 200, None));
    }

    #[test]
    fn test_consolidation_with_prompt_limit() {
        // Create file stats with multiple prompts
        let multi_prompt = FileStats {
            path: "multi.rs".to_string(),
            total_lines: 100,
            ai_lines: 80,
            ai_modified_lines: 10,
            human_lines: 5,
            original_lines: 5,
            is_new_file: false,
            prompts: vec![
                PromptSummary {
                    preview: "Prompt 1".to_string(),
                    full_text: "Prompt 1".to_string(),
                    line_count: 40,
                },
                PromptSummary {
                    preview: "Prompt 2".to_string(),
                    full_text: "Prompt 2".to_string(),
                    line_count: 30,
                },
                PromptSummary {
                    preview: "Prompt 3".to_string(),
                    full_text: "Prompt 3".to_string(),
                    line_count: 10,
                },
                PromptSummary {
                    preview: "Prompt 4".to_string(),
                    full_text: "Prompt 4".to_string(),
                    line_count: 10,
                },
            ],
        };

        // Should consolidate with prompt_limit >= 4
        assert!(multi_prompt.should_consolidate(0.7, 4));
        assert!(multi_prompt.should_consolidate(0.7, 5));

        // Should NOT consolidate with prompt_limit < 4
        assert!(!multi_prompt.should_consolidate(0.7, 3));
        assert!(!multi_prompt.should_consolidate(0.7, 1));
    }

    #[test]
    fn test_session_range_formatting() {
        // Same date
        assert_eq!(
            format_session_range(Some("2024-01-15T10:00:00Z"), Some("2024-01-15T18:00:00Z")),
            Some("2024-01-15".to_string())
        );

        // Different dates
        assert_eq!(
            format_session_range(Some("2024-01-15T10:00:00Z"), Some("2024-01-20T18:00:00Z")),
            Some("2024-01-15 to 2024-01-20".to_string())
        );

        // Only earliest
        assert_eq!(
            format_session_range(Some("2024-01-15T10:00:00Z"), None),
            Some("2024-01-15".to_string())
        );

        // Both same
        let ts = "2024-01-15T10:00:00Z";
        assert_eq!(
            format_session_range(Some(ts), Some(ts)),
            Some("2024-01-15".to_string())
        );
    }

    #[test]
    fn test_github_checks_summary_serialization() {
        let summary = GithubChecksSummary {
            files_analyzed: 5,
            models: vec![
                "claude-sonnet-4-20250514".to_string(),
                "claude-opus-4-5-20251101".to_string(),
            ],
            session_range: Some("2024-01-15 to 2024-01-20".to_string()),
        };

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("\"files_analyzed\":5"));
        assert!(json.contains("\"models\":["));
        assert!(json.contains("claude-sonnet-4-20250514"));
        assert!(json.contains("\"session_range\":"));
    }

    #[test]
    fn test_github_checks_summary_no_session_range() {
        let summary = GithubChecksSummary {
            files_analyzed: 3,
            models: vec!["claude-sonnet-4-20250514".to_string()],
            session_range: None,
        };

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("\"files_analyzed\":3"));
        // session_range should be omitted when None
        assert!(!json.contains("session_range"));
    }

    #[test]
    fn test_annotations_json_output_has_schema_metadata() {
        let output = AnnotationsJsonOutput {
            schema_version: MACHINE_OUTPUT_SCHEMA_VERSION,
            schema: ANNOTATIONS_MACHINE_SCHEMA,
            annotations: vec![CheckAnnotation {
                path: "src/main.rs".to_string(),
                start_line: 1,
                end_line: 1,
                annotation_level: AnnotationLevel::Notice,
                title: "AI Generated (1 line)".to_string(),
                message: "Model: claude-opus-4-5-20251101".to_string(),
                raw_details: None,
            }],
            summary: GithubChecksSummary {
                files_analyzed: 1,
                models: vec!["claude-opus-4-5-20251101".to_string()],
                session_range: Some("2024-01-15".to_string()),
            },
        };

        let json = serde_json::to_value(&output).unwrap();
        assert_eq!(
            json["schema_version"],
            serde_json::Value::from(MACHINE_OUTPUT_SCHEMA_VERSION)
        );
        assert_eq!(json["schema"], ANNOTATIONS_MACHINE_SCHEMA);
        assert!(json["annotations"].is_array());
        assert!(json["summary"].is_object());
    }
}
