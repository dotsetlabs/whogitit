//! Retention command for data retention policy management

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use colored::Colorize;
use std::collections::HashSet;

use crate::privacy::WhogititConfig;
use crate::storage::audit::AuditLog;
use crate::storage::notes::NotesStore;

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
    Preview,
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
        RetentionAction::Preview => run_preview(),
        RetentionAction::Apply { execute, reason } => run_apply(execute, reason),
        RetentionAction::Config => run_config(),
    }
}

fn run_preview() -> Result<()> {
    let repo = git2::Repository::discover(".").context("Not in a git repository")?;
    let repo_root = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("No working directory"))?;

    let config = WhogititConfig::load(repo_root).unwrap_or_default();
    let retention = config.retention.unwrap_or_default();

    let notes_store = NotesStore::new(&repo)?;
    let commits = notes_store.list_attributed_commits()?;

    if commits.is_empty() {
        println!("No attribution data found.");
        return Ok(());
    }

    // Build set of retained refs
    let retained_commits = get_retained_commits(&repo, &retention.retain_refs)?;

    // Calculate cutoff date
    let cutoff = retention
        .max_age_days
        .map(|days| Utc::now() - Duration::days(days as i64));

    // Analyze commits
    let mut to_delete = Vec::new();
    let mut to_keep = Vec::new();

    for commit_oid in commits {
        let commit = repo.find_commit(commit_oid)?;
        let commit_time =
            DateTime::from_timestamp(commit.time().seconds(), 0).unwrap_or(DateTime::UNIX_EPOCH);

        let is_retained = retained_commits.contains(&commit_oid);
        let is_old = cutoff.map(|c| commit_time < c).unwrap_or(false);

        if is_old && !is_retained {
            to_delete.push((commit_oid, commit));
        } else {
            to_keep.push((commit_oid, commit));
        }
    }

    // Apply min_commits if specified
    let min_keep = retention.min_commits.unwrap_or(0) as usize;
    if to_keep.len() < min_keep && !to_delete.is_empty() {
        // Move some from delete to keep
        let need = min_keep - to_keep.len();
        let save_count = need.min(to_delete.len());
        for _ in 0..save_count {
            if let Some(item) = to_delete.pop() {
                to_keep.push(item);
            }
        }
    }

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

    println!();
    println!("{} {} commits to keep", "●".green(), to_keep.len());
    println!("{} {} commits to delete", "●".red(), to_delete.len());

    if !to_delete.is_empty() {
        println!();
        println!("Commits that would be deleted:");
        for (oid, commit) in &to_delete {
            let short = &oid.to_string()[..7];
            let msg = commit.summary().unwrap_or("(no message)");
            let time = DateTime::from_timestamp(commit.time().seconds(), 0)
                .unwrap_or(DateTime::UNIX_EPOCH);
            println!(
                "  {} {} ({}) - {}",
                short.red(),
                msg,
                time.format("%Y-%m-%d"),
                "would be deleted".dimmed()
            );
        }
        println!();
        println!("Run 'whogitit retention apply --execute' to delete these.");
    }

    Ok(())
}

fn run_apply(execute: bool, reason: Option<String>) -> Result<()> {
    let repo = git2::Repository::discover(".").context("Not in a git repository")?;
    let repo_root = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("No working directory"))?;

    let config = WhogititConfig::load(repo_root).unwrap_or_default();
    let retention = config.retention.unwrap_or_default();

    let notes_store = NotesStore::new(&repo)?;
    let commits = notes_store.list_attributed_commits()?;

    if commits.is_empty() {
        println!("No attribution data found.");
        return Ok(());
    }

    // Build set of retained refs
    let retained_commits = get_retained_commits(&repo, &retention.retain_refs)?;

    // Calculate cutoff date
    let cutoff = retention
        .max_age_days
        .map(|days| Utc::now() - Duration::days(days as i64));

    // Analyze commits
    let mut to_delete = Vec::new();
    let mut to_keep = Vec::new();

    for commit_oid in commits {
        let commit = repo.find_commit(commit_oid)?;
        let commit_time =
            DateTime::from_timestamp(commit.time().seconds(), 0).unwrap_or(DateTime::UNIX_EPOCH);

        let is_retained = retained_commits.contains(&commit_oid);
        let is_old = cutoff.map(|c| commit_time < c).unwrap_or(false);

        if is_old && !is_retained {
            to_delete.push(commit_oid);
        } else {
            to_keep.push(commit_oid);
        }
    }

    // Apply min_commits if specified
    let min_keep = retention.min_commits.unwrap_or(0) as usize;
    if to_keep.len() < min_keep && !to_delete.is_empty() {
        let need = min_keep - to_keep.len();
        let save_count = need.min(to_delete.len());
        for _ in 0..save_count {
            if let Some(oid) = to_delete.pop() {
                to_keep.push(oid);
            }
        }
    }

    if to_delete.is_empty() {
        println!("No commits to delete based on current policy.");
        return Ok(());
    }

    if !execute {
        println!(
            "{} {} commits would be deleted (dry-run)",
            "Preview:".yellow(),
            to_delete.len()
        );
        println!("Run with --execute to actually delete.");
        return Ok(());
    }

    // Actually delete
    let reason_str = reason.unwrap_or_else(|| "Retention policy".to_string());

    for commit_oid in &to_delete {
        notes_store.remove_attribution(*commit_oid)?;
    }

    println!(
        "{} Deleted attribution for {} commits",
        "Done:".green(),
        to_delete.len()
    );
    println!("Reason: {}", reason_str);

    if config.privacy.audit_log {
        let audit_log = AuditLog::new(repo_root);
        audit_log.log_retention(to_delete.len() as u32, &reason_str)?;
    }

    Ok(())
}

