//! Audit log viewing command

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use colored::Colorize;

use crate::storage::audit::{AuditEventType, AuditLog};

/// Arguments for audit command
#[derive(Debug, clap::Args)]
pub struct AuditArgs {
    /// Only show events after this date (YYYY-MM-DD)
    #[arg(long)]
    pub since: Option<String>,

    /// Filter by event type
    #[arg(long, value_parser = ["delete", "export", "retention_apply", "config_change", "redaction"])]
    pub event_type: Option<String>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Show last N events
    #[arg(long, default_value = "50")]
    pub limit: usize,
}

/// Run the audit command
pub fn run(args: AuditArgs) -> Result<()> {
    let repo = git2::Repository::discover(".").context("Not in a git repository")?;
    let repo_root = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("No working directory"))?;

    let audit_log = AuditLog::new(repo_root);

    if !audit_log.exists() {
        if args.json {
            println!("[]");
        } else {
            println!("No audit log found.");
            println!(
                "Enable audit logging in .whogitit.toml: {}",
                "[privacy]\naudit_log = true".dimmed()
            );
        }
        return Ok(());
    }

    // Read events
    let mut events = if let Some(since_str) = &args.since {
        let since_date = chrono::NaiveDate::parse_from_str(since_str, "%Y-%m-%d")
            .context("Invalid date format. Use YYYY-MM-DD.")?;
        let since = since_date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| anyhow::anyhow!("Invalid time for date {}", since_str))?
            .and_utc();
        audit_log.read_since(since)?
    } else {
        audit_log.read_all()?
    };

    // Filter by event type
    if let Some(event_type_str) = &args.event_type {
        let event_type = match event_type_str.as_str() {
            "delete" => AuditEventType::Delete,
            "export" => AuditEventType::Export,
            "retention_apply" => AuditEventType::RetentionApply,
            "config_change" => AuditEventType::ConfigChange,
            "redaction" => AuditEventType::Redaction,
            _ => anyhow::bail!("Unknown event type: {}", event_type_str),
        };
        events.retain(|e| e.event == event_type);
    }

    // Sort by timestamp (newest first)
    events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    // Limit
    events.truncate(args.limit);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&events)?);
    } else {
        print_events(&events)?;
    }

    Ok(())
}

