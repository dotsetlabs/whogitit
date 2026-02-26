//! Setup and doctor commands for whogitit configuration
//!
//! The `setup` command handles one-time global configuration:
//! - Installing the capture hook script to ~/.claude/hooks/
//! - Configuring Claude Code settings.json with hook configuration
//!
//! The `doctor` command verifies the configuration is correct.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::{json, Value};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// The embedded capture hook script
pub const CAPTURE_HOOK_SCRIPT: &str = include_str!("../../hooks/whogitit-capture.sh");

/// Get the Claude configuration directory path
///
/// Returns None if home directory cannot be determined (e.g., in containerized environments).
pub fn claude_config_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude"))
}

/// Get the Claude configuration directory path, or error if unavailable
pub fn claude_config_dir_required() -> Result<PathBuf> {
    claude_config_dir().ok_or_else(|| {
        anyhow::anyhow!(
            "Could not determine home directory. Set HOME environment variable or run with --help for alternatives."
        )
    })
}

/// Get the Claude hooks directory path
pub fn claude_hooks_dir() -> Option<PathBuf> {
    claude_config_dir().map(|c| c.join("hooks"))
}

/// Get the Claude settings.json path
pub fn claude_settings_path() -> Option<PathBuf> {
    claude_config_dir().map(|c| c.join("settings.json"))
}

/// Get the capture hook script path
pub fn capture_hook_path() -> Option<PathBuf> {
    claude_hooks_dir().map(|h| h.join("whogitit-capture.sh"))
}

/// The hook configuration that needs to be in settings.json
fn hook_configuration() -> Value {
    json!({
        "PreToolUse": [
            {
                "matcher": "Edit|Write|Bash",
                "hooks": [
                    {
                        "type": "command",
                        "command": "WHOGITIT_HOOK_PHASE=pre ~/.claude/hooks/whogitit-capture.sh"
                    }
                ]
            }
        ],
        "PostToolUse": [
            {
                "matcher": "Edit|Write|Bash",
                "hooks": [
                    {
                        "type": "command",
                        "command": "WHOGITIT_HOOK_PHASE=post ~/.claude/hooks/whogitit-capture.sh"
                    }
                ]
            }
        ]
    })
}

/// Check if whogitit hooks are already configured in a settings value
fn has_whogitit_hooks(settings: &Value) -> bool {
    has_whogitit_phase_hook(settings, "PreToolUse", "pre")
        && has_whogitit_phase_hook(settings, "PostToolUse", "post")
}

