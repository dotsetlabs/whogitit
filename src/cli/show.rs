use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;
use git2::Repository;

use crate::cli::output::OutputFormat;
use crate::storage::notes::NotesStore;

/// Show command arguments
#[derive(Debug, Args)]
pub struct ShowArgs {
    /// Commit to show (default: HEAD)
    #[arg(default_value = "HEAD")]
    pub commit: String,

    /// Output format (pretty or json)
    #[arg(long, default_value = "pretty")]
    pub format: String,
}

/// Run the show command
pub fn run(args: ShowArgs) -> Result<()> {
    // Open repository
    let repo = Repository::discover(".").context("Not in a git repository")?;

    // Resolve commit reference
    let obj = repo
        .revparse_single(&args.commit)
        .with_context(|| format!("Failed to resolve: {}", args.commit))?;
    let commit = obj
        .peel_to_commit()
        .with_context(|| format!("Not a valid commit: {}", args.commit))?;

    let commit_id = commit.id().to_string();
    let commit_short = &commit_id[..7];

    // Get attribution
    let notes_store = NotesStore::new(&repo)?;
    let attribution = notes_store.fetch_attribution(commit.id())?;

    let format = match args.format.to_lowercase().as_str() {
        "json" => OutputFormat::Json,
        _ => OutputFormat::Pretty,
    };

    match attribution {
        Some(attr) => {
            if format == OutputFormat::Json {
                println!("{}", serde_json::to_string_pretty(&attr)?);
            } else {
                print_summary(commit_short, &attr);
            }
        }
        None => {
            println!("No AI attribution found for commit {}", commit_short);
            println!("This commit was not made with AI assistance tracked by ai-blame.");
        }
    }

    Ok(())
}

fn print_summary(commit_short: &str, attr: &crate::core::attribution::AIAttribution) {
    println!("{}: {}", "Commit".bold(), commit_short.yellow());
    println!("{}: {}", "Session".bold(), attr.session.session_id.cyan());
    println!("{}: {}", "Model".bold(), attr.session.model.id);
    println!("{}: {}", "Started".bold(), attr.session.started_at.dimmed());
    println!();

    // Show prompts
    if !attr.prompts.is_empty() {
        println!("{}", "Prompts used:".bold());
        for prompt in &attr.prompts {
            let preview = if prompt.text.len() > 60 {
                format!("{}...", &prompt.text[..57])
            } else {
                prompt.text.clone()
            };
            println!("  #{}: \"{}\"", prompt.index, preview.dimmed());
        }
        println!();
    }

    // Show files with detailed breakdown
    println!("{}", "Files with AI changes:".bold());

    let mut total_ai = 0usize;
    let mut total_ai_modified = 0usize;
    let mut total_human = 0usize;
    let mut total_original = 0usize;

    for file in &attr.files {
        let s = &file.summary;
        total_ai += s.ai_lines;
        total_ai_modified += s.ai_modified_lines;
        total_human += s.human_lines;
        total_original += s.original_lines;

        // Color-coded breakdown
        let ai_str = format!("{} AI", s.ai_lines).green();
        let modified_str = if s.ai_modified_lines > 0 {
            format!(", {} modified", s.ai_modified_lines)
                .yellow()
                .to_string()
        } else {
            String::new()
        };
        let human_str = if s.human_lines > 0 {
            format!(", {} human", s.human_lines).blue().to_string()
        } else {
            String::new()
        };
        let original_str = if s.original_lines > 0 {
            format!(", {} original", s.original_lines)
                .dimmed()
                .to_string()
        } else {
            String::new()
        };

        println!(
            "  {} ({}{}{}{}) - {} total lines",
            file.path, ai_str, modified_str, human_str, original_str, s.total_lines
        );
    }

    println!();
    println!("{}", "Summary:".bold());
    println!("  {} AI-generated lines", total_ai.to_string().green());
    if total_ai_modified > 0 {
        println!(
            "  {} AI lines modified by human",
            total_ai_modified.to_string().yellow()
        );
    }
    if total_human > 0 {
        println!("  {} human-added lines", total_human.to_string().blue());
    }
    if total_original > 0 {
        println!(
            "  {} original/unchanged lines",
            total_original.to_string().dimmed()
        );
    }
}
