//! Privacy configuration for whogitit
//!
//! Supports loading from `.whogitit.toml` (repo) or `~/.config/whogitit/config.toml` (global).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::redaction::{patterns, Redactor};
use regex;

/// Privacy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PrivacyConfig {
    /// Whether redaction is enabled
    pub enabled: bool,

    /// Whether to use builtin patterns
    pub use_builtin_patterns: bool,

    /// Custom patterns to add
    #[serde(default)]
    pub custom_patterns: Vec<PatternConfig>,

    /// Builtin patterns to disable (by name)
    #[serde(default)]
    pub disabled_patterns: Vec<String>,

    /// Whether to log redaction events for audit
    pub audit_log: bool,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            use_builtin_patterns: true,
            custom_patterns: Vec::new(),
            disabled_patterns: Vec::new(),
            audit_log: false,
        }
    }
}

/// Custom pattern configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternConfig {
    /// Pattern name (for audit trail)
    pub name: String,

    /// Regex pattern
    pub pattern: String,

    /// Optional description
    #[serde(default)]
    pub description: Option<String>,
}

/// Full whogitit configuration file
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct WhogititConfig {
    /// Privacy settings
    pub privacy: PrivacyConfig,

    /// Retention settings (for Phase 3)
    #[serde(default)]
    pub retention: Option<RetentionConfig>,

    /// Analysis settings
    #[serde(default)]
    pub analysis: AnalysisConfig,
}

/// Analysis configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalysisConfig {
    /// Maximum age in hours before a pending buffer is considered stale
    /// Default: 24 hours
    pub max_pending_age_hours: u32,

    /// Similarity threshold (0.0-1.0) for detecting AIModified lines
    /// Lower values mean more aggressive matching, higher values require more similarity
    /// Default: 0.6
    pub similarity_threshold: f64,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            max_pending_age_hours: 24,
            similarity_threshold: 0.6,
        }
    }
}

/// Data retention configuration (Phase 3)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RetentionConfig {
    /// Maximum age of attribution data in days
    pub max_age_days: Option<u32>,

    /// Whether to auto-purge old data
    pub auto_purge: bool,

    /// Refs to always retain (e.g., ["refs/heads/main"])
    #[serde(default)]
    pub retain_refs: Vec<String>,

    /// Minimum commits to keep regardless of age
    pub min_commits: Option<u32>,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            max_age_days: None,
            auto_purge: false,
            retain_refs: vec!["refs/heads/main".to_string()],
            min_commits: Some(100),
        }
    }
}

impl WhogititConfig {
    /// Load configuration from repo root, falling back to global config
    pub fn load(repo_root: &Path) -> Result<Self> {
        // Try repo-local config first
        let repo_config = repo_root.join(".whogitit.toml");
        if repo_config.exists() {
            return Self::load_from_file(&repo_config);
        }

        // Try global config
        if let Some(global_config) = Self::global_config_path() {
            if global_config.exists() {
                return Self::load_from_file(&global_config);
            }
        }

        // Return defaults
        Ok(Self::default())
    }

    /// Load configuration from a specific file
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))
    }

    /// Get global config path (~/.config/whogitit/config.toml)
    pub fn global_config_path() -> Option<PathBuf> {
        dirs_path().map(|p| p.join("config.toml"))
    }

    /// Get repo-local config path
    pub fn repo_config_path(repo_root: &Path) -> PathBuf {
        repo_root.join(".whogitit.toml")
    }

    /// Check if a config file exists for this repo
    pub fn exists_for_repo(repo_root: &Path) -> bool {
        Self::repo_config_path(repo_root).exists()
    }
}

impl PrivacyConfig {
    /// Build a Redactor from this configuration
    ///
    /// Validates all patterns and logs warnings for invalid ones.
    /// Invalid custom patterns are skipped (with warning).
    /// Invalid disabled pattern names are also warned about.
    pub fn build_redactor(&self) -> Redactor {
        if !self.enabled {
            return Redactor::none();
        }

        let mut named_patterns: Vec<(String, String)> = Vec::new();

        // Validate disabled pattern names
        let valid_builtin_names: Vec<&str> = patterns::ALL_NAMED.iter().map(|np| np.name).collect();
        for disabled_name in &self.disabled_patterns {
            if !valid_builtin_names.contains(&disabled_name.as_str()) {
                eprintln!(
                    "whogitit: Warning - disabled pattern '{}' is not a valid builtin pattern name",
                    disabled_name
                );
                eprintln!(
                    "whogitit: Valid builtin patterns: {}",
                    valid_builtin_names.join(", ")
                );
            }
        }

        // Add builtin patterns (unless disabled)
        if self.use_builtin_patterns {
            for np in patterns::ALL_NAMED {
                if !self.disabled_patterns.contains(&np.name.to_string()) {
                    named_patterns.push((np.name.to_string(), np.pattern.to_string()));
                }
            }
        }

        // Validate and add custom patterns
        for custom in &self.custom_patterns {
            // Validate the regex pattern
            match regex::Regex::new(&custom.pattern) {
                Ok(_) => {
                    named_patterns.push((custom.name.clone(), custom.pattern.clone()));
                }
                Err(e) => {
                    eprintln!(
                        "whogitit: Warning - invalid custom pattern '{}': {}",
                        custom.name, e
                    );
                    eprintln!("whogitit: Pattern '{}' will be skipped", custom.name);
                }
            }
        }

        Redactor::with_named_patterns(&named_patterns)
    }

    /// List all available builtin pattern names
    pub fn available_patterns() -> Vec<(&'static str, &'static str)> {
        patterns::ALL_NAMED
            .iter()
            .map(|np| (np.name, np.description))
            .collect()
    }
}

