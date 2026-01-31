//! Append-only audit log for compliance tracking

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// Audit log directory
const AUDIT_DIR: &str = ".whogitit";
/// Audit log file name
const AUDIT_FILE: &str = "audit.jsonl";

/// An audit log event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// ISO8601 timestamp
    pub timestamp: String,
    /// Event type
    pub event: AuditEventType,
    /// Additional details
    #[serde(flatten)]
    pub details: AuditDetails,
}

/// Types of audit events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    /// Attribution data was deleted
    Delete,
    /// Attribution data was exported
    Export,
    /// Retention policy was applied
    RetentionApply,
    /// Configuration was changed
    ConfigChange,
    /// Redaction occurred (when audit logging enabled)
    Redaction,
}

impl std::fmt::Display for AuditEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Delete => write!(f, "delete"),
            Self::Export => write!(f, "export"),
            Self::RetentionApply => write!(f, "retention_apply"),
            Self::ConfigChange => write!(f, "config_change"),
            Self::Redaction => write!(f, "redaction"),
        }
    }
}

/// Additional details for an audit event
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuditDetails {
    /// Commit ID (for delete events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Reason for the action
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Export format (for export events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Number of commits affected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_count: Option<u32>,
    /// Pattern name (for redaction events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern_name: Option<String>,
    /// Redaction count (for redaction events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redaction_count: Option<u32>,
    /// User who performed the action
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

/// Append-only audit log store
pub struct AuditLog {
    path: PathBuf,
}

impl AuditLog {
    /// Create a new audit log for the given repo root
    pub fn new(repo_root: &Path) -> Self {
        let path = repo_root.join(AUDIT_DIR).join(AUDIT_FILE);
        Self { path }
    }

    /// Ensure the audit log directory exists
    fn ensure_dir(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create audit directory")?;
        }
        Ok(())
    }

    /// Append an event to the audit log
    pub fn log(&self, event: AuditEvent) -> Result<()> {
        self.ensure_dir()?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .context("Failed to open audit log")?;

        let json = serde_json::to_string(&event)?;
        writeln!(file, "{}", json).context("Failed to write to audit log")?;

        Ok(())
    }

    /// Log a delete event
    pub fn log_delete(&self, commit: &str, reason: &str) -> Result<()> {
        self.log(AuditEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            event: AuditEventType::Delete,
            details: AuditDetails {
                commit: Some(commit.to_string()),
                reason: Some(reason.to_string()),
                user: get_current_user(),
                ..Default::default()
            },
        })
    }

    /// Log an export event
    pub fn log_export(&self, format: &str, commit_count: u32) -> Result<()> {
        self.log(AuditEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            event: AuditEventType::Export,
            details: AuditDetails {
                format: Some(format.to_string()),
                commit_count: Some(commit_count),
                user: get_current_user(),
                ..Default::default()
            },
        })
    }

    /// Log a retention policy application
    pub fn log_retention(&self, commit_count: u32, reason: &str) -> Result<()> {
        self.log(AuditEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            event: AuditEventType::RetentionApply,
            details: AuditDetails {
                commit_count: Some(commit_count),
                reason: Some(reason.to_string()),
                user: get_current_user(),
                ..Default::default()
            },
        })
    }

    /// Log a redaction event
    pub fn log_redaction(&self, pattern_name: &str, redaction_count: u32) -> Result<()> {
        self.log(AuditEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            event: AuditEventType::Redaction,
            details: AuditDetails {
                pattern_name: Some(pattern_name.to_string()),
                redaction_count: Some(redaction_count),
                ..Default::default()
            },
        })
    }

    /// Read all events from the audit log
    pub fn read_all(&self) -> Result<Vec<AuditEvent>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&self.path).context("Failed to open audit log")?;
        let reader = BufReader::new(file);

        let mut events = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(event) = serde_json::from_str::<AuditEvent>(&line) {
                events.push(event);
            }
        }

        Ok(events)
    }

    /// Read events filtered by date range
    pub fn read_since(&self, since: chrono::DateTime<chrono::Utc>) -> Result<Vec<AuditEvent>> {
        let all = self.read_all()?;
        Ok(all
            .into_iter()
            .filter(|e| {
                chrono::DateTime::parse_from_rfc3339(&e.timestamp)
                    .map(|t| t >= since)
                    .unwrap_or(false)
            })
            .collect())
    }

    /// Check if audit log exists
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Get the path to the audit log
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Get the current user name for audit trail
fn get_current_user() -> Option<String> {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_audit_log_roundtrip() {
        let dir = TempDir::new().unwrap();
        let log = AuditLog::new(dir.path());

        log.log_delete("abc123", "GDPR request").unwrap();
        log.log_export("json", 42).unwrap();
        log.log_retention(10, "Retention policy").unwrap();

        let events = log.read_all().unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].event, AuditEventType::Delete);
        assert_eq!(events[1].event, AuditEventType::Export);
        assert_eq!(events[2].event, AuditEventType::RetentionApply);
    }

    #[test]
    fn test_audit_log_empty() {
        let dir = TempDir::new().unwrap();
        let log = AuditLog::new(dir.path());

        let events = log.read_all().unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_audit_event_serialization() {
        let event = AuditEvent {
            timestamp: "2026-01-31T12:00:00Z".to_string(),
            event: AuditEventType::Delete,
            details: AuditDetails {
                commit: Some("abc123".to_string()),
                reason: Some("test".to_string()),
                ..Default::default()
            },
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: AuditEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.event, AuditEventType::Delete);
        assert_eq!(parsed.details.commit, Some("abc123".to_string()));
    }
}
