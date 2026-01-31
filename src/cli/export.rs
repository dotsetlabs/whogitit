//! Export command for bulk attribution data export

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::io::Write;

use crate::core::attribution::AIAttribution;
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
    let repo = git2::Repository::discover(".").context("Not in a git repository")?;
    let notes_store = NotesStore::new(&repo)?;

    // Parse date filters
    let since = parse_date(&args.since)?;
    let until = parse_date(&args.until)?;

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
        _ => anyhow::bail!("Unsupported format: {}", args.format),
    }

    Ok(())
}

fn parse_date(date_str: &Option<String>) -> Result<Option<DateTime<Utc>>> {
    match date_str {
        Some(s) => {
            // Parse YYYY-MM-DD format
            let date =
                chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").context("Invalid date format")?;
            Ok(Some(date.and_hms_opt(0, 0, 0).unwrap().and_utc()))
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
