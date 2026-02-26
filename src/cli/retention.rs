//! Retention command for data retention policy management

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use colored::Colorize;
use git2::{Oid, Repository};

use crate::privacy::WhogititConfig;
use crate::retention::{apply_retention_policy_with_sets, compute_retention_sets};

const DEFAULT_PREVIEW_SHOW_LIMIT: usize = 25;

/// Arguments for retention command
#[derive(Debug, clap::Args)]
pub struct RetentionArgs {
    /// Subcommand
    #[command(subcommand)]
    pub action: RetentionAction,
}

/// Retention subcommands
#[derive(Debug, clap::Subcommand)]
pub enum RetentionAction {
    /// Preview what would be deleted based on current policy
    Preview {
        /// Number of deletable commits to list in preview output
        #[arg(long = "show", default_value_t = DEFAULT_PREVIEW_SHOW_LIMIT)]
        show: usize,
    },
    /// Apply retention policy (dry-run by default)
    Apply {
        /// Actually delete (without this flag, does a dry-run)
        #[arg(long)]
        execute: bool,

        /// Reason for deletion (for audit log)
        #[arg(long)]
        reason: Option<String>,
    },
    /// Show current retention configuration
    Config,
}

/// Run the retention command
pub fn run(args: RetentionArgs) -> Result<()> {
    match args.action {
        RetentionAction::Preview { show } => run_preview(show),
        RetentionAction::Apply { execute, reason } => run_apply(execute, reason),
        RetentionAction::Config => run_config(),
    }
}

fn run_preview(show_limit: usize) -> Result<()> {
    let repo = git2::Repository::discover(".").context("Not in a git repository")?;
    let repo_root = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("No working directory"))?;

    let config = WhogititConfig::load(repo_root).context("Failed to load configuration")?;
    let retention = config.retention.unwrap_or_default();

    let sets = compute_retention_sets(&repo, &retention)?;
    if sets.to_delete.is_empty() && sets.to_keep.is_empty() {
        println!("No attribution data found.");
        return Ok(());
    }
    let to_delete_previews = load_commit_previews(&repo, &sets.to_delete, show_limit);

    // Print summary
    println!("{}", "Retention Policy Preview".bold());
    println!("{}", "=".repeat(50));

    if let Some(days) = retention.max_age_days {
        println!("Max age: {} days", days);
    } else {
        println!("Max age: unlimited");
    }

    if !retention.retain_refs.is_empty() {
        println!("Retained refs: {}", retention.retain_refs.join(", "));
    }

    if let Some(min) = retention.min_commits {
        println!("Min commits to keep: {}", min);
    }
    println!("Preview list size: {}", show_limit);

    println!();
    println!("{} {} commits to keep", "●".green(), sets.to_keep.len());
    println!("{} {} commits to delete", "●".red(), sets.to_delete.len());

    if !sets.to_delete.is_empty() {
        println!();
        if show_limit == 0 {
            println!("Commit list hidden (--show 0).");
        } else {
            println!(
                "Commits that would be deleted (showing up to {}):",
                show_limit
            );
            for preview in &to_delete_previews {
                let short = short_oid(preview.oid);
                println!(
                    "  {} {} ({}) - {}",
                    short.red(),
                    preview.message,
                    preview.time.format("%Y-%m-%d"),
                    "would be deleted".dimmed()
                );
            }

            let hidden_count = sets
                .to_delete
                .len()
                .saturating_sub(to_delete_previews.len());
            if hidden_count > 0 {
                println!(
                    "  ... and {} more not shown (increase with --show)",
                    hidden_count
                );
            }
        }
        println!();
        println!("Run 'whogitit retention apply --execute' to delete these.");
    }

    Ok(())
}

#[derive(Debug)]
struct CommitPreview {
    oid: Oid,
    message: String,
    time: DateTime<Utc>,
}

fn load_commit_previews(
    repo: &Repository,
    commit_oids: &[Oid],
    show_limit: usize,
) -> Vec<CommitPreview> {
    if show_limit == 0 {
        return Vec::new();
    }

    let mut previews = Vec::new();

    for oid in commit_oids.iter().take(show_limit) {
        match repo.find_commit(*oid) {
            Ok(commit) => {
                let time = DateTime::from_timestamp(commit.time().seconds(), 0)
                    .unwrap_or(DateTime::UNIX_EPOCH);
                previews.push(CommitPreview {
                    oid: *oid,
                    message: commit.summary().unwrap_or("(no message)").to_string(),
                    time,
                });
            }
            Err(e) => {
                eprintln!(
                    "whogitit: Warning - skipping missing commit {} in retention preview: {}",
                    oid, e
                );
            }
        }
    }

    previews
}

fn short_oid(oid: Oid) -> String {
    let oid_str = oid.to_string();
    oid_str.chars().take(7).collect()
}

