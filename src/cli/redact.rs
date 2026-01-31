//! Redact-test command for testing redaction patterns

use anyhow::Result;
use colored::Colorize;

use crate::privacy::{PrivacyConfig, WhogititConfig};

/// Arguments for redact-test command
#[derive(Debug, clap::Args)]
pub struct RedactArgs {
    /// Text to test redaction on
    #[arg(long, conflicts_with = "file")]
    pub text: Option<String>,

    /// File to read and test redaction on
    #[arg(long, conflicts_with = "text")]
    pub file: Option<String>,

    /// Show only matches without redacting
    #[arg(long)]
    pub matches_only: bool,

    /// Show audit trail
    #[arg(long)]
    pub audit: bool,

    /// List available patterns
    #[arg(long)]
    pub list_patterns: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Run the redact-test command
pub fn run(args: RedactArgs) -> Result<()> {
    // Handle list-patterns mode
    if args.list_patterns {
        return list_patterns(args.json);
    }

    // Get input text
    let input = get_input(&args)?;

    // Load config and build redactor
    let repo = git2::Repository::discover(".").ok();
    let config = match &repo {
        Some(r) => {
            let root = r.workdir().unwrap_or(std::path::Path::new("."));
            WhogititConfig::load(root).unwrap_or_default()
        }
        None => WhogititConfig::default(),
    };

    let redactor = config.privacy.build_redactor();

    if args.json {
        run_json_output(&input, &redactor, args.audit)
    } else if args.matches_only {
        run_matches_only(&input, &redactor)
    } else if args.audit {
        run_with_audit(&input, &redactor)
    } else {
        run_basic(&input, &redactor)
    }
}

fn get_input(args: &RedactArgs) -> Result<String> {
    match (&args.text, &args.file) {
        (Some(text), None) => Ok(text.clone()),
        (None, Some(file)) => {
            std::fs::read_to_string(file).map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))
        }
        (None, None) => {
            anyhow::bail!("Either --text or --file is required (or use --list-patterns)")
        }
        (Some(_), Some(_)) => {
            anyhow::bail!("Cannot specify both --text and --file")
        }
    }
}

fn list_patterns(json: bool) -> Result<()> {
    let patterns = PrivacyConfig::available_patterns();

    if json {
        let json_patterns: Vec<_> = patterns
            .iter()
            .map(|(name, desc)| {
                serde_json::json!({
                    "name": name,
                    "description": desc,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_patterns)?);
    } else {
        println!("{}", "Available Redaction Patterns".bold());
        println!("{}", "=".repeat(50));
        for (name, description) in patterns {
            println!("{:16} {}", name.cyan(), description);
        }
    }

    Ok(())
}

fn run_basic(input: &str, redactor: &crate::privacy::Redactor) -> Result<()> {
    let output = redactor.redact(input);

    if output == input {
        println!("{}", "No sensitive data detected.".green());
    } else {
        println!("{}", "Redacted output:".bold());
        println!("{}", output);
    }

    Ok(())
}

fn run_matches_only(input: &str, redactor: &crate::privacy::Redactor) -> Result<()> {
    let matches = redactor.find_sensitive_named(input);

    if matches.is_empty() {
        println!("{}", "No sensitive data detected.".green());
    } else {
        println!("{} {} found:", "Sensitive data".yellow(), matches.len());
        println!();
        for (name, matched) in matches {
            let preview = if matched.len() > 40 {
                format!("{}...", &matched[..40])
            } else {
                matched
            };
            println!("  {:16} {}", name.cyan(), preview.red());
        }
    }

    Ok(())
}

fn run_with_audit(input: &str, redactor: &crate::privacy::Redactor) -> Result<()> {
    let result = redactor.redact_with_audit(input);

    if result.redaction_count == 0 {
        println!("{}", "No sensitive data detected.".green());
    } else {
        println!(
            "{} {} redactions made:",
            "Audit Trail:".bold(),
            result.redaction_count
        );
        println!();

        for event in &result.events {
            println!(
                "  Pattern: {}  Range: {:?}  Preview: {}",
                event.pattern_name.cyan(),
                event.char_range,
                event.preview.red()
            );
        }

        println!();
        println!("{}", "Redacted output:".bold());
        println!("{}", result.text);
    }

    Ok(())
}

fn run_json_output(input: &str, redactor: &crate::privacy::Redactor, audit: bool) -> Result<()> {
    if audit {
        let result = redactor.redact_with_audit(input);
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        let output = redactor.redact(input);
        let matches = redactor.find_sensitive_named(input);

        let json = serde_json::json!({
            "input_length": input.len(),
            "output": output,
            "match_count": matches.len(),
            "matches": matches.iter().map(|(name, _)| name).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    }

    Ok(())
}
