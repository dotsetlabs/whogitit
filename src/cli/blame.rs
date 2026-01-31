use anyhow::{Context, Result};
use clap::Args;
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

/// Run the blame command
pub fn run(args: BlameArgs) -> Result<()> {
    // Open repository
    let repo = Repository::discover(".").context("Not in a git repository")?;

    // Create blamer
    let mut blamer = AIBlamer::new(&repo)?;

    // Run blame
    let mut result = blamer.blame(&args.file, args.revision.as_deref())?;

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
