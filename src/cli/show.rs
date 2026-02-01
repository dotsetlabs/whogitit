use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;
use git2::Repository;

use crate::cli::output::OutputFormat;
use crate::storage::notes::NotesStore;
use crate::utils::{truncate, SHORT_COMMIT_LEN};

/// Show command arguments
#[derive(Debug, Args)]
pub struct ShowArgs {
    /// Commit to show (default: HEAD)
    #[arg(default_value = "HEAD")]
    pub commit: String,

    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Pretty)]
    pub format: OutputFormat,
}

/// Run the show command
pub fn run(args: ShowArgs) -> Result<()> {
    // Open repository
    let repo = Repository::discover(".").context(
        "Not in a git repository. \
         Run 'git init' to create one, or 'cd' to a directory containing a .git folder.",
    )?;

    // Resolve commit reference
    let obj = repo.revparse_single(&args.commit).with_context(|| {
        format!(
            "Failed to resolve '{}'. \n\
                 Suggestions:\n  \
                 - Use a valid commit SHA: whogitit show abc1234\n  \
                 - Use HEAD for latest: whogitit show HEAD\n  \
                 - Use a branch name: whogitit show main\n  \
                 - Use HEAD~N for parent: whogitit show HEAD~1",
            args.commit
        )
    })?;
    let commit = obj
        .peel_to_commit()
        .with_context(|| format!("'{}' is not a valid commit reference", args.commit))?;

    let commit_id = commit.id().to_string();
    // Safe substring: commit IDs are hex strings (ASCII), but we still use min() for safety
    let commit_short = &commit_id[..commit_id.len().min(SHORT_COMMIT_LEN)];

    // Get attribution
    let notes_store = NotesStore::new(&repo)?;
    let attribution = notes_store.fetch_attribution(commit.id())?;

    match attribution {
        Some(attr) => {
            if args.format == OutputFormat::Json {
                println!("{}", serde_json::to_string_pretty(&attr)?);
            } else {
                print_summary(commit_short, &attr);
            }
        }
        None => {
            if args.format == OutputFormat::Json {
                // Output null JSON for programmatic consumption
                println!("null");
            } else {
                println!("No AI attribution found for commit {}", commit_short);
                println!("This commit was not made with AI assistance tracked by whogitit.");
            }
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
            let preview = truncate(&prompt.text, 60);
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
