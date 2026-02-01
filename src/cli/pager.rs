//! Pager command - annotate git diff output with AI attribution symbols
//!
//! This command reads git diff output from stdin and annotates it with
//! AI attribution markers, then passes it through to the default pager.
//!
//! Usage:
//!   git config --global core.pager "whogitit pager"
//!   # or as an alias:
//!   git config --global alias.ai-diff '!whogitit pager'

use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;
use git2::Repository;
use regex::Regex;

use crate::capture::snapshot::LineSource;
use crate::core::blame::AIBlamer;

/// Pager command arguments
#[derive(Debug, Args)]
pub struct PagerArgs {
    /// Disable colors in output
    #[arg(long)]
    pub no_color: bool,

    /// Show detailed attribution info (model, timestamp)
    #[arg(long, short)]
    pub verbose: bool,

    /// Bypass the pager and output directly to stdout
    #[arg(long)]
    pub no_pager: bool,
}

/// Attribution info for a line
#[derive(Debug, Clone)]
struct LineAttribution {
    source: LineSource,
    #[allow(dead_code)] // Reserved for future verbose output
    prompt_preview: Option<String>,
}

/// Run the pager command
pub fn run(args: PagerArgs) -> Result<()> {
    // Read diff from stdin
    let stdin = io::stdin();
    let lines: Vec<String> = stdin.lock().lines().map_while(Result::ok).collect();

    // If stdin is empty, just return
    if lines.is_empty() {
        return Ok(());
    }

    // Try to open repository for attribution lookup
    let repo = Repository::discover(".").ok();
    let mut blamer = repo.as_ref().and_then(|r| AIBlamer::new(r).ok());

    // Parse diff and build attribution map
    let attribution_map = if let Some(ref mut b) = blamer {
        build_attribution_map(&lines, b)
    } else {
        HashMap::new()
    };

    // Annotate the diff output
    let annotated = annotate_diff(&lines, &attribution_map, &args);

    // Output through pager or directly
    if args.no_pager || !atty::is(atty::Stream::Stdout) {
        // Direct output
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        for line in annotated {
            writeln!(handle, "{}", line)?;
        }
    } else {
        // Use pager
        output_through_pager(&annotated)?;
    }

    Ok(())
}

