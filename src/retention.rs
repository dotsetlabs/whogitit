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

    let mut entries: Vec<RetentionEntry> = Vec::new();

    for commit_oid in commits {
        let commit = match repo.find_commit(commit_oid) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "whogitit: Warning - skipping missing commit {} during retention: {}",
                    commit_oid, e
                );
                continue;
            }
        };
        let commit_time =
            DateTime::from_timestamp(commit.time().seconds(), 0).unwrap_or(DateTime::UNIX_EPOCH);

        let is_retained = retained_commits.contains(&commit_oid);
        let is_old = cutoff.map(|c| commit_time < c).unwrap_or(false);

        entries.push(RetentionEntry {
            oid: commit_oid,
            time: commit_time,
            is_retained,
            is_old,
        });
    }

    let (to_delete, to_keep) = compute_sets_from_entries(&entries, retention.min_commits);

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
    apply_retention_policy_with_sets(repo, sets, execute, reason, audit_log_enabled)
}

/// Apply a retention policy using precomputed retention sets.
///
/// This allows callers that already computed sets for preview/validation
/// to avoid recomputing before execute.
pub fn apply_retention_policy_with_sets(
    repo: &Repository,
    sets: RetentionSets,
    execute: bool,
    reason: &str,
    audit_log_enabled: bool,
) -> Result<RetentionResult> {
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

#[derive(Debug)]
struct RetentionEntry {
    oid: Oid,
    time: DateTime<Utc>,
    is_retained: bool,
    is_old: bool,
}

fn compute_sets_from_entries(
    entries: &[RetentionEntry],
    min_commits: Option<u32>,
) -> (Vec<Oid>, Vec<Oid>) {
    let mut keep: Vec<Oid> = entries
        .iter()
        .filter(|e| e.is_retained || !e.is_old)
        .map(|e| e.oid)
        .collect();

    let min_keep = min_commits.unwrap_or(0) as usize;
    if keep.len() < min_keep {
        let mut candidates: Vec<&RetentionEntry> = entries
            .iter()
            .filter(|e| !e.is_retained && e.is_old)
            .collect();
        candidates.sort_by_key(|e| std::cmp::Reverse(e.time));

        let need = min_keep - keep.len();
        for entry in candidates.into_iter().take(need) {
            keep.push(entry.oid);
        }
    }

    let keep_set: std::collections::HashSet<Oid> = keep.iter().copied().collect();
    let mut delete: Vec<Oid> = entries
        .iter()
        .filter(|e| e.is_old && !e.is_retained && !keep_set.contains(&e.oid))
        .map(|e| e.oid)
        .collect();

    delete.sort();
    keep.sort();

    (delete, keep)
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
    use std::collections::HashSet;

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

    #[test]
    fn test_min_commits_keeps_newest() {
        let entries = vec![
            RetentionEntry {
                oid: Oid::from_bytes(&[1; 20]).unwrap(),
                time: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
                is_retained: false,
                is_old: true,
            },
            RetentionEntry {
                oid: Oid::from_bytes(&[2; 20]).unwrap(),
                time: Utc.with_ymd_and_hms(2021, 1, 1, 0, 0, 0).unwrap(),
                is_retained: false,
                is_old: true,
            },
            RetentionEntry {
                oid: Oid::from_bytes(&[3; 20]).unwrap(),
                time: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                is_retained: false,
                is_old: true,
            },
        ];

        let (to_delete, to_keep) = compute_sets_from_entries(&entries, Some(2));

        assert_eq!(to_keep.len(), 2);
        assert!(to_keep.contains(&entries[2].oid));
        assert!(to_keep.contains(&entries[1].oid));
        assert_eq!(to_delete, vec![entries[0].oid]);
    }

    #[test]
    fn test_compute_sets_large_input_partitions_without_overlap() {
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let mut entries = Vec::new();

        for i in 0..5000u32 {
            entries.push(RetentionEntry {
                oid: oid_from_index(i),
                time: start + Duration::seconds(i as i64),
                is_retained: i % 997 == 0,
                is_old: i < 4000,
            });
        }

        let (to_delete, to_keep) = compute_sets_from_entries(&entries, Some(1500));

        assert!(to_keep.len() >= 1500);
        assert_eq!(to_delete.len() + to_keep.len(), entries.len());
        assert!(to_delete.windows(2).all(|w| w[0] <= w[1]));
        assert!(to_keep.windows(2).all(|w| w[0] <= w[1]));

        let keep_set: HashSet<Oid> = to_keep.iter().copied().collect();
        assert_eq!(keep_set.len(), to_keep.len());
        assert!(to_delete.iter().all(|oid| !keep_set.contains(oid)));
    }

    fn oid_from_index(index: u32) -> Oid {
        let mut bytes = [0u8; 20];
        bytes[0..4].copy_from_slice(&index.to_be_bytes());
        bytes[4..8].copy_from_slice(&(index.wrapping_mul(2654435761)).to_be_bytes());
        bytes[8..12].copy_from_slice(&(index.reverse_bits()).to_be_bytes());
        bytes[12..16].copy_from_slice(&(index.rotate_left(7)).to_be_bytes());
        bytes[16..20].copy_from_slice(&(index ^ 0xA5A5_A5A5).to_be_bytes());
        Oid::from_bytes(&bytes).expect("generated bytes should always produce a valid OID")
    }
}
