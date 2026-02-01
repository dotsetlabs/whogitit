use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;
use git2::Repository;

use crate::cli::output::{format_blame, OutputFormat};
use crate::core::blame::AIBlamer;

/// Blame command arguments
#[derive(Debug, Args)]
pub struct BlameArgs {
    /// File to blame
    pub file: String,

    /// Revision to blame against (default: HEAD)
    #[arg(short, long)]
    pub revision: Option<String>,

    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Pretty)]
    pub format: OutputFormat,

    /// Show only AI-generated lines
    #[arg(long)]
    pub ai_only: bool,

    /// Show only human-written lines
    #[arg(long)]
    pub human_only: bool,
}

/// Check if repository is a shallow clone
fn is_shallow_clone(repo: &Repository) -> bool {
    repo.is_shallow()
}

/// Print shallow clone warning
fn print_shallow_warning() {
    eprintln!(
        "{} Running in shallow clone mode - some attribution data may be unavailable.",
        "Warning:".yellow()
    );
    eprintln!(
        "         Run '{}' to get full history.",
        "git fetch --unshallow".cyan()
    );
    eprintln!();
}

/// Run the blame command
pub fn run(args: BlameArgs) -> Result<()> {
    // Open repository
    let repo = Repository::discover(".").context(
        "Not in a git repository. \
         Run 'git init' to create one, or 'cd' to a directory containing a .git folder.",
    )?;

    // Check for shallow clone - warn in all formats for consistency
    if is_shallow_clone(&repo) {
        match args.format {
            OutputFormat::Pretty => print_shallow_warning(),
            OutputFormat::Json => {
                // For programmatic output, still warn to stderr
                eprintln!(
                    "Warning: Shallow clone detected - attribution data may be incomplete. \
                     Run 'git fetch --unshallow' for full history."
                );
            }
        }
    }

    // Create blamer
    let mut blamer = AIBlamer::new(&repo).context(
        "Failed to initialize blame engine. \
         Run 'whogitit doctor' to diagnose configuration issues.",
    )?;

    // Run blame with improved error context
    let revision_display = args.revision.as_deref().unwrap_or("HEAD");
    let mut result = blamer
        .blame(&args.file, args.revision.as_deref())
        .with_context(|| {
            format!(
                "Failed to blame '{}' at revision '{}'. \n\
                 Suggestions:\n  \
                 - Verify the file exists: git show {}:{}\n  \
                 - Check the revision is valid: git rev-parse {}\n  \
                 - Try with HEAD: whogitit blame {}",
                args.file,
                revision_display,
                revision_display,
                args.file,
                revision_display,
                args.file
            )
        })?;

    // Filter lines if requested
    if args.ai_only {
        result.lines.retain(|l| l.source.is_ai());
    } else if args.human_only {
        result.lines.retain(|l| l.source.is_human());
    }

    // Format output
    let output = format_blame(&result, args.format);
    print!("{}", output);

    Ok(())
}
