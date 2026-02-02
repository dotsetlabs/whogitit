//! Retention policy engine shared by CLI and hooks

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use git2::{Oid, Repository};
use std::collections::HashSet;

use crate::privacy::RetentionConfig;
use crate::storage::audit::AuditLog;
use crate::storage::notes::NotesStore;

/// Retention computation result
#[derive(Debug)]
pub struct RetentionSets {
    pub to_delete: Vec<Oid>,
    pub to_keep: Vec<Oid>,
}

/// Retention execution result
#[derive(Debug)]
pub struct RetentionResult {
    pub deleted_count: usize,
    pub sets: RetentionSets,
}

/// Compute which commits should be deleted vs kept based on retention policy
pub fn compute_retention_sets(
    repo: &Repository,
    retention: &RetentionConfig,
) -> Result<RetentionSets> {
    let notes_store = NotesStore::new(repo)?;
    let commits = notes_store.list_attributed_commits()?;

    let retained_commits = get_retained_commits(repo, &retention.retain_refs)?;
    let cutoff = retention
        .max_age_days
        .map(|days| Utc::now() - Duration::days(days as i64));

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

    apply_min_commits_to_sets(&mut to_delete, &mut to_keep, retention.min_commits);

    Ok(RetentionSets { to_delete, to_keep })
}

/// Apply retention policy (execute=false for dry run)
pub fn apply_retention_policy(
    repo: &Repository,
    retention: &RetentionConfig,
    execute: bool,
    reason: &str,
    audit_log_enabled: bool,
) -> Result<RetentionResult> {
    let sets = compute_retention_sets(repo, retention)?;

    if execute {
        let notes_store = NotesStore::new(repo)?;
        for commit_oid in &sets.to_delete {
            notes_store.remove_attribution(*commit_oid)?;
        }

        if audit_log_enabled {
            if let Some(repo_root) = repo.workdir() {
                let audit_log = AuditLog::new(repo_root);
                audit_log.log_retention(sets.to_delete.len() as u32, reason)?;
            }
        }
    }

    Ok(RetentionResult {
        deleted_count: if execute { sets.to_delete.len() } else { 0 },
        sets,
    })
}

/// Apply min_commits logic by moving some deletions to keep
fn apply_min_commits_to_sets(
    to_delete: &mut Vec<Oid>,
    to_keep: &mut Vec<Oid>,
    min_commits: Option<u32>,
) {
    let min_keep = min_commits.unwrap_or(0) as usize;
    if to_keep.len() >= min_keep || to_delete.is_empty() {
        return;
    }

    let need = min_keep - to_keep.len();
    let save_count = need.min(to_delete.len());
    for _ in 0..save_count {
        if let Some(oid) = to_delete.pop() {
            to_keep.push(oid);
        }
    }
}

/// Check if a commit is old based on cutoff
pub fn is_commit_old(commit_time: DateTime<Utc>, max_age_days: Option<u32>) -> bool {
    match max_age_days {
        Some(days) => {
            let cutoff = Utc::now() - Duration::days(days as i64);
            commit_time < cutoff
        }
        None => false,
    }
}

/// Get all commits that are reachable from retained refs
fn get_retained_commits(repo: &Repository, retain_refs: &[String]) -> Result<HashSet<Oid>> {
    let mut retained = HashSet::new();

    for ref_name in retain_refs {
        if let Ok(reference) = repo.find_reference(ref_name) {
            if let Ok(commit) = reference.peel_to_commit() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_is_commit_old_no_max_age() {
        let commit_time = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
        assert!(!is_commit_old(commit_time, None));
    }

    #[test]
    fn test_is_commit_old_recent_commit() {
        let commit_time = Utc::now() - Duration::days(1);
        assert!(!is_commit_old(commit_time, Some(30)));
    }

    #[test]
    fn test_is_commit_old_old_commit() {
        let commit_time = Utc::now() - Duration::days(100);
        assert!(is_commit_old(commit_time, Some(30)));
    }

    #[test]
    fn test_is_commit_old_exactly_at_cutoff() {
        let commit_time = Utc::now() - Duration::days(30);
        let _ = is_commit_old(commit_time, Some(30));
    }

    #[test]
    fn test_is_commit_old_future_commit() {
        let commit_time = Utc::now() + Duration::days(10);
        assert!(!is_commit_old(commit_time, Some(30)));
    }

    #[test]
    fn test_is_commit_old_zero_max_age() {
        let commit_time = Utc::now() - Duration::seconds(1);
        assert!(is_commit_old(commit_time, Some(0)));
    }

    #[test]
    fn test_is_commit_old_very_old_commit() {
        let commit_time = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
        assert!(is_commit_old(commit_time, Some(365)));
    }
}
