use anyhow::{Context, Result};
use clap::{Args, ValueEnum};
use colored::Colorize;
use git2::Repository;

use crate::storage::notes::NotesStore;

/// Check if repository is a shallow clone
fn is_shallow_clone(repo: &Repository) -> bool {
    repo.is_shallow()
}

/// Print shallow clone warning
fn print_shallow_warning() {
    eprintln!(
        "{} Running in shallow clone mode - historical attribution data may be incomplete.",
        "Warning:".yellow()
    );
    eprintln!(
        "         Run '{}' to get full history.",
        "git fetch --unshallow".cyan()
    );
    eprintln!();
}

/// Output format for summary command
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum SummaryFormat {
    /// Human-readable terminal output with colors
    #[default]
    Pretty,
    /// JSON output for machine consumption
    Json,
    /// Markdown output for documentation/PRs
    #[value(alias = "md")]
    Markdown,
}

/// Summary command arguments
#[derive(Debug, Args)]
pub struct SummaryArgs {
    /// Base commit (exclusive) - defaults to first commit if not specified
    #[arg(long)]
    pub base: Option<String>,

    /// Head commit (inclusive) - defaults to HEAD
    #[arg(long, default_value = "HEAD")]
    pub head: String,

    /// Output format
    #[arg(long, value_enum, default_value_t = SummaryFormat::Pretty)]
    pub format: SummaryFormat,
}

/// Per-file summary for diff-focused display
#[derive(Debug, Clone)]
struct FileSummary {
    path: String,
    ai_lines: usize,
    ai_modified_lines: usize,
    human_lines: usize,
    original_lines: usize,
    is_new_file: bool,
}

impl FileSummary {
    /// Lines added in this file (AI + AI-modified + Human)
    fn additions(&self) -> usize {
        self.ai_lines + self.ai_modified_lines + self.human_lines
    }

    /// AI additions (AI + AI-modified)
    fn ai_additions(&self) -> usize {
        self.ai_lines + self.ai_modified_lines
    }

    /// Percentage of additions that are AI-generated
    fn ai_percent(&self) -> f64 {
        let adds = self.additions();
        if adds == 0 {
            0.0
        } else {
            (self.ai_additions() as f64 / adds as f64) * 100.0
        }
    }
}

/// Aggregated summary across multiple commits (diff-focused)
#[derive(Debug, Default)]
struct AggregateSummary {
    commits_analyzed: usize,
    commits_with_ai: usize,
    /// AI-generated lines (additions)
    total_ai_lines: usize,
    /// AI lines modified by human (additions)
    total_ai_modified_lines: usize,
    /// Human-written lines (additions)
    total_human_lines: usize,
    /// Original/unchanged lines (NOT additions - for context only)
    total_original_lines: usize,
    /// Per-file summaries for detailed breakdown
    file_summaries: Vec<FileSummary>,
    models_used: Vec<String>,
}

impl AggregateSummary {
    /// Total additions (lines added in the diff)
    fn total_additions(&self) -> usize {
        self.total_ai_lines + self.total_ai_modified_lines + self.total_human_lines
    }

    /// AI additions (AI + AI-modified)
    fn ai_additions(&self) -> usize {
        self.total_ai_lines + self.total_ai_modified_lines
    }

    /// AI involvement as percentage of additions
    fn ai_percentage(&self) -> f64 {
        let additions = self.total_additions();
        if additions == 0 {
            0.0
        } else {
            (self.ai_additions() as f64 / additions as f64) * 100.0
        }
    }
}

