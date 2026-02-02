//! Export command for bulk attribution data export

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::io::Write;

use crate::core::attribution::AIAttribution;
use crate::privacy::WhogititConfig;
use crate::storage::audit::AuditLog;
use crate::storage::notes::NotesStore;

/// Arguments for export command
#[derive(Debug, clap::Args)]
pub struct ExportArgs {
    /// Output format (json or csv)
    #[arg(long, value_parser = ["json", "csv"], default_value = "json")]
    pub format: String,

    /// Only include commits after this date (YYYY-MM-DD)
    #[arg(long)]
    pub since: Option<String>,

    /// Only include commits before this date (YYYY-MM-DD)
    #[arg(long)]
    pub until: Option<String>,

    /// Output file (default: stdout)
    #[arg(short, long)]
    pub output: Option<String>,

    /// Include full prompt text (default: truncated)
    #[arg(long)]
    pub full_prompts: bool,

    /// Maximum prompt length when not using --full-prompts
    #[arg(long, default_value = "100")]
    pub prompt_max_len: usize,
}

/// Export format for JSON output
#[derive(Debug, Serialize)]
pub struct ExportData {
    /// Export schema version
    pub export_version: u8,
    /// When the export was created
    pub exported_at: String,
    /// Date range filter (if specified)
    pub date_range: Option<DateRange>,
    /// Exported commits
    pub commits: Vec<CommitExport>,
    /// Summary statistics
    pub summary: ExportSummary,
}

/// Date range filter
#[derive(Debug, Serialize)]
pub struct DateRange {
    pub since: Option<String>,
    pub until: Option<String>,
}

/// Exported commit data
#[derive(Debug, Serialize)]
pub struct CommitExport {
    /// Git commit SHA
    pub commit_id: String,
    /// Short commit SHA (7 chars)
    pub commit_short: String,
    /// Commit message (first line)
    pub message: String,
    /// Commit author
    pub author: String,
    /// Commit timestamp
    pub committed_at: String,
    /// AI session ID
    pub session_id: String,
    /// Model used
    pub model: String,
    /// Total AI-generated lines
    pub ai_lines: usize,
    /// AI lines modified by human
    pub ai_modified_lines: usize,
    /// Human-written lines
    pub human_lines: usize,
    /// Original lines (unchanged)
    pub original_lines: usize,
    /// Files affected
    pub files: Vec<String>,
    /// Prompts used
    pub prompts: Vec<PromptExport>,
}

/// Exported prompt data
#[derive(Debug, Serialize)]
pub struct PromptExport {
    pub index: u32,
    pub text: String,
    pub affected_files: Vec<String>,
}

/// Export summary statistics
#[derive(Debug, Serialize)]
pub struct ExportSummary {
    pub total_commits: usize,
    pub commits_with_ai: usize,
    pub total_ai_lines: usize,
    pub total_ai_modified_lines: usize,
    pub total_human_lines: usize,
    pub total_original_lines: usize,
    pub total_prompts: usize,
}

/// Run the export command
pub fn run(args: ExportArgs) -> Result<()> {
    let repo = git2::Repository::discover(".").context(
        "Not in a git repository. \
         Run 'git init' to create one, or 'cd' to a directory containing a .git folder.",
    )?;
    let repo_root = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("No working directory"))?;
    let notes_store = NotesStore::new(&repo)?;

    // Parse date filters
    let since = parse_date(&args.since)?;
    let until = parse_date(&args.until)?;

    // Validate date range
    if let (Some(ref since_date), Some(ref until_date)) = (&since, &until) {
        if since_date > until_date {
            anyhow::bail!(
                "Invalid date range: --since ({}) must be before --until ({})",
                args.since.as_ref().unwrap(),
                args.until.as_ref().unwrap()
            );
        }
    }

    // Get all commits with attribution
    let attributed_commits = notes_store.list_attributed_commits()?;

    // Collect export data
    let mut commits: Vec<CommitExport> = Vec::new();

    for commit_oid in attributed_commits {
        let commit = repo.find_commit(commit_oid)?;
        let commit_time =
            DateTime::from_timestamp(commit.time().seconds(), 0).unwrap_or(DateTime::UNIX_EPOCH);

        // Apply date filters
        if let Some(ref since_date) = since {
            if commit_time < *since_date {
                continue;
            }
        }
        if let Some(ref until_date) = until {
            if commit_time > *until_date {
                continue;
            }
        }

        // Get attribution data
        if let Some(attribution) = notes_store.fetch_attribution(commit_oid)? {
            let export = build_commit_export(&commit, &attribution, &args)?;
            commits.push(export);
        }
    }

    // Sort by commit time (newest first)
    commits.sort_by(|a, b| b.committed_at.cmp(&a.committed_at));

    // Build summary
    let summary = build_summary(&commits);

    // Write output
    let output_data = ExportData {
        export_version: 1,
        exported_at: Utc::now().to_rfc3339(),
        date_range: if args.since.is_some() || args.until.is_some() {
            Some(DateRange {
                since: args.since.clone(),
                until: args.until.clone(),
            })
        } else {
            None
        },
        commits,
        summary,
    };

    match args.format.as_str() {
        "json" => write_json(&output_data, &args.output)?,
        "csv" => write_csv(&output_data, &args.output)?,
        other => anyhow::bail!(
            "Unsupported format: '{}'. Supported formats: json, csv",
            other
        ),
    }

    let config = WhogititConfig::load(repo_root).unwrap_or_default();
    if config.privacy.audit_log {
        let audit_log = AuditLog::new(repo_root);
        audit_log.log_export(&args.format, output_data.summary.total_commits as u32)?;
    }

    Ok(())
}