fn print_events(events: &[crate::storage::audit::AuditEvent]) -> Result<()> {
    if events.is_empty() {
        println!("No audit events found.");
        return Ok(());
    }

    println!("{}", "Audit Log".bold());
    println!("{}", "=".repeat(60));

    for event in events {
        let timestamp = DateTime::parse_from_rfc3339(&event.timestamp)
            .map(|t| {
                t.with_timezone(&Utc)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_else(|_| event.timestamp.clone());

        let event_color = match event.event {
            AuditEventType::Delete => "delete".red(),
            AuditEventType::Export => "export".blue(),
            AuditEventType::RetentionApply => "retention".yellow(),
            AuditEventType::ConfigChange => "config".cyan(),
            AuditEventType::Redaction => "redaction".magenta(),
        };

        print!("{} {} ", timestamp.dimmed(), event_color);

        // Print details
        let details = &event.details;
        let mut detail_parts: Vec<String> = Vec::new();

        if let Some(commit) = &details.commit {
            detail_parts.push(format!("commit:{}", &commit[..7.min(commit.len())]));
        }
        if let Some(count) = details.commit_count {
            detail_parts.push(format!("commits:{}", count));
        }
        if let Some(format) = &details.format {
            detail_parts.push(format!("format:{}", format));
        }
        if let Some(pattern) = &details.pattern_name {
            detail_parts.push(format!("pattern:{}", pattern));
        }
        if let Some(count) = details.redaction_count {
            detail_parts.push(format!("redactions:{}", count));
        }
        if let Some(user) = &details.user {
            detail_parts.push(format!("user:{}", user));
        }

        if !detail_parts.is_empty() {
            print!("{}", detail_parts.join(" ").dimmed());
        }

        if let Some(reason) = &details.reason {
            print!(" - {}", reason);
        }

        println!();
    }

    Ok(())
}

/// Parse event type string to AuditEventType
#[allow(dead_code)]
fn parse_event_type(s: &str) -> Option<AuditEventType> {
    match s {
        "delete" => Some(AuditEventType::Delete),
        "export" => Some(AuditEventType::Export),
        "retention_apply" => Some(AuditEventType::RetentionApply),
        "config_change" => Some(AuditEventType::ConfigChange),
        "redaction" => Some(AuditEventType::Redaction),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::audit::{AuditDetails, AuditEvent};

    // AuditArgs tests

    #[test]
    fn test_audit_args_defaults() {
        let args = AuditArgs {
            since: None,
            event_type: None,
            json: false,
            limit: 50,
        };
        assert!(args.since.is_none());
        assert!(args.event_type.is_none());
        assert!(!args.json);
        assert_eq!(args.limit, 50);
    }

    #[test]
    fn test_audit_args_with_filters() {
        let args = AuditArgs {
            since: Some("2024-01-01".to_string()),
            event_type: Some("delete".to_string()),
            json: true,
            limit: 100,
        };
        assert_eq!(args.since, Some("2024-01-01".to_string()));
        assert_eq!(args.event_type, Some("delete".to_string()));
        assert!(args.json);
        assert_eq!(args.limit, 100);
    }

    // Event type parsing tests

    #[test]
    fn test_parse_event_type_delete() {
        assert!(matches!(
            parse_event_type("delete"),
            Some(AuditEventType::Delete)
        ));
    }

    #[test]
    fn test_parse_event_type_export() {
        assert!(matches!(
            parse_event_type("export"),
            Some(AuditEventType::Export)
        ));
    }

    #[test]
    fn test_parse_event_type_retention_apply() {
        assert!(matches!(
            parse_event_type("retention_apply"),
            Some(AuditEventType::RetentionApply)
        ));
    }

    #[test]
    fn test_parse_event_type_config_change() {
        assert!(matches!(
            parse_event_type("config_change"),
            Some(AuditEventType::ConfigChange)
        ));
    }

    #[test]
    fn test_parse_event_type_redaction() {
        assert!(matches!(
            parse_event_type("redaction"),
            Some(AuditEventType::Redaction)
        ));
    }

    #[test]
    fn test_parse_event_type_invalid() {
        assert!(parse_event_type("invalid").is_none());
        assert!(parse_event_type("").is_none());
        assert!(parse_event_type("Delete").is_none()); // case sensitive
    }

    // Event filtering test

    #[test]
    fn test_event_filtering() {
        let events = vec![
            create_test_event(AuditEventType::Delete),
            create_test_event(AuditEventType::Export),
            create_test_event(AuditEventType::Delete),
            create_test_event(AuditEventType::ConfigChange),
        ];

        let filtered: Vec<_> = events
            .into_iter()
            .filter(|e| e.event == AuditEventType::Delete)
            .collect();

        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|e| e.event == AuditEventType::Delete));
    }

    #[test]
    fn test_event_sorting_by_timestamp() {
        #[allow(clippy::useless_vec)]
        let mut events = vec![
            create_test_event_with_time("2024-01-01T10:00:00Z"),
            create_test_event_with_time("2024-01-03T10:00:00Z"),
            create_test_event_with_time("2024-01-02T10:00:00Z"),
        ];

        // Sort newest first
        events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        assert_eq!(events[0].timestamp, "2024-01-03T10:00:00Z");
        assert_eq!(events[1].timestamp, "2024-01-02T10:00:00Z");
        assert_eq!(events[2].timestamp, "2024-01-01T10:00:00Z");
    }

    #[test]
    fn test_event_truncation() {
        let mut events: Vec<i32> = (0..100).collect();
        let limit = 50;
        events.truncate(limit);
        assert_eq!(events.len(), 50);
    }

    // Helper functions

    fn create_test_event(event_type: AuditEventType) -> AuditEvent {
        AuditEvent {
            timestamp: "2024-01-15T12:00:00Z".to_string(),
            event: event_type,
            details: AuditDetails::default(),
        }
    }

    fn create_test_event_with_time(timestamp: &str) -> AuditEvent {
        AuditEvent {
            timestamp: timestamp.to_string(),
            event: AuditEventType::Delete,
            details: AuditDetails::default(),
        }
    }

    // Date parsing test (shared logic with other modules)
    #[test]
    fn test_date_parsing() {
        let date_str = "2024-01-15";
        let parsed = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d");
        assert!(parsed.is_ok());
        assert_eq!(parsed.unwrap().to_string(), "2024-01-15");
    }

    #[test]
    fn test_invalid_date_parsing() {
        let date_str = "2024/01/15";
        let parsed = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d");
        assert!(parsed.is_err());
    }
}