/// Build a map of file:line -> attribution by parsing diff hunks
fn build_attribution_map(
    diff_lines: &[String],
    blamer: &mut AIBlamer,
) -> HashMap<(String, u32), LineAttribution> {
    let mut map = HashMap::new();

    // Regex to match diff file headers
    let file_header_re = Regex::new(r"^\+\+\+ b/(.+)$").unwrap();
    // Regex to match hunk headers: @@ -start,count +start,count @@
    let hunk_re = Regex::new(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@").unwrap();

    let mut current_file: Option<String> = None;
    let mut current_line: u32 = 0;

    // Cache of blame results per file
    let mut blame_cache: HashMap<String, Vec<(u32, LineAttribution)>> = HashMap::new();

    for line in diff_lines {
        // Check for new file
        if let Some(caps) = file_header_re.captures(line) {
            let file_path = caps.get(1).map(|m| m.as_str().to_string());
            current_file = file_path.clone();

            // Prefetch blame for this file
            if let Some(ref path) = current_file {
                if !blame_cache.contains_key(path) {
                    if let Ok(blame_result) = blamer.blame(path, None) {
                        let attrs: Vec<(u32, LineAttribution)> = blame_result
                            .lines
                            .into_iter()
                            .map(|l| {
                                (
                                    l.line_number,
                                    LineAttribution {
                                        source: l.source,
                                        prompt_preview: l.prompt_preview,
                                    },
                                )
                            })
                            .collect();
                        blame_cache.insert(path.clone(), attrs);
                    }
                }
            }
            continue;
        }

        // Check for hunk header
        if let Some(caps) = hunk_re.captures(line) {
            if let Some(start) = caps.get(1) {
                current_line = start.as_str().parse().unwrap_or(1);
            }
            continue;
        }

        // Track line numbers for added lines
        if let Some(ref file) = current_file {
            if line.starts_with('+') && !line.starts_with("+++") {
                // This is an added line
                if let Some(file_attrs) = blame_cache.get(file) {
                    if let Some((_, attr)) = file_attrs.iter().find(|(ln, _)| *ln == current_line) {
                        map.insert((file.clone(), current_line), attr.clone());
                    }
                }
                current_line += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                // Deleted line - don't increment current_line
            } else if !line.starts_with('\\') {
                // Context line
                current_line += 1;
            }
        }
    }

    map
}

/// Annotate diff lines with AI attribution markers
fn annotate_diff(
    diff_lines: &[String],
    attribution_map: &HashMap<(String, u32), LineAttribution>,
    args: &PagerArgs,
) -> Vec<String> {
    let mut result = Vec::with_capacity(diff_lines.len());

    // Regex patterns
    let file_header_re = Regex::new(r"^\+\+\+ b/(.+)$").unwrap();
    let hunk_re = Regex::new(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@").unwrap();

    let mut current_file: Option<String> = None;
    let mut current_line: u32 = 0;

    for line in diff_lines {
        // Check for file header
        if let Some(caps) = file_header_re.captures(line) {
            current_file = caps.get(1).map(|m| m.as_str().to_string());
            result.push(line.clone());
            continue;
        }

        // Check for hunk header
        if let Some(caps) = hunk_re.captures(line) {
            if let Some(start) = caps.get(1) {
                current_line = start.as_str().parse().unwrap_or(1);
            }
            result.push(line.clone());
            continue;
        }

        // Check for added lines that might have attribution
        if line.starts_with('+') && !line.starts_with("+++") {
            if let Some(ref file) = current_file {
                if let Some(attr) = attribution_map.get(&(file.clone(), current_line)) {
                    let annotated = annotate_added_line(line, attr, args);
                    result.push(annotated);
                    current_line += 1;
                    continue;
                }
            }
            current_line += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            // Deleted line - no increment
        } else if !line.starts_with('\\') {
            // Context line
            current_line += 1;
        }

        result.push(line.clone());
    }

    result
}

/// Annotate a single added line with AI attribution
fn annotate_added_line(line: &str, attr: &LineAttribution, args: &PagerArgs) -> String {
    // Determine source type and build annotation
    let (is_ai, edit_id, similarity) = match &attr.source {
        LineSource::AI { edit_id } => (true, Some(edit_id.clone()), None),
        LineSource::AIModified {
            edit_id,
            similarity,
        } => (false, Some(edit_id.clone()), Some(*similarity)),
        _ => return line.to_string(),
    };

    // Build the annotation suffix
    let suffix = if args.verbose {
        if is_ai {
            if let Some(ref eid) = edit_id {
                format!("  # AI: {}", &eid[..8.min(eid.len())])
            } else {
                "  # AI".to_string()
            }
        } else if let Some(sim) = similarity {
            if let Some(ref eid) = edit_id {
                format!(
                    "  # AI-mod({:.0}%): {}",
                    sim * 100.0,
                    &eid[..8.min(eid.len())]
                )
            } else {
                format!("  # AI-mod({:.0}%)", sim * 100.0)
            }
        } else {
            "  # AI-mod".to_string()
        }
    } else if is_ai {
        "  # AI".to_string()
    } else {
        "  # AI-mod".to_string()
    };

    // Build marker and format output
    if args.no_color {
        let marker = if is_ai { "●" } else { "◐" };
        format!("{} {}{}", marker, line, suffix)
    } else {
        let colored_marker = if is_ai {
            "●".green().bold().to_string()
        } else {
            "◐".yellow().to_string()
        };
        let colored_suffix = suffix.dimmed().to_string();
        format!("{} {}{}", colored_marker, line, colored_suffix)
    }
}

/// Output through the system pager (less, more, etc.)
fn output_through_pager(lines: &[String]) -> Result<()> {
    // Try to use the user's preferred pager, falling back to less, then more
    let pager = std::env::var("PAGER").unwrap_or_else(|_| "less".to_string());

    // For less, add -R flag to handle colors
    let (cmd, args) = if pager.contains("less") {
        ("less", vec!["-R"])
    } else {
        (pager.as_str(), vec![])
    };

    let mut child = Command::new(cmd)
        .args(&args)
        .stdin(Stdio::piped())
        .spawn()
        .context("Failed to spawn pager")?;

    if let Some(mut stdin) = child.stdin.take() {
        for line in lines {
            writeln!(stdin, "{}", line)?;
        }
    }

    child.wait().context("Pager failed")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_header_regex() {
        let re = Regex::new(r"^\+\+\+ b/(.+)$").unwrap();
        let caps = re.captures("+++ b/src/main.rs").unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "src/main.rs");
    }

    #[test]
    fn test_hunk_header_regex() {
        let re = Regex::new(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@").unwrap();

        // Standard format
        let caps = re.captures("@@ -10,5 +15,8 @@ fn main()").unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "15");

        // Without count
        let caps = re.captures("@@ -1 +1 @@").unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "1");
    }

    #[test]
    fn test_annotate_added_line_ai() {
        let attr = LineAttribution {
            source: LineSource::AI {
                edit_id: "abc12345-uuid".to_string(),
            },
            prompt_preview: None,
        };
        let args = PagerArgs {
            no_color: true,
            verbose: false,
            no_pager: true,
        };

        let result = annotate_added_line("+    let x = 42;", &attr, &args);
        assert!(result.contains("●"));
        assert!(result.contains("# AI"));
    }

    #[test]
    fn test_annotate_added_line_ai_modified() {
        let attr = LineAttribution {
            source: LineSource::AIModified {
                edit_id: "abc12345-uuid".to_string(),
                similarity: 0.85,
            },
            prompt_preview: None,
        };
        let args = PagerArgs {
            no_color: true,
            verbose: true,
            no_pager: true,
        };

        let result = annotate_added_line("+    let y = 99;", &attr, &args);
        assert!(result.contains("◐"));
        assert!(result.contains("AI-mod(85%)"));
    }
}
