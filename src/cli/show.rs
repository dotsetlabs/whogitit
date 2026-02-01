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

#[cfg(test)]
mod tests {
    use super::*;

    // ShowArgs tests

    #[test]
    fn test_show_args_default_commit() {
        let args = ShowArgs {
            commit: "HEAD".to_string(),
            format: OutputFormat::Pretty,
        };
        assert_eq!(args.commit, "HEAD");
        assert!(matches!(args.format, OutputFormat::Pretty));
    }

    #[test]
    fn test_show_args_with_sha() {
        let args = ShowArgs {
            commit: "abc1234".to_string(),
            format: OutputFormat::Json,
        };
        assert_eq!(args.commit, "abc1234");
        assert!(matches!(args.format, OutputFormat::Json));
    }

    #[test]
    fn test_show_args_with_branch() {
        let args = ShowArgs {
            commit: "main".to_string(),
            format: OutputFormat::Pretty,
        };
        assert_eq!(args.commit, "main");
    }

    #[test]
    fn test_show_args_with_parent_ref() {
        let args = ShowArgs {
            commit: "HEAD~3".to_string(),
            format: OutputFormat::Pretty,
        };
        assert_eq!(args.commit, "HEAD~3");
    }

    // Line counting aggregation test (simulated)
    #[test]
    fn test_line_count_aggregation() {
        // Simulate the aggregation logic from print_summary
        let file_stats = vec![
            (10, 2, 5, 100), // (ai, ai_modified, human, original)
            (20, 5, 10, 200),
            (5, 1, 2, 50),
        ];

        let mut total_ai = 0usize;
        let mut total_ai_modified = 0usize;
        let mut total_human = 0usize;
        let mut total_original = 0usize;

        for (ai, ai_mod, human, original) in &file_stats {
            total_ai += ai;
            total_ai_modified += ai_mod;
            total_human += human;
            total_original += original;
        }

        assert_eq!(total_ai, 35);
        assert_eq!(total_ai_modified, 8);
        assert_eq!(total_human, 17);
        assert_eq!(total_original, 350);
    }

    // Commit short substring test
    #[test]
    fn test_commit_short_extraction() {
        let commit_id = "abc1234def456789";
        let commit_short = &commit_id[..commit_id.len().min(SHORT_COMMIT_LEN)];
        assert_eq!(commit_short, "abc1234");
        assert_eq!(commit_short.len(), 7);
    }

    #[test]
    fn test_commit_short_extraction_short_id() {
        // Edge case: commit ID shorter than SHORT_COMMIT_LEN
        let commit_id = "abc12";
        let commit_short = &commit_id[..commit_id.len().min(SHORT_COMMIT_LEN)];
        assert_eq!(commit_short, "abc12");
    }
}
