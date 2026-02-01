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

#[cfg(test)]
mod tests {
    use super::*;

    // RedactArgs tests

    #[test]
    fn test_redact_args_with_text() {
        let args = RedactArgs {
            text: Some("test text".to_string()),
            file: None,
            matches_only: false,
            audit: false,
            list_patterns: false,
            json: false,
        };
        assert_eq!(args.text, Some("test text".to_string()));
        assert!(args.file.is_none());
    }

    #[test]
    fn test_redact_args_with_file() {
        let args = RedactArgs {
            text: None,
            file: Some("/path/to/file.txt".to_string()),
            matches_only: false,
            audit: false,
            list_patterns: false,
            json: false,
        };
        assert!(args.text.is_none());
        assert_eq!(args.file, Some("/path/to/file.txt".to_string()));
    }

    #[test]
    fn test_redact_args_list_patterns() {
        let args = RedactArgs {
            text: None,
            file: None,
            matches_only: false,
            audit: false,
            list_patterns: true,
            json: false,
        };
        assert!(args.list_patterns);
    }

    #[test]
    fn test_redact_args_output_modes() {
        // Test different output mode combinations
        let args_basic = RedactArgs {
            text: Some("test".to_string()),
            file: None,
            matches_only: false,
            audit: false,
            list_patterns: false,
            json: false,
        };
        assert!(!args_basic.matches_only && !args_basic.audit && !args_basic.json);

        let args_matches = RedactArgs {
            text: Some("test".to_string()),
            file: None,
            matches_only: true,
            audit: false,
            list_patterns: false,
            json: false,
        };
        assert!(args_matches.matches_only);

        let args_audit = RedactArgs {
            text: Some("test".to_string()),
            file: None,
            matches_only: false,
            audit: true,
            list_patterns: false,
            json: false,
        };
        assert!(args_audit.audit);

        let args_json = RedactArgs {
            text: Some("test".to_string()),
            file: None,
            matches_only: false,
            audit: false,
            list_patterns: false,
            json: true,
        };
        assert!(args_json.json);
    }

    // get_input tests

    #[test]
    fn test_get_input_with_text() {
        let args = RedactArgs {
            text: Some("inline text".to_string()),
            file: None,
            matches_only: false,
            audit: false,
            list_patterns: false,
            json: false,
        };
        let result = get_input(&args).unwrap();
        assert_eq!(result, "inline text");
    }

    #[test]
    fn test_get_input_neither_text_nor_file() {
        let args = RedactArgs {
            text: None,
            file: None,
            matches_only: false,
            audit: false,
            list_patterns: false,
            json: false,
        };
        let result = get_input(&args);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Either --text or --file is required"));
    }

    #[test]
    fn test_get_input_both_text_and_file() {
        let args = RedactArgs {
            text: Some("text".to_string()),
            file: Some("file.txt".to_string()),
            matches_only: false,
            audit: false,
            list_patterns: false,
            json: false,
        };
        let result = get_input(&args);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Cannot specify both"));
    }

    #[test]
    fn test_get_input_file_not_found() {
        let args = RedactArgs {
            text: None,
            file: Some("/nonexistent/path/file.txt".to_string()),
            matches_only: false,
            audit: false,
            list_patterns: false,
            json: false,
        };
        let result = get_input(&args);
        assert!(result.is_err());
    }

    // Available patterns test
    #[test]
    fn test_available_patterns_not_empty() {
        let patterns = PrivacyConfig::available_patterns();
        assert!(!patterns.is_empty());
    }

    #[test]
    fn test_available_patterns_have_descriptions() {
        let patterns = PrivacyConfig::available_patterns();
        for (name, description) in patterns {
            assert!(!name.is_empty());
            assert!(!description.is_empty());
        }
    }

    // Match preview truncation test (simulated from run_matches_only)
    #[test]
    fn test_match_preview_truncation() {
        let matched = "This is a very long matched string that exceeds 40 characters";
        let preview = if matched.len() > 40 {
            format!("{}...", &matched[..40])
        } else {
            matched.to_string()
        };
        assert!(preview.ends_with("..."));
        assert_eq!(preview.len(), 43); // 40 chars + "..."
    }

    #[test]
    fn test_match_preview_short() {
        let matched = "Short";
        let preview = if matched.len() > 40 {
            format!("{}...", &matched[..40])
        } else {
            matched.to_string()
        };
        assert_eq!(preview, "Short");
        assert!(!preview.ends_with("..."));
    }
}