fn has_whogitit_phase_hook(settings: &Value, phase_key: &str, phase_value: &str) -> bool {
    let expected_phase = format!("WHOGITIT_HOOK_PHASE={phase_value}");

    settings
        .get("hooks")
        .and_then(|hooks| hooks.get(phase_key))
        .and_then(Value::as_array)
        .map(|entries| {
            entries.iter().any(|entry| {
                entry
                    .get("hooks")
                    .and_then(Value::as_array)
                    .map(|inner_arr| {
                        inner_arr.iter().any(|hook| {
                            hook.get("command")
                                .and_then(Value::as_str)
                                .map(|cmd| {
                                    cmd.contains("whogitit-capture.sh")
                                        && cmd.contains(&expected_phase)
                                })
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// Merge whogitit hooks into existing settings
fn merge_hooks_into_settings(mut settings: Value) -> Value {
    let hook_config = hook_configuration();

    // Ensure hooks object exists
    let hooks_is_object = settings
        .get("hooks")
        .map(|hooks| hooks.is_object())
        .unwrap_or(false);
    if !hooks_is_object {
        if settings.get("hooks").is_some() {
            eprintln!(
                "whogitit: Warning - settings.json hooks is not an object, replacing with defaults"
            );
        }
        settings["hooks"] = json!({});
    }

    let hooks = settings["hooks"]
        .as_object_mut()
        .expect("hooks should be an object after normalization");

    // Merge PreToolUse
    if let Some(pre_hooks) = hook_config.get("PreToolUse") {
        if hooks.contains_key("PreToolUse") {
            // Append to existing array
            if let Some(existing) = hooks.get_mut("PreToolUse") {
                if let Some(arr) = existing.as_array_mut() {
                    if let Some(new_hooks) = pre_hooks.as_array() {
                        arr.extend(new_hooks.iter().cloned());
                    }
                }
            }
        } else {
            hooks.insert("PreToolUse".to_string(), pre_hooks.clone());
        }
    }

    // Merge PostToolUse
    if let Some(post_hooks) = hook_config.get("PostToolUse") {
        if hooks.contains_key("PostToolUse") {
            // Append to existing array
            if let Some(existing) = hooks.get_mut("PostToolUse") {
                if let Some(arr) = existing.as_array_mut() {
                    if let Some(new_hooks) = post_hooks.as_array() {
                        arr.extend(new_hooks.iter().cloned());
                    }
                }
            }
        } else {
            hooks.insert("PostToolUse".to_string(), post_hooks.clone());
        }
    }

    settings
}

/// Result of checking setup status
#[derive(Debug, Clone)]
pub struct SetupStatus {
    pub hook_script_installed: bool,
    pub hook_script_executable: bool,
    pub settings_configured: bool,
    pub claude_dir_exists: bool,
}

impl SetupStatus {
    /// Check if global setup is complete
    pub fn is_complete(&self) -> bool {
        self.hook_script_installed && self.hook_script_executable && self.settings_configured
    }
}

/// Check the current setup status
pub fn check_setup_status() -> SetupStatus {
    let claude_dir = match claude_config_dir() {
        Some(dir) => dir,
        None => {
            return SetupStatus {
                hook_script_installed: false,
                hook_script_executable: false,
                settings_configured: false,
                claude_dir_exists: false,
            };
        }
    };

    let hook_path = claude_dir.join("hooks").join("whogitit-capture.sh");
    let settings_path = claude_dir.join("settings.json");

    let claude_dir_exists = claude_dir.exists();
    let hook_script_installed = hook_path.exists();

    let hook_script_executable = if hook_script_installed {
        #[cfg(unix)]
        {
            fs::metadata(&hook_path)
                .map(|m| m.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
        }
        #[cfg(not(unix))]
        {
            true // Windows doesn't need execute permission
        }
    } else {
        false
    };

    let settings_configured = if settings_path.exists() {
        fs::read_to_string(&settings_path)
            .ok()
            .and_then(|content| serde_json::from_str::<Value>(&content).ok())
            .map(|settings| has_whogitit_hooks(&settings))
            .unwrap_or(false)
    } else {
        false
    };

    SetupStatus {
        hook_script_installed,
        hook_script_executable,
        settings_configured,
        claude_dir_exists,
    }
}

/// Install the capture hook script
fn install_hook_script() -> Result<bool> {
    let hooks_dir =
        claude_hooks_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
    let hook_path = hooks_dir.join("whogitit-capture.sh");

    // Create hooks directory if needed
    if !hooks_dir.exists() {
        fs::create_dir_all(&hooks_dir).context("Failed to create ~/.claude/hooks directory")?;
    }

    // Check if already installed with same content
    if hook_path.exists() {
        let existing = fs::read_to_string(&hook_path)?;
        if existing == CAPTURE_HOOK_SCRIPT {
            return Ok(false); // Already up to date
        }
    }

    // Write the hook script
    fs::write(&hook_path, CAPTURE_HOOK_SCRIPT).context("Failed to write capture hook script")?;

    // Make executable on Unix
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&hook_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&hook_path, perms)?;
    }

    Ok(true)
}

/// Configure Claude Code settings.json
fn configure_settings() -> Result<bool> {
    let claude_dir =
        claude_config_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
    let settings_path = claude_dir.join("settings.json");

    // Create .claude directory if needed
    if !claude_dir.exists() {
        fs::create_dir_all(&claude_dir).context("Failed to create ~/.claude directory")?;
    }

    // Load existing settings or create new
    let settings: Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)?;
        serde_json::from_str(&content).context("Failed to parse ~/.claude/settings.json")?
    } else {
        json!({})
    };

    // Check if already configured
    if has_whogitit_hooks(&settings) {
        return Ok(false); // Already configured
    }

    // Backup existing settings
    if settings_path.exists() {
        let backup_path = claude_dir.join("settings.json.backup");
        fs::copy(&settings_path, &backup_path).context("Failed to backup settings.json")?;
    }

    // Merge and write new settings
    let new_settings = merge_hooks_into_settings(settings);
    let formatted = serde_json::to_string_pretty(&new_settings)?;
    fs::write(&settings_path, formatted).context("Failed to write settings.json")?;

    Ok(true)
}

/// Run the setup command
pub fn run_setup() -> Result<()> {
    println!("Setting up whogitit for Claude Code...\n");

    // Install hook script
    match install_hook_script() {
        Ok(true) => println!("  Installed capture hook to ~/.claude/hooks/whogitit-capture.sh"),
        Ok(false) => println!("  Capture hook already installed and up to date."),
        Err(e) => {
            return Err(e.context("Failed to install capture hook"));
        }
    }

    // Configure settings.json
    match configure_settings() {
        Ok(true) => {
            println!("  Configured Claude Code hooks in ~/.claude/settings.json");
            println!("    (Previous settings backed up to settings.json.backup)");
        }
        Ok(false) => println!("  Claude Code hooks already configured."),
        Err(e) => {
            return Err(e.context("Failed to configure Claude Code settings"));
        }
    }

    println!("\nGlobal setup complete!");
    println!("\nNext steps:");
    println!("  1. Run 'whogitit init' in each repository you want to track");
    println!("  2. Use Claude Code normally - AI attribution will be captured automatically");
    println!("\nRun 'whogitit doctor' to verify your configuration at any time.");

    Ok(())
}

/// Result of a single doctor check
#[derive(Debug)]
pub struct DoctorCheck {
    pub name: &'static str,
    pub passed: bool,
    pub message: String,
    pub fix_hint: Option<String>,
}

/// Run the doctor command
pub fn run_doctor() -> Result<()> {
    println!("Checking whogitit configuration...\n");

    let mut checks: Vec<DoctorCheck> = Vec::new();
    let mut all_passed = true;

    // Check 1: whogitit binary
    checks.push(check_binary());

    // Check 2: Capture hook installed
    checks.push(check_hook_installed());

    // Check 3: Capture hook executable
    checks.push(check_hook_executable());

    // Check 4: Claude settings configured
    checks.push(check_settings_configured());

    // Check 5: Required tools (jq)
    checks.push(check_required_tools());

    // Check 6: Git repo (if in one)
    if let Some(repo_check) = check_git_repo() {
        checks.push(repo_check);
    }

    // Check 7: Orphaned notes (if in a git repo with notes)
    if let Some(notes_check) = check_orphaned_notes() {
        checks.push(notes_check);
    }

    // Display results
    for check in &checks {
        let status = if check.passed { "[OK]" } else { "[FAIL]" };
        println!("{} {}: {}", status, check.name, check.message);
        if !check.passed {
            all_passed = false;
            if let Some(hint) = &check.fix_hint {
                println!("   Fix: {}", hint);
            }
        }
    }

    println!();

    if all_passed {
        println!("All checks passed! whogitit is properly configured.");
    } else {
        println!("Some checks failed. Run 'whogitit setup' to fix configuration issues.");
    }

    Ok(())
}

fn check_binary() -> DoctorCheck {
    // The binary is obviously available if we're running
    DoctorCheck {
        name: "whogitit binary",
        passed: true,
        message: "Installed and running".to_string(),
        fix_hint: None,
    }
}

fn check_hook_installed() -> DoctorCheck {
    let hook_path = match capture_hook_path() {
        Some(p) => p,
        None => {
            return DoctorCheck {
                name: "Capture hook",
                passed: false,
                message: "Cannot determine home directory".to_string(),
                fix_hint: Some("Set HOME environment variable".to_string()),
            }
        }
    };

    if hook_path.exists() {
        // Also check if it's the current version
        let is_current = fs::read_to_string(&hook_path)
            .map(|content| content == CAPTURE_HOOK_SCRIPT)
            .unwrap_or(false);

        if is_current {
            DoctorCheck {
                name: "Capture hook",
                passed: true,
                message: format!("Installed at {}", hook_path.display()),
                fix_hint: None,
            }
        } else {
            DoctorCheck {
                name: "Capture hook",
                passed: false,
                message: "Installed but outdated".to_string(),
                fix_hint: Some("Run 'whogitit setup' to update".to_string()),
            }
        }
    } else {
        DoctorCheck {
            name: "Capture hook",
            passed: false,
            message: "Not installed".to_string(),
            fix_hint: Some("Run 'whogitit setup' to install".to_string()),
        }
    }
}

fn check_hook_executable() -> DoctorCheck {
    let hook_path = match capture_hook_path() {
        Some(p) => p,
        None => {
            return DoctorCheck {
                name: "Hook permissions",
                passed: false,
                message: "Cannot determine home directory".to_string(),
                fix_hint: Some("Set HOME environment variable".to_string()),
            }
        }
    };

    if !hook_path.exists() {
        return DoctorCheck {
            name: "Hook permissions",
            passed: false,
            message: "Hook not installed".to_string(),
            fix_hint: Some("Run 'whogitit setup'".to_string()),
        };
    }

    #[cfg(unix)]
    {
        let executable = fs::metadata(&hook_path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);

        if executable {
            DoctorCheck {
                name: "Hook permissions",
                passed: true,
                message: "Executable".to_string(),
                fix_hint: None,
            }
        } else {
            DoctorCheck {
                name: "Hook permissions",
                passed: false,
                message: "Not executable".to_string(),
                fix_hint: Some(format!("Run 'chmod +x {}'", hook_path.display())),
            }
        }
    }

    #[cfg(not(unix))]
    {
        DoctorCheck {
            name: "Hook permissions",
            passed: true,
            message: "OK (Windows)".to_string(),
            fix_hint: None,
        }
    }
}

fn check_settings_configured() -> DoctorCheck {
    let settings_path = match claude_settings_path() {
        Some(p) => p,
        None => {
            return DoctorCheck {
                name: "Claude Code settings",
                passed: false,
                message: "Cannot determine home directory".to_string(),
                fix_hint: Some("Set HOME environment variable".to_string()),
            }
        }
    };

    if !settings_path.exists() {
        return DoctorCheck {
            name: "Claude Code settings",
            passed: false,
            message: "settings.json not found".to_string(),
            fix_hint: Some("Run 'whogitit setup' to configure".to_string()),
        };
    }

    let content = match fs::read_to_string(&settings_path) {
        Ok(c) => c,
        Err(_) => {
            return DoctorCheck {
                name: "Claude Code settings",
                passed: false,
                message: "Cannot read settings.json".to_string(),
                fix_hint: Some("Check file permissions".to_string()),
            }
        }
    };

    let settings: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => {
            return DoctorCheck {
                name: "Claude Code settings",
                passed: false,
                message: "Invalid JSON in settings.json".to_string(),
                fix_hint: Some("Check settings.json syntax".to_string()),
            }
        }
    };

    if has_whogitit_hooks(&settings) {
        DoctorCheck {
            name: "Claude Code settings",
            passed: true,
            message: "Hooks configured".to_string(),
            fix_hint: None,
        }
    } else {
        DoctorCheck {
            name: "Claude Code settings",
            passed: false,
            message: "whogitit hooks not configured".to_string(),
            fix_hint: Some("Run 'whogitit setup' to configure".to_string()),
        }
    }
}

fn check_required_tools() -> DoctorCheck {
    // Check for jq which is required by the hook script
    let jq_available = std::process::Command::new("jq")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if jq_available {
        DoctorCheck {
            name: "Required tools (jq)",
            passed: true,
            message: "Available".to_string(),
            fix_hint: None,
        }
    } else {
        DoctorCheck {
            name: "Required tools (jq)",
            passed: false,
            message: "jq not found".to_string(),
            fix_hint: Some(
                "Install jq: brew install jq (macOS) or apt install jq (Linux)".to_string(),
            ),
        }
    }
}

fn check_orphaned_notes() -> Option<DoctorCheck> {
    let repo = git2::Repository::discover(".").ok()?;
    let store = crate::storage::notes::NotesStore::new(&repo).ok()?;

    let all_notes = store.list_attributed_commits().ok()?;
    if all_notes.is_empty() {
        return None;
    }

    let mut orphaned = 0;
    for oid in &all_notes {
        if repo.find_commit(*oid).is_err() {
            orphaned += 1;
        }
    }

    Some(DoctorCheck {
        name: "Attribution notes",
        passed: orphaned == 0,
        message: if orphaned == 0 {
            format!("{} notes, all valid", all_notes.len())
        } else {
            format!(
                "{}/{} notes orphaned (commits deleted)",
                orphaned,
                all_notes.len()
            )
        },
        fix_hint: if orphaned > 0 {
            Some("Run 'git notes --ref=whogitit prune' to clean up".to_string())
        } else {
            None
        },
    })
}

fn check_git_repo() -> Option<DoctorCheck> {
    // Only check if we're in a git repo
    let repo = git2::Repository::discover(".").ok()?;
    let repo_root = repo.workdir()?;

    let hooks_dir = repo_root.join(".git/hooks");
    let post_commit = hooks_dir.join("post-commit");
    let pre_push = hooks_dir.join("pre-push");
    let post_rewrite = hooks_dir.join("post-rewrite");

    let post_commit_ok = post_commit.exists()
        && fs::read_to_string(&post_commit)
            .map(|c| c.contains("whogitit"))
            .unwrap_or(false);

    let pre_push_ok = pre_push.exists()
        && fs::read_to_string(&pre_push)
            .map(|c| c.contains("whogitit"))
            .unwrap_or(false);

    let post_rewrite_ok = post_rewrite.exists()
        && fs::read_to_string(&post_rewrite)
            .map(|c| c.contains("whogitit"))
            .unwrap_or(false);

    if post_commit_ok && pre_push_ok && post_rewrite_ok {
        Some(DoctorCheck {
            name: "Repository hooks",
            passed: true,
            message: "Initialized in current repo".to_string(),
            fix_hint: None,
        })
    } else {
        let mut missing = Vec::new();
        if !post_commit_ok {
            missing.push("post-commit");
        }
        if !pre_push_ok {
            missing.push("pre-push");
        }
        if !post_rewrite_ok {
            missing.push("post-rewrite");
        }
        Some(DoctorCheck {
            name: "Repository hooks",
            passed: false,
            message: format!("Missing or invalid hooks: {}", missing.join(", ")),
            fix_hint: Some("Run 'whogitit init' in this repository".to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_whogitit_hooks_empty() {
        let settings = json!({});
        assert!(!has_whogitit_hooks(&settings));
    }

    #[test]
    fn test_has_whogitit_hooks_other_hooks() {
        let settings = json!({
            "hooks": {
                "PreToolUse": [
                    {
                        "matcher": "Edit",
                        "hooks": [
                            {
                                "type": "command",
                                "command": "some-other-hook"
                            }
                        ]
                    }
                ]
            }
        });
        assert!(!has_whogitit_hooks(&settings));
    }

    #[test]
    fn test_has_whogitit_hooks_configured() {
        let settings = json!({
            "hooks": {
                "PreToolUse": [
                    {
                        "matcher": "Edit|Write|Bash",
                        "hooks": [
                            {
                                "type": "command",
                                "command": "WHOGITIT_HOOK_PHASE=pre ~/.claude/hooks/whogitit-capture.sh"
                            }
                        ]
                    }
                ],
                "PostToolUse": [
                    {
                        "matcher": "Edit|Write|Bash",
                        "hooks": [
                            {
                                "type": "command",
                                "command": "WHOGITIT_HOOK_PHASE=post ~/.claude/hooks/whogitit-capture.sh"
                            }
                        ]
                    }
                ]
            }
        });
        assert!(has_whogitit_hooks(&settings));
    }

    #[test]
    fn test_has_whogitit_hooks_requires_both_phases() {
        let pre_only = json!({
            "hooks": {
                "PreToolUse": [
                    {
                        "matcher": "Edit|Write|Bash",
                        "hooks": [
                            {
                                "type": "command",
                                "command": "WHOGITIT_HOOK_PHASE=pre ~/.claude/hooks/whogitit-capture.sh"
                            }
                        ]
                    }
                ]
            }
        });
        assert!(!has_whogitit_hooks(&pre_only));
    }

    #[test]
    fn test_merge_hooks_empty_settings() {
        let settings = json!({});
        let merged = merge_hooks_into_settings(settings);

        assert!(merged.get("hooks").is_some());
        assert!(merged["hooks"].get("PreToolUse").is_some());
        assert!(merged["hooks"].get("PostToolUse").is_some());
    }

    #[test]
    fn test_merge_hooks_preserves_existing() {
        let settings = json!({
            "other_setting": "value",
            "hooks": {
                "PreToolUse": [
                    {
                        "matcher": "Read",
                        "hooks": [
                            {
                                "type": "command",
                                "command": "existing-hook"
                            }
                        ]
                    }
                ]
            }
        });

        let merged = merge_hooks_into_settings(settings);

        // Original setting preserved
        assert_eq!(merged["other_setting"], "value");

        // Existing hook preserved
        let pre_hooks = merged["hooks"]["PreToolUse"].as_array().unwrap();
        assert!(pre_hooks.len() >= 2); // Original + new

        // Has both old and new hooks
        let has_existing = pre_hooks
            .iter()
            .any(|h| h["hooks"][0]["command"].as_str() == Some("existing-hook"));
        let has_whogitit = pre_hooks.iter().any(|h| {
            h["hooks"][0]["command"]
                .as_str()
                .unwrap_or("")
                .contains("whogitit")
        });

        assert!(has_existing, "Should preserve existing hook");
        assert!(has_whogitit, "Should add whogitit hook");
    }

    #[test]
    fn test_merge_hooks_replaces_invalid_hooks() {
        let settings = json!({
            "hooks": "not-an-object"
        });

        let merged = merge_hooks_into_settings(settings);

        assert!(merged["hooks"].is_object());
        assert!(merged["hooks"].get("PreToolUse").is_some());
        assert!(merged["hooks"].get("PostToolUse").is_some());
    }

    #[test]
    fn test_hook_configuration_structure() {
        let config = hook_configuration();

        // Check PreToolUse structure
        let pre = &config["PreToolUse"][0];
        assert_eq!(pre["matcher"], "Edit|Write|Bash");
        assert!(pre["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("WHOGITIT_HOOK_PHASE=pre"));

        // Check PostToolUse structure
        let post = &config["PostToolUse"][0];
        assert_eq!(post["matcher"], "Edit|Write|Bash");
        assert!(post["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("WHOGITIT_HOOK_PHASE=post"));
    }

    #[test]
    fn test_doctor_check_structure() {
        let check = DoctorCheck {
            name: "Test check",
            passed: true,
            message: "Test passed".to_string(),
            fix_hint: None,
        };

        assert_eq!(check.name, "Test check");
        assert!(check.passed);
        assert!(check.fix_hint.is_none());
    }

    #[test]
    fn test_doctor_check_with_fix_hint() {
        let check = DoctorCheck {
            name: "Failing check",
            passed: false,
            message: "Something is wrong".to_string(),
            fix_hint: Some("Run this command to fix".to_string()),
        };

        assert!(!check.passed);
        assert!(check.fix_hint.is_some());
        assert_eq!(
            check.fix_hint.unwrap(),
            "Run this command to fix".to_string()
        );
    }

    #[test]
    fn test_setup_status_is_complete() {
        let complete = SetupStatus {
            hook_script_installed: true,
            hook_script_executable: true,
            settings_configured: true,
            claude_dir_exists: true,
        };
        assert!(complete.is_complete());

        let incomplete1 = SetupStatus {
            hook_script_installed: false,
            hook_script_executable: true,
            settings_configured: true,
            claude_dir_exists: true,
        };
        assert!(!incomplete1.is_complete());

        let incomplete2 = SetupStatus {
            hook_script_installed: true,
            hook_script_executable: false,
            settings_configured: true,
            claude_dir_exists: true,
        };
        assert!(!incomplete2.is_complete());

        let incomplete3 = SetupStatus {
            hook_script_installed: true,
            hook_script_executable: true,
            settings_configured: false,
            claude_dir_exists: true,
        };
        assert!(!incomplete3.is_complete());
    }
}
