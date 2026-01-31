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

/// Aggregated summary across multiple commits
#[derive(Debug, Default)]
struct AggregateSummary {
    commits_analyzed: usize,
    commits_with_ai: usize,
    total_ai_lines: usize,
    total_ai_modified_lines: usize,
    total_human_lines: usize,
    total_original_lines: usize,
    files_touched: Vec<String>,
    models_used: Vec<String>,
}

impl AggregateSummary {
    /// Total lines including unchanged (for showing full breakdown)
    fn total_lines(&self) -> usize {
        self.total_ai_lines
            + self.total_ai_modified_lines
            + self.total_human_lines
            + self.total_original_lines
    }

    /// Only lines that were actually changed (not original/unchanged)
    fn changed_lines(&self) -> usize {
        self.total_ai_lines + self.total_ai_modified_lines + self.total_human_lines
    }

    /// AI involvement as percentage of CHANGED lines (not including original)
    fn ai_percentage(&self) -> f64 {
        let changed = self.changed_lines();
        if changed == 0 {
            0.0
        } else {
            ((self.total_ai_lines + self.total_ai_modified_lines) as f64 / changed as f64) * 100.0
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

                if !summary.files_touched.contains(&file.path) {
                    summary.files_touched.push(file.path.clone());
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

    println!("{}", "Line Attribution:".bold());
    println!(
        "  {} AI-generated lines",
        summary.total_ai_lines.to_string().green()
    );
    println!(
        "  {} AI lines modified by human",
        summary.total_ai_modified_lines.to_string().yellow()
    );
    println!(
        "  {} human-added lines",
        summary.total_human_lines.to_string().blue()
    );
    println!(
        "  {} original/unchanged lines",
        summary.total_original_lines.to_string().dimmed()
    );
    println!();

    println!(
        "{}: {:.1}%",
        "AI involvement".bold(),
        summary.ai_percentage()
    );
    println!();

    println!("{}", "Files with AI changes:".bold());
    for file in &summary.files_touched {
        println!("  - {}", file);
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
    let output = serde_json::json!({
        "commits_analyzed": summary.commits_analyzed,
        "commits_with_ai": summary.commits_with_ai,
        "lines": {
            "ai": summary.total_ai_lines,
            "ai_modified": summary.total_ai_modified_lines,
            "human": summary.total_human_lines,
            "original": summary.total_original_lines,
            "total": summary.total_lines(),
        },
        "ai_percentage": summary.ai_percentage(),
        "files": summary.files_touched,
        "models": summary.models_used,
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
    );
}

fn print_markdown(summary: &AggregateSummary) {
    let total = summary.total_lines();
    let ai_pct = if total > 0 {
        (summary.total_ai_lines as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    let mod_pct = if total > 0 {
        (summary.total_ai_modified_lines as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    let human_pct = if total > 0 {
        (summary.total_human_lines as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    let orig_pct = if total > 0 {
        (summary.total_original_lines as f64 / total as f64) * 100.0
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
        "**{}** of **{}** commits contain AI-assisted changes.",
        summary.commits_with_ai, summary.commits_analyzed
    );
    println!();
    println!("### Overview");
    println!();
    println!("| Metric | Lines | Percentage |");
    println!("|--------|------:|----------:|");
    println!(
        "| 🟢 AI-generated | {} | {:.1}% |",
        summary.total_ai_lines, ai_pct
    );
    println!(
        "| 🟡 AI-modified by human | {} | {:.1}% |",
        summary.total_ai_modified_lines, mod_pct
    );
    println!(
        "| 🔵 Human-added | {} | {:.1}% |",
        summary.total_human_lines, human_pct
    );
    println!(
        "| ⚪ Original/unchanged | {} | {:.1}% |",
        summary.total_original_lines, orig_pct
    );
    println!("| **Total** | **{}** | **100%** |", total);
    println!();
    println!(
        "**AI involvement: {:.1}%** of changed lines",
        summary.ai_percentage()
    );
    println!();

    if !summary.files_touched.is_empty() {
        println!("### Files with AI Changes");
        println!();
        for file in &summary.files_touched {
            println!("- `{}`", file);
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