/// Run the summary command
pub fn run(args: SummaryArgs) -> Result<()> {
    let repo = Repository::discover(".").context("Not in a git repository")?;

    // Check for shallow clone
    if is_shallow_clone(&repo) && matches!(args.format, SummaryFormat::Pretty) {
        print_shallow_warning();
    }

    let notes_store = NotesStore::new(&repo)?;

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

    // Analyze commits
    let mut summary = AggregateSummary::default();

    for oid_result in revwalk {
        let oid = oid_result?;
        summary.commits_analyzed += 1;

        if let Ok(Some(attr)) = notes_store.fetch_attribution(oid) {
            summary.commits_with_ai += 1;

            // Aggregate file statistics
            for file in &attr.files {
                summary.total_ai_lines += file.summary.ai_lines;
                summary.total_ai_modified_lines += file.summary.ai_modified_lines;
                summary.total_human_lines += file.summary.human_lines;
                summary.total_original_lines += file.summary.original_lines;

                // Check if file already exists in summaries
                let existing = summary
                    .file_summaries
                    .iter_mut()
                    .find(|f| f.path == file.path);

                if let Some(existing) = existing {
                    // Aggregate with existing
                    existing.ai_lines += file.summary.ai_lines;
                    existing.ai_modified_lines += file.summary.ai_modified_lines;
                    existing.human_lines += file.summary.human_lines;
                    existing.original_lines += file.summary.original_lines;
                } else {
                    // Add new file summary
                    let is_new = file.summary.original_lines == 0
                        && (file.summary.ai_lines > 0
                            || file.summary.ai_modified_lines > 0
                            || file.summary.human_lines > 0);

                    summary.file_summaries.push(FileSummary {
                        path: file.path.clone(),
                        ai_lines: file.summary.ai_lines,
                        ai_modified_lines: file.summary.ai_modified_lines,
                        human_lines: file.summary.human_lines,
                        original_lines: file.summary.original_lines,
                        is_new_file: is_new,
                    });
                }
            }

            // Track models used
            if !summary.models_used.contains(&attr.session.model.id) {
                summary.models_used.push(attr.session.model.id.clone());
            }
        }
    }

    // Output based on format
    match args.format {
        SummaryFormat::Pretty => print_pretty(&summary),
        SummaryFormat::Json => print_json(&summary),
        SummaryFormat::Markdown => print_markdown(&summary),
    }

    Ok(())
}

fn print_pretty(summary: &AggregateSummary) {
    println!();
    println!("{}", "═".repeat(60).dimmed());
    println!("{}", "  AI Attribution Summary".bold());
    println!("{}", "═".repeat(60).dimmed());
    println!();

    println!(
        "Commits analyzed: {} ({} with AI attribution)",
        summary.commits_analyzed.to_string().cyan(),
        summary.commits_with_ai.to_string().green()
    );
    println!();

    if summary.commits_with_ai == 0 {
        println!("No AI attribution data found in the specified commit range.");
        return;
    }

    let total_additions = summary.total_additions();

    println!("{}", "Lines Added:".bold());
    println!(
        "  {} AI-generated ({:.1}%)",
        format!("+{}", summary.total_ai_lines).green(),
        if total_additions > 0 {
            (summary.total_ai_lines as f64 / total_additions as f64) * 100.0
        } else {
            0.0
        }
    );
    println!(
        "  {} AI-modified by human ({:.1}%)",
        format!("+{}", summary.total_ai_modified_lines).yellow(),
        if total_additions > 0 {
            (summary.total_ai_modified_lines as f64 / total_additions as f64) * 100.0
        } else {
            0.0
        }
    );
    println!(
        "  {} Human-written ({:.1}%)",
        format!("+{}", summary.total_human_lines).blue(),
        if total_additions > 0 {
            (summary.total_human_lines as f64 / total_additions as f64) * 100.0
        } else {
            0.0
        }
    );
    println!(
        "  {} Total additions",
        format!("+{}", total_additions).bold()
    );
    println!();

    println!(
        "{}: {:.1}% of additions are AI-generated",
        "AI involvement".bold(),
        summary.ai_percentage()
    );
    println!();

    println!("{}", "Files Changed:".bold());
    for file in &summary.file_summaries {
        let status = if file.is_new_file { " (new)" } else { "" };
        let ai_pct = file.ai_percent();
        println!(
            "  {} +{} ({:.0}% AI){}",
            file.path,
            file.additions(),
            ai_pct,
            status
        );
    }
    println!();

    if !summary.models_used.is_empty() {
        println!("{}", "Models used:".bold());
        for model in &summary.models_used {
            println!("  - {}", model.cyan());
        }
    }

    println!();
    println!("{}", "═".repeat(60).dimmed());
}