fn run_config() -> Result<()> {
    let repo = git2::Repository::discover(".").context("Not in a git repository")?;
    let repo_root = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("No working directory"))?;

    let config = WhogititConfig::load(repo_root).unwrap_or_default();
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

/// Get all commits that are reachable from retained refs
fn get_retained_commits(
    repo: &git2::Repository,
    retain_refs: &[String],
) -> Result<HashSet<git2::Oid>> {
    let mut retained = HashSet::new();

    for ref_name in retain_refs {
        if let Ok(reference) = repo.find_reference(ref_name) {
            if let Ok(commit) = reference.peel_to_commit() {
                // Walk the commit history
                let mut revwalk = repo.revwalk()?;
                revwalk.push(commit.id())?;

                for oid in revwalk.flatten() {
                    retained.insert(oid);
                }
            }
        }
    }

    Ok(retained)
}

/// Apply min_commits logic - returns (to_delete, to_keep) counts after adjustment
/// This is extracted for testing purposes
#[allow(dead_code)]
fn apply_min_commits(
    to_delete_count: usize,
    to_keep_count: usize,
    min_commits: Option<u32>,
) -> (usize, usize) {
    let min_keep = min_commits.unwrap_or(0) as usize;
    if to_keep_count >= min_keep || to_delete_count == 0 {
        return (to_delete_count, to_keep_count);
    }

    // Need to save some from deletion
    let need = min_keep - to_keep_count;
    let save_count = need.min(to_delete_count);

    (to_delete_count - save_count, to_keep_count + save_count)
}

/// Check if a commit is old based on cutoff
#[allow(dead_code)]
fn is_commit_old(commit_time: DateTime<Utc>, max_age_days: Option<u32>) -> bool {
    match max_age_days {
        Some(days) => {
            let cutoff = Utc::now() - Duration::days(days as i64);
            commit_time < cutoff
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    // RetentionAction enum tests

    #[test]
    fn test_retention_action_variants() {
        let _preview = RetentionAction::Preview;
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

    // apply_min_commits tests

    #[test]
    fn test_apply_min_commits_no_limit() {
        // No min_commits specified - keep original counts
        let (delete, keep) = apply_min_commits(5, 10, None);
        assert_eq!(delete, 5);
        assert_eq!(keep, 10);
    }

    #[test]
    fn test_apply_min_commits_already_met() {
        // Already have enough to keep
        let (delete, keep) = apply_min_commits(5, 100, Some(50));
        assert_eq!(delete, 5);
        assert_eq!(keep, 100);
    }

    #[test]
    fn test_apply_min_commits_save_some() {
        // Need to save some from deletion
        // Have 10 to delete, 30 to keep, need min 50
        // Need to save 20, but only have 10 to delete
        let (delete, keep) = apply_min_commits(10, 30, Some(50));
        assert_eq!(delete, 0); // All 10 saved
        assert_eq!(keep, 40); // 30 + 10 saved
    }

    #[test]
    fn test_apply_min_commits_save_partial() {
        // Need to save some from deletion
        // Have 30 to delete, 10 to keep, need min 25
        // Need 15, have 30 available
        let (delete, keep) = apply_min_commits(30, 10, Some(25));
        assert_eq!(delete, 15); // 30 - 15 saved
        assert_eq!(keep, 25); // 10 + 15 = exactly min
    }

    #[test]
    fn test_apply_min_commits_nothing_to_delete() {
        // Nothing to delete - should do nothing
        let (delete, keep) = apply_min_commits(0, 5, Some(100));
        assert_eq!(delete, 0);
        assert_eq!(keep, 5);
    }

    #[test]
    fn test_apply_min_commits_zero_min() {
        // min_commits = 0 should be treated as no minimum
        let (delete, keep) = apply_min_commits(10, 5, Some(0));
        assert_eq!(delete, 10);
        assert_eq!(keep, 5);
    }

    // is_commit_old tests

    #[test]
    fn test_is_commit_old_no_max_age() {
        let commit_time = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
        assert!(!is_commit_old(commit_time, None));
    }

    #[test]
    fn test_is_commit_old_recent_commit() {
        // Commit from 1 day ago with 30 day max age
        let commit_time = Utc::now() - Duration::days(1);
        assert!(!is_commit_old(commit_time, Some(30)));
    }

    #[test]
    fn test_is_commit_old_old_commit() {
        // Commit from 100 days ago with 30 day max age
        let commit_time = Utc::now() - Duration::days(100);
        assert!(is_commit_old(commit_time, Some(30)));
    }

    #[test]
    fn test_is_commit_old_exactly_at_cutoff() {
        // Commit from exactly 30 days ago with 30 day max age
        // Due to sub-second timing, this could go either way
        // Just verify it doesn't panic
        let commit_time = Utc::now() - Duration::days(30);
        let _ = is_commit_old(commit_time, Some(30));
    }

    #[test]
    fn test_is_commit_old_future_commit() {
        // Edge case: commit in the future
        let commit_time = Utc::now() + Duration::days(10);
        assert!(!is_commit_old(commit_time, Some(30)));
    }

    #[test]
    fn test_is_commit_old_zero_max_age() {
        // Zero max age - everything is old except future commits
        let commit_time = Utc::now() - Duration::seconds(1);
        assert!(is_commit_old(commit_time, Some(0)));
    }

    #[test]
    fn test_is_commit_old_very_old_commit() {
        // Very old commit
        let commit_time = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
        assert!(is_commit_old(commit_time, Some(365)));
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
}
