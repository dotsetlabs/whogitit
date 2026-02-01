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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::snapshot::LineSource;
    use crate::core::attribution::BlameLineResult;

    // BlameArgs tests

    #[test]
    fn test_blame_args_defaults() {
        // Verify default values exist in the structure
        let args = BlameArgs {
            file: "test.rs".to_string(),
            revision: None,
            format: OutputFormat::Pretty,
            ai_only: false,
            human_only: false,
        };
        assert_eq!(args.file, "test.rs");
        assert!(args.revision.is_none());
        assert!(matches!(args.format, OutputFormat::Pretty));
        assert!(!args.ai_only);
        assert!(!args.human_only);
    }

    #[test]
    fn test_blame_args_with_revision() {
        let args = BlameArgs {
            file: "src/main.rs".to_string(),
            revision: Some("abc1234".to_string()),
            format: OutputFormat::Json,
            ai_only: true,
            human_only: false,
        };
        assert_eq!(args.revision, Some("abc1234".to_string()));
        assert!(matches!(args.format, OutputFormat::Json));
    }

    // Filter logic tests

    #[test]
    fn test_ai_only_filter() {
        let mut lines = vec![
            create_test_blame_line(
                1,
                LineSource::AI {
                    edit_id: "e1".to_string(),
                },
            ),
            create_test_blame_line(2, LineSource::Human),
            create_test_blame_line(
                3,
                LineSource::AIModified {
                    edit_id: "e2".to_string(),
                    similarity: 0.8,
                },
            ),
            create_test_blame_line(4, LineSource::Original),
        ];

        // Apply ai_only filter
        lines.retain(|l| l.source.is_ai());

        assert_eq!(lines.len(), 2);
        assert!(lines[0].source.is_ai());
        assert!(lines[1].source.is_ai());
    }

    #[test]
    fn test_human_only_filter() {
        let mut lines = vec![
            create_test_blame_line(
                1,
                LineSource::AI {
                    edit_id: "e1".to_string(),
                },
            ),
            create_test_blame_line(2, LineSource::Human),
            create_test_blame_line(
                3,
                LineSource::AIModified {
                    edit_id: "e2".to_string(),
                    similarity: 0.8,
                },
            ),
            create_test_blame_line(4, LineSource::Original),
        ];

        // Apply human_only filter
        lines.retain(|l| l.source.is_human());

        assert_eq!(lines.len(), 2);
        assert!(lines[0].source.is_human());
        assert!(lines[1].source.is_human());
    }

    #[test]
    fn test_no_filter() {
        #[allow(clippy::useless_vec)]
        let lines = vec![
            create_test_blame_line(
                1,
                LineSource::AI {
                    edit_id: "e1".to_string(),
                },
            ),
            create_test_blame_line(2, LineSource::Human),
            create_test_blame_line(
                3,
                LineSource::AIModified {
                    edit_id: "e2".to_string(),
                    similarity: 0.8,
                },
            ),
            create_test_blame_line(4, LineSource::Original),
        ];

        // No filter - all lines retained
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn test_filter_empty_result() {
        let mut lines = vec![
            create_test_blame_line(1, LineSource::Human),
            create_test_blame_line(2, LineSource::Original),
        ];

        // Apply ai_only filter on lines with no AI
        lines.retain(|l| l.source.is_ai());

        assert!(lines.is_empty());
    }

    // Helper to create test BlameLineResult
    fn create_test_blame_line(line_num: u32, source: LineSource) -> BlameLineResult {
        BlameLineResult {
            line_number: line_num,
            commit_id: "abc1234567890".to_string(),
            commit_short: "abc1234".to_string(),
            author: "Test Author".to_string(),
            source,
            content: format!("line {} content", line_num),
            prompt_index: None,
            prompt_preview: None,
        }
    }

    // OutputFormat tests
    #[test]
    fn test_output_format_variants() {
        let _pretty = OutputFormat::Pretty;
        let _json = OutputFormat::Json;
        assert!(matches!(OutputFormat::default(), OutputFormat::Pretty));
    }

    // LineSource behavior tests
    #[test]
    fn test_line_source_is_ai() {
        assert!(LineSource::AI {
            edit_id: "e1".to_string()
        }
        .is_ai());
        assert!(LineSource::AIModified {
            edit_id: "e1".to_string(),
            similarity: 0.9
        }
        .is_ai());
        assert!(!LineSource::Human.is_ai());
        assert!(!LineSource::Original.is_ai());
    }

    #[test]
    fn test_line_source_is_human() {
        assert!(!LineSource::AI {
            edit_id: "e1".to_string()
        }
        .is_human());
        assert!(!LineSource::AIModified {
            edit_id: "e1".to_string(),
            similarity: 0.9
        }
        .is_human());
        assert!(LineSource::Human.is_human());
        assert!(LineSource::Original.is_human());
    }
}