fn parse_date(date_str: &Option<String>) -> Result<Option<DateTime<Utc>>> {
    match date_str {
        Some(s) => {
            // Parse YYYY-MM-DD format
            let date = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .with_context(|| format!("Invalid date format '{}'. Use YYYY-MM-DD.", s))?;
            let datetime = date
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| anyhow::anyhow!("Invalid time for date {}", s))?;
            Ok(Some(datetime.and_utc()))
        }
        None => Ok(None),
    }
}

fn build_commit_export(
    commit: &git2::Commit,
    attribution: &AIAttribution,
    args: &ExportArgs,
) -> Result<CommitExport> {
    let commit_id = commit.id().to_string();
    let commit_short = commit_id[..7].to_string();
    let message = commit.summary().unwrap_or("(no message)").to_string();
    let author = commit.author().name().unwrap_or("Unknown").to_string();
    let committed_at = DateTime::from_timestamp(commit.time().seconds(), 0)
        .unwrap_or(DateTime::UNIX_EPOCH)
        .to_rfc3339();

    let ai_lines = attribution.total_ai_lines();
    let ai_modified_lines = attribution.total_ai_modified_lines();
    let human_lines = attribution.total_human_lines();
    let original_lines = attribution.total_original_lines();

    let files: Vec<String> = attribution.files.iter().map(|f| f.path.clone()).collect();

    let prompts: Vec<PromptExport> = attribution
        .prompts
        .iter()
        .map(|p| {
            let text = if args.full_prompts {
                p.text.clone()
            } else if p.text.len() > args.prompt_max_len {
                format!("{}...", &p.text[..args.prompt_max_len])
            } else {
                p.text.clone()
            };
            PromptExport {
                index: p.index,
                text,
                affected_files: p.affected_files.clone(),
            }
        })
        .collect();

    Ok(CommitExport {
        commit_id,
        commit_short,
        message,
        author,
        committed_at,
        session_id: attribution.session.session_id.clone(),
        model: attribution.session.model.id.clone(),
        ai_lines,
        ai_modified_lines,
        human_lines,
        original_lines,
        files,
        prompts,
    })
}

fn build_summary(commits: &[CommitExport]) -> ExportSummary {
    let commits_with_ai = commits.iter().filter(|c| c.ai_lines > 0).count();
    let total_ai_lines: usize = commits.iter().map(|c| c.ai_lines).sum();
    let total_ai_modified_lines: usize = commits.iter().map(|c| c.ai_modified_lines).sum();
    let total_human_lines: usize = commits.iter().map(|c| c.human_lines).sum();
    let total_original_lines: usize = commits.iter().map(|c| c.original_lines).sum();
    let total_prompts: usize = commits.iter().map(|c| c.prompts.len()).sum();

    ExportSummary {
        total_commits: commits.len(),
        commits_with_ai,
        total_ai_lines,
        total_ai_modified_lines,
        total_human_lines,
        total_original_lines,
        total_prompts,
    }
}

fn write_json(data: &ExportData, output: &Option<String>) -> Result<()> {
    let json = serde_json::to_string_pretty(data)?;

    match output {
        Some(path) => {
            let mut file = std::fs::File::create(path)?;
            file.write_all(json.as_bytes())?;
            eprintln!(
                "Exported {} commits to {}",
                data.summary.total_commits, path
            );
        }
        None => {
            println!("{}", json);
        }
    }

    Ok(())
}