/// Get whogitit config directory path
fn dirs_path() -> Option<PathBuf> {
    // Try XDG_CONFIG_HOME first, then fall back to ~/.config
    std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        })
        .map(|p| p.join("whogitit"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = WhogititConfig::default();
        assert!(config.privacy.enabled);
        assert!(config.privacy.use_builtin_patterns);
        assert!(config.privacy.custom_patterns.is_empty());
        assert!(config.privacy.disabled_patterns.is_empty());
        assert!(!config.privacy.audit_log);
    }

    #[test]
    fn test_parse_toml_config() {
        let toml = r#"
[privacy]
enabled = true
use_builtin_patterns = true
audit_log = true
disabled_patterns = ["EMAIL"]

[[privacy.custom_patterns]]
name = "INTERNAL_ID"
pattern = "INT-[0-9]{6}"
description = "Internal tracking IDs"
"#;

        let config: WhogititConfig = toml::from_str(toml).unwrap();

        assert!(config.privacy.enabled);
        assert!(config.privacy.audit_log);
        assert_eq!(config.privacy.disabled_patterns, vec!["EMAIL"]);
        assert_eq!(config.privacy.custom_patterns.len(), 1);
        assert_eq!(config.privacy.custom_patterns[0].name, "INTERNAL_ID");
    }

    #[test]
    fn test_build_redactor_with_disabled() {
        let config = PrivacyConfig {
            disabled_patterns: vec!["EMAIL".to_string()],
            ..Default::default()
        };

        let redactor = config.build_redactor();

        // EMAIL should be disabled
        let input = "test@example.com";
        let output = redactor.redact(input);
        assert_eq!(input, output);

        // But API_KEY should still work
        let input2 = "api_key = secret123";
        let output2 = redactor.redact(input2);
        assert!(output2.contains("[REDACTED]"));
    }

    #[test]
    fn test_build_redactor_with_custom() {
        let config = PrivacyConfig {
            use_builtin_patterns: false,
            custom_patterns: vec![PatternConfig {
                name: "CUSTOM".to_string(),
                pattern: r"CUSTOM-\d+".to_string(),
                description: None,
            }],
            ..Default::default()
        };

        let redactor = config.build_redactor();

        // Custom pattern should work
        let input = "Reference: CUSTOM-12345";
        let output = redactor.redact(input);
        assert!(output.contains("[REDACTED]"));

        // Builtin patterns should NOT work
        let input2 = "api_key = secret123";
        let output2 = redactor.redact(input2);
        assert_eq!(input2, output2);
    }

    #[test]
    fn test_disabled_redaction() {
        let config = PrivacyConfig {
            enabled: false,
            ..Default::default()
        };

        let redactor = config.build_redactor();

        let input = "api_key = secret123 and test@example.com";
        let output = redactor.redact(input);
        assert_eq!(input, output);
    }

    #[test]
    fn test_load_from_file() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join(".whogitit.toml");

        std::fs::write(
            &config_path,
            r#"
[privacy]
enabled = true
audit_log = true
"#,
        )
        .unwrap();

        let config = WhogititConfig::load_from_file(&config_path).unwrap();
        assert!(config.privacy.enabled);
        assert!(config.privacy.audit_log);
    }

    #[test]
    fn test_load_default_when_missing() {
        let dir = TempDir::new().unwrap();
        let config = WhogititConfig::load(dir.path()).unwrap();

        // Should return defaults
        assert!(config.privacy.enabled);
        assert!(!config.privacy.audit_log);
    }

    #[test]
    fn test_available_patterns() {
        let patterns = PrivacyConfig::available_patterns();
        assert!(!patterns.is_empty());

        let names: Vec<_> = patterns.iter().map(|(name, _)| *name).collect();
        assert!(names.contains(&"API_KEY"));
        assert!(names.contains(&"EMAIL"));
        assert!(names.contains(&"SSN"));
        assert!(names.contains(&"CREDIT_CARD"));
    }

    #[test]
    fn test_retention_config() {
        let toml = r#"
[retention]
max_age_days = 365
auto_purge = false
retain_refs = ["refs/heads/main", "refs/heads/release"]
min_commits = 50
"#;

        let config: WhogititConfig = toml::from_str(toml).unwrap();
        let retention = config.retention.unwrap();

        assert_eq!(retention.max_age_days, Some(365));
        assert!(!retention.auto_purge);
        assert_eq!(retention.retain_refs.len(), 2);
        assert_eq!(retention.min_commits, Some(50));
    }

    #[test]
    fn test_invalid_custom_pattern_validation() {
        // Config with an invalid regex pattern
        let config = PrivacyConfig {
            custom_patterns: vec![
                PatternConfig {
                    name: "VALID".to_string(),
                    pattern: r"\d+".to_string(),
                    description: None,
                },
                PatternConfig {
                    name: "INVALID".to_string(),
                    pattern: r"[invalid(".to_string(), // Invalid regex
                    description: None,
                },
            ],
            ..Default::default()
        };

        // Build should succeed (skipping invalid pattern)
        let redactor = config.build_redactor();

        // Only the valid custom pattern should be added (plus builtins)
        let names = redactor.pattern_names();
        assert!(names.contains(&"VALID"));
        assert!(!names.contains(&"INVALID"));
    }

    #[test]
    fn test_invalid_disabled_pattern_name() {
        // Config with an invalid disabled pattern name
        let config = PrivacyConfig {
            disabled_patterns: vec!["NOT_A_REAL_PATTERN".to_string()],
            ..Default::default()
        };

        // Build should succeed (with warning logged)
        let redactor = config.build_redactor();

        // Should still have all builtin patterns (nothing was actually disabled)
        let names = redactor.pattern_names();
        assert!(names.contains(&"API_KEY"));
        assert!(names.contains(&"EMAIL"));
    }
}
