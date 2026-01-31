use anyhow::{bail, Context, Result};
use clap::Args;
use colored::Colorize;
use git2::Repository;

use crate::core::blame::AIBlamer;
use crate::utils::{pad_right, truncate, word_wrap};

/// Prompt command arguments
#[derive(Debug, Args)]
pub struct PromptArgs {
    /// File and line reference (e.g., "src/main.rs:42" or "src/main.rs")
    pub reference: String,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Parsed file:line reference
struct FileLineRef {
    file: String,
    line: Option<u32>,
}

impl FileLineRef {
    fn parse(reference: &str) -> Result<Self> {
        if let Some((file, line_str)) = reference.rsplit_once(':') {
            // Check if the part after : is actually a line number
            if let Ok(line) = line_str.parse::<u32>() {
                return Ok(Self {
                    file: file.to_string(),
                    line: Some(line),
                });
            }
        }

        // No line number, just a file path
        Ok(Self {
            file: reference.to_string(),
            line: None,
        })
    }
}

/// Run the prompt command
pub fn run(args: PromptArgs) -> Result<()> {
    // Parse reference
    let file_ref = FileLineRef::parse(&args.reference)?;

    // Open repository
    let repo = Repository::discover(".").context("Not in a git repository")?;

    // Create blamer
    let mut blamer = AIBlamer::new(&repo)?;

    // Run blame to find AI attribution
    let result = blamer.blame(&file_ref.file, None)?;

    // Find the relevant line
    let target_line = match file_ref.line {
        Some(line) => result.lines.iter().find(|l| l.line_number == line),
        None => {
            // Find first AI-generated line
            result.lines.iter().find(|l| l.source.is_ai())
        }
    };

    let line = match target_line {
        Some(l) => l,
        None => {
            if let Some(line_num) = file_ref.line {
                bail!("Line {} not found in {}", line_num, file_ref.file);
            } else {
                bail!("No AI-generated lines found in {}", file_ref.file);
            }
        }
    };

    if !line.source.is_ai() {
        bail!(
            "Line {} in {} was not AI-generated",
            line.line_number,
            file_ref.file
        );
    }

    // Get attribution for more details
    let attribution = blamer
        .get_commit_attribution(&line.commit_id)?
        .context("Failed to fetch attribution data")?;

    // Get the prompt info
    let prompt_info = line
        .prompt_index
        .and_then(|idx| attribution.get_prompt(idx));

    if args.json {
        let output = serde_json::json!({
            "file": file_ref.file,
            "line": line.line_number,
            "commit": line.commit_id,
            "source": format!("{:?}", line.source),
            "prompt_index": line.prompt_index,
            "prompt_text": prompt_info.map(|p| &p.text),
            "session": {
                "id": attribution.session.session_id,
                "model": attribution.session.model.id,
                "started_at": attribution.session.started_at,
            },
        });

        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        match prompt_info {
            Some(prompt) => {
                print_prompt_box(
                    prompt,
                    &attribution.session.session_id,
                    &attribution.session.model.id,
                    &attribution.session.started_at,
                );
            }
            None => {
                println!(
                    "Line {} is AI-generated but prompt details are not available.",
                    line.line_number
                );
            }
        }

        println!("File: {}:{}", file_ref.file, line.line_number);
        println!("Commit: {}", line.commit_short);
        println!("Source: {:?}", line.source);
    }

    Ok(())
}

fn print_prompt_box(
    prompt: &crate::core::attribution::PromptInfo,
    session_id: &str,
    model: &str,
    timestamp: &str,
) {
    // Box top
    println!("╔{}╗", "═".repeat(68));

    // Header
    println!(
        "║  {} #{} in session {}  ",
        "PROMPT".bold(),
        prompt.index,
        truncate(session_id, 20)
    );
    println!("║  Model: {} | {}  ", model.cyan(), timestamp.dimmed());

    // Separator
    println!("╠{}╣", "═".repeat(68));

    // Prompt content with word wrap
    for line in word_wrap(&prompt.text, 64) {
        println!("║  {}  ║", pad_right(&line, 64));
    }

    // Box bottom
    println!("╚{}╝", "═".repeat(68));
    println!();

    // Files affected
    if !prompt.affected_files.is_empty() {
        println!("{}", "Files affected by this prompt:".dimmed());
        for file in &prompt.affected_files {
            println!("  - {}", file);
        }
        println!();
    }
}