fn write_csv(data: &ExportData, output: &Option<String>) -> Result<()> {
    let mut csv_content = String::new();

    // Header
    csv_content.push_str(
        "commit_id,commit_short,message,author,committed_at,session_id,model,ai_lines,ai_modified_lines,human_lines,original_lines,files_count,prompts_count\n",
    );

    // Rows
    for commit in &data.commits {
        let message = commit.message.replace(',', ";").replace('\n', " ");
        csv_content.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            commit.commit_id,
            commit.commit_short,
            message,
            commit.author.replace(',', ";"),
            commit.committed_at,
            commit.session_id,
            commit.model,
            commit.ai_lines,
            commit.ai_modified_lines,
            commit.human_lines,
            commit.original_lines,
            commit.files.len(),
            commit.prompts.len()
        ));
    }

    match output {
        Some(path) => {
            let mut file = std::fs::File::create(path)?;
            file.write_all(csv_content.as_bytes())?;
            eprintln!(
                "Exported {} commits to {}",
                data.summary.total_commits, path
            );
        }
        None => {
            print!("{}", csv_content);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // parse_date tests

    #[test]
    fn test_parse_date_valid() {
        let result = parse_date(&Some("2024-01-15".to_string())).unwrap();
        assert!(result.is_some());
        let date = result.unwrap();
        assert_eq!(date.format("%Y-%m-%d").to_string(), "2024-01-15");
    }

    #[test]
    fn test_parse_date_none() {
        let result = parse_date(&None).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_date_invalid_format() {
        // Wrong separator
        let result = parse_date(&Some("2024/01/15".to_string()));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid date format"));
    }

    #[test]
    fn test_parse_date_invalid_date() {
        // February 30th doesn't exist
        let result = parse_date(&Some("2024-02-30".to_string()));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_date_partial_format() {
        // Missing day
        let result = parse_date(&Some("2024-01".to_string()));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_date_leap_year() {
        // February 29th on leap year
        let result = parse_date(&Some("2024-02-29".to_string())).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_parse_date_non_leap_year() {
        // February 29th on non-leap year
        let result = parse_date(&Some("2023-02-29".to_string()));
        assert!(result.is_err());
    }

    // build_summary tests

    #[test]
    fn test_build_summary_empty() {
        let commits: Vec<CommitExport> = vec![];
        let summary = build_summary(&commits);
        assert_eq!(summary.total_commits, 0);
        assert_eq!(summary.commits_with_ai, 0);
        assert_eq!(summary.total_ai_lines, 0);
        assert_eq!(summary.total_prompts, 0);
    }

    #[test]
    fn test_build_summary_single_commit() {
        let commits = vec![CommitExport {
            commit_id: "abc123".to_string(),
            commit_short: "abc1234".to_string(),
            message: "Test commit".to_string(),
            author: "Test Author".to_string(),
            committed_at: "2024-01-15T00:00:00+00:00".to_string(),
            session_id: "session-123".to_string(),
            model: "claude-opus-4-5-20251101".to_string(),
            ai_lines: 50,
            ai_modified_lines: 10,
            human_lines: 20,
            original_lines: 100,
            files: vec!["src/main.rs".to_string()],
            prompts: vec![PromptExport {
                index: 0,
                text: "Test prompt".to_string(),
                affected_files: vec!["src/main.rs".to_string()],
            }],
        }];
        let summary = build_summary(&commits);
        assert_eq!(summary.total_commits, 1);
        assert_eq!(summary.commits_with_ai, 1);
        assert_eq!(summary.total_ai_lines, 50);
        assert_eq!(summary.total_ai_modified_lines, 10);
        assert_eq!(summary.total_human_lines, 20);
        assert_eq!(summary.total_original_lines, 100);
        assert_eq!(summary.total_prompts, 1);
    }

    #[test]
    fn test_build_summary_multiple_commits() {
        let commits = vec![
            CommitExport {
                commit_id: "abc123".to_string(),
                commit_short: "abc1234".to_string(),
                message: "First".to_string(),
                author: "Author".to_string(),
                committed_at: "2024-01-15T00:00:00+00:00".to_string(),
                session_id: "session-1".to_string(),
                model: "claude-opus-4-5-20251101".to_string(),
                ai_lines: 50,
                ai_modified_lines: 10,
                human_lines: 5,
                original_lines: 100,
                files: vec!["file1.rs".to_string()],
                prompts: vec![
                    PromptExport {
                        index: 0,
                        text: "Prompt 1".to_string(),
                        affected_files: vec![],
                    },
                    PromptExport {
                        index: 1,
                        text: "Prompt 2".to_string(),
                        affected_files: vec![],
                    },
                ],
            },
            CommitExport {
                commit_id: "def456".to_string(),
                commit_short: "def4567".to_string(),
                message: "Second".to_string(),
                author: "Author".to_string(),
                committed_at: "2024-01-16T00:00:00+00:00".to_string(),
                session_id: "session-2".to_string(),
                model: "claude-opus-4-5-20251101".to_string(),
                ai_lines: 30,
                ai_modified_lines: 5,
                human_lines: 10,
                original_lines: 50,
                files: vec!["file2.rs".to_string()],
                prompts: vec![PromptExport {
                    index: 0,
                    text: "Prompt 3".to_string(),
                    affected_files: vec![],
                }],
            },
        ];
        let summary = build_summary(&commits);
        assert_eq!(summary.total_commits, 2);
        assert_eq!(summary.commits_with_ai, 2);
        assert_eq!(summary.total_ai_lines, 80); // 50 + 30
        assert_eq!(summary.total_ai_modified_lines, 15); // 10 + 5
        assert_eq!(summary.total_human_lines, 15); // 5 + 10
        assert_eq!(summary.total_original_lines, 150); // 100 + 50
        assert_eq!(summary.total_prompts, 3); // 2 + 1
    }

    #[test]
    fn test_build_summary_no_ai_lines() {
        let commits = vec![CommitExport {
            commit_id: "abc123".to_string(),
            commit_short: "abc1234".to_string(),
            message: "Human only".to_string(),
            author: "Author".to_string(),
            committed_at: "2024-01-15T00:00:00+00:00".to_string(),
            session_id: "session-123".to_string(),
            model: "claude-opus-4-5-20251101".to_string(),
            ai_lines: 0,
            ai_modified_lines: 0,
            human_lines: 100,
            original_lines: 200,
            files: vec!["file.rs".to_string()],
            prompts: vec![],
        }];
        let summary = build_summary(&commits);
        assert_eq!(summary.total_commits, 1);
        assert_eq!(summary.commits_with_ai, 0); // No AI lines
        assert_eq!(summary.total_ai_lines, 0);
        assert_eq!(summary.total_human_lines, 100);
    }

    // CSV escaping tests

    #[test]
    fn test_csv_message_escaping() {
        // The message escaping happens in write_csv inline
        // We can test the replacement logic directly
        let message = "Fix bug, add feature\nSecond line";
        let escaped = message.replace(',', ";").replace('\n', " ");
        assert_eq!(escaped, "Fix bug; add feature Second line");
        assert!(!escaped.contains(','));
        assert!(!escaped.contains('\n'));
    }

    #[test]
    fn test_csv_author_escaping() {
        let author = "John Doe, Jr.";
        let escaped = author.replace(',', ";");
        assert_eq!(escaped, "John Doe; Jr.");
    }

    // ExportData structure tests

    #[test]
    fn test_export_data_serialization() {
        let data = ExportData {
            export_version: 1,
            exported_at: "2024-01-15T12:00:00Z".to_string(),
            date_range: Some(DateRange {
                since: Some("2024-01-01".to_string()),
                until: None,
            }),
            commits: vec![],
            summary: ExportSummary {
                total_commits: 0,
                commits_with_ai: 0,
                total_ai_lines: 0,
                total_ai_modified_lines: 0,
                total_human_lines: 0,
                total_original_lines: 0,
                total_prompts: 0,
            },
        };

        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("\"export_version\":1"));
        assert!(json.contains("\"since\":\"2024-01-01\""));
        assert!(json.contains("\"until\":null"));
    }

    #[test]
    fn test_export_data_no_date_range() {
        let data = ExportData {
            export_version: 1,
            exported_at: "2024-01-15T12:00:00Z".to_string(),
            date_range: None,
            commits: vec![],
            summary: ExportSummary {
                total_commits: 0,
                commits_with_ai: 0,
                total_ai_lines: 0,
                total_ai_modified_lines: 0,
                total_human_lines: 0,
                total_original_lines: 0,
                total_prompts: 0,
            },
        };

        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("\"date_range\":null"));
    }

    #[test]
    fn test_commit_export_serialization() {
        let commit = CommitExport {
            commit_id: "abc123def456".to_string(),
            commit_short: "abc123d".to_string(),
            message: "Test commit".to_string(),
            author: "Test Author".to_string(),
            committed_at: "2024-01-15T00:00:00+00:00".to_string(),
            session_id: "session-123".to_string(),
            model: "claude-opus-4-5-20251101".to_string(),
            ai_lines: 42,
            ai_modified_lines: 8,
            human_lines: 10,
            original_lines: 100,
            files: vec!["src/main.rs".to_string(), "src/lib.rs".to_string()],
            prompts: vec![],
        };

        let json = serde_json::to_string(&commit).unwrap();
        assert!(json.contains("\"commit_short\":\"abc123d\""));
        assert!(json.contains("\"ai_lines\":42"));
        assert!(json.contains("\"model\":\"claude-opus-4-5-20251101\""));
    }
}