fn run_apply(execute: bool, reason: Option<String>) -> Result<()> {
    let repo = git2::Repository::discover(".").context("Not in a git repository")?;
    let repo_root = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("No working directory"))?;

    let config = WhogititConfig::load(repo_root).context("Failed to load configuration")?;
    let retention = config.retention.unwrap_or_default();

    let sets = compute_retention_sets(&repo, &retention)?;
    if sets.to_delete.is_empty() && sets.to_keep.is_empty() {
        println!("No attribution data found.");
        return Ok(());
    }
    if sets.to_delete.is_empty() {
        println!("No commits to delete based on current policy.");
        return Ok(());
    }

    if !execute {
        println!(
            "{} {} commits would be deleted (dry-run)",
            "Preview:".yellow(),
            sets.to_delete.len()
        );
        println!("Run with --execute to actually delete.");
        return Ok(());
    }

    let reason_str = reason.unwrap_or_else(|| "Retention policy".to_string());
    let result =
        apply_retention_policy_with_sets(&repo, sets, true, &reason_str, config.privacy.audit_log)?;

    println!(
        "{} Deleted attribution for {} commits",
        "Done:".green(),
        result.deleted_count
    );
    println!("Reason: {}", reason_str);

    Ok(())
}

fn run_config() -> Result<()> {
    let repo = git2::Repository::discover(".").context("Not in a git repository")?;
    let repo_root = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("No working directory"))?;

    let config = WhogititConfig::load(repo_root).context("Failed to load configuration")?;
    let retention = config.retention.unwrap_or_default();

    println!("{}", "Current Retention Configuration".bold());
    println!("{}", "=".repeat(50));

    if WhogititConfig::exists_for_repo(repo_root) {
        println!(
            "Config file: {}",
            repo_root.join(".whogitit.toml").display()
        );
    } else {
        println!("Config file: {} (using defaults)", "(not found)".dimmed());
    }

    println!();
    println!(
        "max_age_days: {}",
        retention
            .max_age_days
            .map(|d| d.to_string())
            .unwrap_or_else(|| "(unlimited)".to_string())
    );
    println!("auto_purge: {}", retention.auto_purge);
    println!(
        "retain_refs: {}",
        if retention.retain_refs.is_empty() {
            "(none)".to_string()
        } else {
            retention.retain_refs.join(", ")
        }
    );
    println!(
        "min_commits: {}",
        retention
            .min_commits
            .map(|c| c.to_string())
            .unwrap_or_else(|| "(none)".to_string())
    );

    println!();
    println!("{}", "Example configuration:".dimmed());
    println!(
        "{}",
        r#"
# .whogitit.toml
[retention]
max_age_days = 365
auto_purge = false
retain_refs = ["refs/heads/main"]
min_commits = 100
"#
        .dimmed()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retention::is_commit_old;
    use chrono::{Duration, Utc};

    // RetentionAction enum tests

    #[test]
    fn test_retention_action_variants() {
        let _preview = RetentionAction::Preview {
            show: DEFAULT_PREVIEW_SHOW_LIMIT,
        };
        let _apply = RetentionAction::Apply {
            execute: false,
            reason: None,
        };
        let _config = RetentionAction::Config;
    }

    #[test]
    fn test_retention_apply_with_reason() {
        let action = RetentionAction::Apply {
            execute: true,
            reason: Some("GDPR request".to_string()),
        };
        match action {
            RetentionAction::Apply { execute, reason } => {
                assert!(execute);
                assert_eq!(reason, Some("GDPR request".to_string()));
            }
            _ => panic!("Wrong variant"),
        }
    }

    // Integration test for delete/keep classification logic
    #[test]
    fn test_retention_classification_logic() {
        // Simulate the retention classification logic
        let now = Utc::now();
        let old_time = now - Duration::days(100);
        let recent_time = now - Duration::days(10);
        let max_age_days = Some(30u32);

        // Old commit, not retained
        let is_old1 = is_commit_old(old_time, max_age_days);
        let is_retained1 = false;
        let should_delete1 = is_old1 && !is_retained1;
        assert!(should_delete1);

        // Recent commit, not retained
        let is_old2 = is_commit_old(recent_time, max_age_days);
        let is_retained2 = false;
        let should_delete2 = is_old2 && !is_retained2;
        assert!(!should_delete2);

        // Old commit, but retained
        let is_old3 = is_commit_old(old_time, max_age_days);
        let is_retained3 = true;
        let should_delete3 = is_old3 && !is_retained3;
        assert!(!should_delete3);
    }

    #[test]
    fn test_short_oid_output_length() {
        let oid =
            Oid::from_str("0123456789abcdef0123456789abcdef01234567").expect("valid test oid");
        assert_eq!(short_oid(oid), "0123456");
    }
}