fn print_json(summary: &AggregateSummary) {
    let files_json: Vec<_> = summary
        .file_summaries
        .iter()
        .map(|f| {
            serde_json::json!({
                "path": f.path,
                "additions": f.additions(),
                "ai_additions": f.ai_additions(),
                "ai_lines": f.ai_lines,
                "ai_modified_lines": f.ai_modified_lines,
                "human_lines": f.human_lines,
                "ai_percent": f.ai_percent(),
                "is_new_file": f.is_new_file,
            })
        })
        .collect();

    let output = serde_json::json!({
        "commits_analyzed": summary.commits_analyzed,
        "commits_with_ai": summary.commits_with_ai,
        "additions": {
            "total": summary.total_additions(),
            "ai": summary.total_ai_lines,
            "ai_modified": summary.total_ai_modified_lines,
            "human": summary.total_human_lines,
        },
        "ai_percentage": summary.ai_percentage(),
        "files": files_json,
        "models": summary.models_used,
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
    );
}

fn print_markdown(summary: &AggregateSummary) {
    let total_additions = summary.total_additions();
    let ai_pct = if total_additions > 0 {
        (summary.total_ai_lines as f64 / total_additions as f64) * 100.0
    } else {
        0.0
    };
    let mod_pct = if total_additions > 0 {
        (summary.total_ai_modified_lines as f64 / total_additions as f64) * 100.0
    } else {
        0.0
    };
    let human_pct = if total_additions > 0 {
        (summary.total_human_lines as f64 / total_additions as f64) * 100.0
    } else {
        0.0
    };

    let emoji = if summary.ai_percentage() >= 80.0 {
        "🤖🤖🤖"
    } else if summary.ai_percentage() >= 50.0 {
        "🤖🤖"
    } else if summary.ai_percentage() >= 20.0 {
        "🤖"
    } else {
        "👤"
    };

    println!("## {} AI Attribution Summary", emoji);
    println!();
    println!(
        "This PR adds **+{}** lines with AI attribution across **{}** files.",
        total_additions,
        summary.file_summaries.len()
    );
    println!();
    println!("### Additions Breakdown");
    println!();
    println!("| Metric | Lines | % of Additions |");
    println!("|--------|------:|--------------:|");
    println!(
        "| 🟢 AI-generated | +{} | {:.1}% |",
        summary.total_ai_lines, ai_pct
    );
    println!(
        "| 🟡 AI-modified by human | +{} | {:.1}% |",
        summary.total_ai_modified_lines, mod_pct
    );
    println!(
        "| 🔵 Human-written | +{} | {:.1}% |",
        summary.total_human_lines, human_pct
    );
    println!(
        "| **Total additions** | **+{}** | **100%** |",
        total_additions
    );
    println!();
    println!(
        "**AI involvement: {:.1}%** of additions are AI-generated",
        summary.ai_percentage()
    );
    println!();

    if !summary.file_summaries.is_empty() {
        println!("### Files Changed");
        println!();
        println!("| File | +Added | AI | Human | AI % | Status |");
        println!("|------|-------:|---:|------:|-----:|--------|");
        for file in &summary.file_summaries {
            let status = if file.is_new_file { "New" } else { "Modified" };
            println!(
                "| `{}` | +{} | {} | {} | {:.0}% | {} |",
                file.path,
                file.additions(),
                file.ai_additions(),
                file.human_lines,
                file.ai_percent(),
                status
            );
        }
        println!();
    }

    if !summary.models_used.is_empty() {
        println!("### Models Used");
        println!();
        for model in &summary.models_used {
            println!("- {}", model);
        }
    }
}
