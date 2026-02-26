//! Append-only audit log for compliance tracking

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
extern crate libc;

/// Audit log directory
const AUDIT_DIR: &str = ".whogitit";
/// Audit log file name
const AUDIT_FILE: &str = "audit.jsonl";
/// Number of hex chars retained from SHA-256 for event hash chaining (128 bits).
const EVENT_HASH_HEX_LEN: usize = 32;

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
    /// Hash of the previous event (for tamper detection chain)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_hash: Option<String>,
    /// Hash of this event's content (for integrity verification)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_hash: Option<String>,
    /// User who performed the action
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Configuration field that changed (for config_change events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
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
        let event = self.with_chain(event)?;
        self.write_event(&event)
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

    /// Log a configuration change event
    pub fn log_config_change(&self, field: &str, reason: &str) -> Result<()> {
        self.log(AuditEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            event: AuditEventType::ConfigChange,
            details: AuditDetails {
                field: Some(field.to_string()),
                reason: Some(reason.to_string()),
                user: get_current_user(),
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
        for (idx, line) in reader.lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let event = serde_json::from_str::<AuditEvent>(&line)
                .with_context(|| format!("Failed to parse audit log entry at line {}", idx + 1))?;
            events.push(event);
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

    fn with_chain(&self, mut event: AuditEvent) -> Result<AuditEvent> {
        use sha2::{Digest, Sha256};

        // Get the hash of the last event (if any)
        let prev_hash = if self.path.exists() {
            self.get_last_event_hash()?
        } else {
            None
        };

        // Set the previous hash
        event.details.prev_hash = prev_hash;

        // Calculate hash of this event's content (excluding event_hash itself)
        let content_to_hash = self.hashable_event_content(&event)?;
        let mut hasher = Sha256::new();
        hasher.update(content_to_hash.as_bytes());
        let hash = format!("{:x}", hasher.finalize());
        event.details.event_hash = Some(truncate_event_hash(&hash));

        Ok(event)
    }

    fn hashable_event_content(&self, event: &AuditEvent) -> Result<String> {
        let mut value = serde_json::to_value(event)?;
        if let Some(map) = value.as_object_mut() {
            map.remove("event_hash");
        }
        Ok(serde_json::to_string(&value)?)
    }

    fn write_event(&self, event: &AuditEvent) -> Result<()> {
        self.ensure_dir()?;

        let is_new_file = !self.path.exists();

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .context("Failed to open audit log")?;

        // Set restrictive permissions (0600) on new audit log - contains sensitive data
        #[cfg(unix)]
        if is_new_file {
            let mut perms = fs::metadata(&self.path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&self.path, perms)
                .context("Failed to set permissions on audit log")?;
        }

        let json = serde_json::to_string(event)?;
        writeln!(file, "{}", json).context("Failed to write to audit log")?;

        // Ensure data is persisted to disk for audit integrity
        file.sync_all()
            .context("Failed to sync audit log to disk")?;

        Ok(())
    }

    /// Get the hash of the last event in the log
    fn get_last_event_hash(&self) -> Result<Option<String>> {
        let events = self.read_all()?;
        Ok(events.last().and_then(|e| e.details.event_hash.clone()))
    }

    /// Verify the integrity of the audit log chain
    ///
    /// Returns Ok(true) if the chain is valid, Ok(false) if tampered,
    /// or an error if the log cannot be read.
    pub fn verify_chain(&self) -> Result<bool> {
        let events = self.read_all()?;

        if events.is_empty() {
            return Ok(true);
        }

        // First event should have no prev_hash
        if events[0].details.prev_hash.is_some() {
            return Ok(false);
        }

        if let Some(stored_hash) = events[0].details.event_hash.as_ref() {
            let computed = self.compute_event_hash(&events[0])?;
            if stored_hash != &computed {
                return Ok(false);
            }
        }

        // Each subsequent event should reference the previous event's hash
        for i in 1..events.len() {
            let expected_prev = events[i - 1].details.event_hash.as_ref();
            let actual_prev = events[i].details.prev_hash.as_ref();

            // If the chain has hashes, prev_hash must match exactly
            if expected_prev != actual_prev && (expected_prev.is_some() || actual_prev.is_some()) {
                return Ok(false);
            }

            // If the event has a hash, ensure it matches recomputation
            if let Some(stored_hash) = events[i].details.event_hash.as_ref() {
                let computed = self.compute_event_hash(&events[i])?;
                if stored_hash != &computed {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    fn compute_event_hash(&self, event: &AuditEvent) -> Result<String> {
        use sha2::{Digest, Sha256};
        let content_to_hash = self.hashable_event_content(event)?;
        let mut hasher = Sha256::new();
        hasher.update(content_to_hash.as_bytes());
        let hash = format!("{:x}", hasher.finalize());
        Ok(truncate_event_hash(&hash))
    }
}

fn truncate_event_hash(full_hash_hex: &str) -> String {
    full_hash_hex[..EVENT_HASH_HEX_LEN.min(full_hash_hex.len())].to_string()
}

/// Get the current user name for audit trail
///
/// On Unix, validates against the actual system user ID to detect spoofing.
/// Falls back to USER/USERNAME environment variables on non-Unix or if validation fails.
fn get_current_user() -> Option<String> {
    let env_user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok();

    #[cfg(unix)]
    {
        // Get the actual username from the system based on UID
        let uid = unsafe { libc::getuid() };

        // Use getpwuid_r (re-entrant) instead of getpwuid to avoid static-buffer races.
        if let Some(system_user) = get_system_user_for_uid(uid) {
            // Check if env var matches system user
            if let Some(ref env_name) = env_user {
                if env_name != &system_user {
                    eprintln!(
                        "whogitit: Warning - USER env var '{}' does not match system user '{}', using system user",
                        env_name, system_user
                    );
                }
            }

            return Some(system_user);
        }
    }

    // Fallback to environment variable
    env_user
}

#[cfg(unix)]
fn get_system_user_for_uid(uid: libc::uid_t) -> Option<String> {
    let mut buf_size = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) };
    if buf_size <= 0 {
        buf_size = 1024;
    }

    for _ in 0..4 {
        let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        let mut buffer = vec![0u8; buf_size as usize];

        let ret = unsafe {
            libc::getpwuid_r(
                uid,
                &mut pwd,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };

        if ret == 0 && !result.is_null() && !pwd.pw_name.is_null() {
            return Some(
                unsafe { std::ffi::CStr::from_ptr(pwd.pw_name) }
                    .to_string_lossy()
                    .into_owned(),
            );
        }

        if ret != libc::ERANGE {
            return None;
        }

        buf_size *= 2;
    }

    None
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

        assert!(events[0].details.prev_hash.is_none());
        assert!(events[0].details.event_hash.is_some());
        assert_eq!(
            events[0].details.event_hash.as_ref().unwrap().len(),
            EVENT_HASH_HEX_LEN
        );
        assert_eq!(events[1].details.prev_hash, events[0].details.event_hash);
        assert_eq!(events[2].details.prev_hash, events[1].details.event_hash);
        assert!(log.verify_chain().unwrap());
    }

    #[test]
    fn test_audit_chain_detects_tamper() {
        let dir = TempDir::new().unwrap();
        let log = AuditLog::new(dir.path());

        log.log_delete("abc123", "GDPR request").unwrap();
        log.log_export("json", 42).unwrap();

        let path = log.path();
        let content = std::fs::read_to_string(path).unwrap();
        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        assert!(lines.len() >= 2);

        let mut value: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "prev_hash".to_string(),
                serde_json::Value::String("deadbeefdeadbeef".to_string()),
            );
        }
        lines[1] = serde_json::to_string(&value).unwrap();
        std::fs::write(path, format!("{}\n", lines.join("\n"))).unwrap();

        assert!(!log.verify_chain().unwrap());
    }

    #[test]
    fn test_audit_chain_fails_on_invalid_json() {
        let dir = TempDir::new().unwrap();
        let log = AuditLog::new(dir.path());

        log.log_delete("abc123", "GDPR request").unwrap();

        let path = log.path();
        let mut content = std::fs::read_to_string(path).unwrap();
        content.push_str("\nnot json\n");
        std::fs::write(path, content).unwrap();

        assert!(log.verify_chain().is_err());
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

    #[test]
    fn test_log_config_change() {
        let dir = TempDir::new().unwrap();
        let log = AuditLog::new(dir.path());

        log.log_config_change("git.remote.origin.fetch", "Configured notes fetch")
            .unwrap();

        let events = log.read_all().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, AuditEventType::ConfigChange);
        assert_eq!(
            events[0].details.field.as_deref(),
            Some("git.remote.origin.fetch")
        );
        assert_eq!(
            events[0].details.reason.as_deref(),
            Some("Configured notes fetch")
        );
    }

    #[test]
    fn test_hashable_event_content_excludes_event_hash() {
        let dir = TempDir::new().unwrap();
        let log = AuditLog::new(dir.path());

        let mut event = AuditEvent {
            timestamp: "2026-01-31T12:00:00Z".to_string(),
            event: AuditEventType::Delete,
            details: AuditDetails {
                commit: Some("abc123".to_string()),
                reason: Some("test".to_string()),
                event_hash: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()),
                ..Default::default()
            },
        };

        let hashable_a = log.hashable_event_content(&event).unwrap();
        event.details.event_hash = Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string());
        let hashable_b = log.hashable_event_content(&event).unwrap();

        assert_eq!(hashable_a, hashable_b);
    }
}
