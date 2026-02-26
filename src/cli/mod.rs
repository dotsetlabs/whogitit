pub mod annotations;
pub mod audit;
pub mod blame;
pub mod copy;
pub mod export;
pub mod output;
pub mod pager;
pub mod prompt;
pub mod redact;
pub mod retention;
pub mod setup;
pub mod show;
pub mod summary;

use std::fs;

use anyhow::{Context, Result};

use clap::{Parser, Subcommand};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::capture::hook;
use crate::privacy::WhogititConfig;
use crate::storage::audit::AuditLog;

/// AI-aware git blame tool for tracking AI-generated code
#[derive(Debug, Parser)]
#[command(name = "whogitit")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Show AI attribution for each line of a file
    Blame(blame::BlameArgs),

    /// View the prompt that generated specific lines
    Prompt(prompt::PromptArgs),

    /// Show AI attribution summary for a commit
    Show(show::ShowArgs),

    /// Generate summary for a range of commits (useful for PRs)
    Summary(summary::SummaryArgs),

    /// Generate annotations for GitHub Checks API
    Annotations(annotations::AnnotationsArgs),

    /// Annotate git diff output with AI attribution (for use as git pager)
    Pager(pager::PagerArgs),

    /// Test redaction patterns against text or files
    RedactTest(redact::RedactArgs),

    /// Export attribution data for multiple commits
    Export(export::ExportArgs),

    /// Manage data retention policies
    Retention(retention::RetentionArgs),

    /// View the audit log
    Audit(audit::AuditArgs),

    /// Capture a file change (called by Claude Code hook)
    #[command(hide = true)]
    Capture(CaptureArgs),

    /// Finalize attribution after a commit (post-commit hook)
    #[command(hide = true)]
    PostCommit,

    /// Show pending changes status
    Status,

    /// Clear pending changes without committing
    Clear,

    /// Initialize whogitit in a git repository (installs post-commit hook)
    Init(InitArgs),

    /// Set up whogitit globally (install capture hook and configure Claude Code)
    Setup,

    /// Check whogitit configuration and diagnose issues
    Doctor,

    /// Copy AI attribution from one commit to another
    CopyNotes(copy::CopyNotesArgs),
}

/// Init command arguments
#[derive(Debug, clap::Args)]
pub struct InitArgs {
    /// Skip global setup check
    #[arg(long)]
    pub force: bool,
}

/// Capture command arguments
#[derive(Debug, clap::Args)]
pub struct CaptureArgs {
    /// Read hook input from stdin
    #[arg(long)]
    pub stdin: bool,

    /// File path (if not using stdin)
    #[arg(long)]
    pub file: Option<String>,

    /// Tool name
    #[arg(long)]
    pub tool: Option<String>,

    /// Prompt text
    #[arg(long)]
    pub prompt: Option<String>,
}

/// Run the CLI
pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Blame(args) => blame::run(args),
        Commands::Prompt(args) => prompt::run(args),
        Commands::Show(args) => show::run(args),
        Commands::Summary(args) => summary::run(args),
        Commands::Annotations(args) => annotations::run(args),
        Commands::Pager(args) => pager::run(args),
        Commands::RedactTest(args) => redact::run(args),
        Commands::Export(args) => export::run(args),
        Commands::Retention(args) => retention::run(args),
        Commands::Audit(args) => audit::run(args),
        Commands::Capture(args) => run_capture(args),
        Commands::PostCommit => run_post_commit(),
        Commands::Status => run_status(),
        Commands::Clear => run_clear(),
        Commands::Init(args) => run_init(args),
        Commands::Setup => setup::run_setup(),
        Commands::Doctor => setup::run_doctor(),
        Commands::CopyNotes(args) => copy::run(args),
    }
}

fn run_capture(args: CaptureArgs) -> Result<()> {
    if args.stdin {
        hook::run_capture_hook()
    } else {
        anyhow::bail!("Capture requires --stdin flag for hook input")
    }
}

fn run_post_commit() -> Result<()> {
    hook::run_post_commit_hook()
}

fn run_status() -> Result<()> {
    let repo = git2::Repository::discover(".")?;
    let repo_root = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("No working directory"))?;

    let hook_handler = crate::capture::CaptureHook::new(repo_root)?;
    let status = hook_handler.status()?;

    if status.has_pending {
        println!("Pending AI attribution:");
        println!(
            "  Session: {}",
            status.session_id.as_deref().unwrap_or("unknown")
        );
        println!("  Files: {}", status.file_count);
        println!("  Edits: {}", status.edit_count);
        println!("  Lines: {}", status.line_count);
        if !status.age.is_empty() {
            println!("  Age: {}", status.age);
        }

        if status.is_stale {
            println!(
                "\n⚠️  Warning: This pending buffer is stale (> {} hours old).",
                status.max_pending_age_hours
            );
            println!("   Run 'whogitit clear' if these changes are no longer relevant.");
        } else {
            println!("\nRun 'git commit' to finalize attribution.");
        }
    } else {
        println!("No pending AI attribution.");
    }

    Ok(())
}

fn run_clear() -> Result<()> {
    let repo = git2::Repository::discover(".")?;
    let repo_root = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("No working directory"))?;

    let hook_handler = crate::capture::CaptureHook::new(repo_root)?;
    hook_handler.clear_pending()?;

    println!("Cleared pending AI attribution.");

    Ok(())
}

fn run_init(args: InitArgs) -> Result<()> {
    // Check global setup status first (unless --force is used)
    let status = setup::check_setup_status();
    if !status.is_complete() && !args.force {
        println!("Global setup incomplete:");
        if !status.hook_script_installed {
            println!("  - Capture hook not installed");
        }
        if !status.hook_script_executable {
            println!("  - Capture hook not executable");
        }
        if !status.settings_configured {
            println!("  - Claude Code hooks not configured");
        }
        println!();
        println!("Run 'whogitit setup' first to configure Claude Code integration.");
        println!("Then run 'whogitit init' again to initialize this repository.");
        println!();
        println!("Or run 'whogitit init --force' to skip this check and proceed anyway.");
        return Ok(());
    }

    if !status.is_complete() && args.force {
        println!("Warning: Global setup is incomplete. Proceeding with --force.\n");
    }

    let repo = git2::Repository::discover(".").context("Not in a git repository")?;
    let repo_root = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("No working directory"))?;

    let hooks_dir = repo_root.join(".git/hooks");
    fs::create_dir_all(&hooks_dir).context("Failed to create hooks directory")?;

    // Install post-commit hook (attaches attribution to commits)
    install_post_commit_hook(&hooks_dir)?;

    // Install pre-push hook (auto-pushes notes with regular git push)
    install_pre_push_hook(&hooks_dir)?;

    // Install post-rewrite hook (preserves notes during rebase/amend)
    install_post_rewrite_hook(&hooks_dir)?;

    // Configure git to auto-fetch notes
    let fetch_updated = configure_git_fetch(&repo)?;
    let exclude_updated = add_git_exclude(&repo)?;

    if let Ok(config) = WhogititConfig::load(repo_root) {
        if config.privacy.audit_log {
            let audit_log = AuditLog::new(repo_root);
            if fetch_updated {
                if let Err(e) = audit_log.log_config_change(
                    "git.remote.origin.fetch",
                    "Configured automatic fetch for whogitit notes",
                ) {
                    eprintln!("whogitit: Warning - failed to write audit event: {}", e);
                }
            }
            if exclude_updated {
                if let Err(e) = audit_log.log_config_change(
                    "git.info.exclude",
                    "Added whogitit local artifacts to .git/info/exclude",
                ) {
                    eprintln!("whogitit: Warning - failed to write audit event: {}", e);
                }
            }
        }
    }

    println!("\nRepository initialized! AI attribution will be tracked for commits in this repo.");
    println!("Notes will be automatically pushed with 'git push' and fetched with 'git fetch'.");

    if !status.is_complete() {
        println!("\nReminder: Run 'whogitit setup' to complete Claude Code integration.");
    }

    Ok(())
}

/// Marker comment to identify whogitit hook sections
const WHOGITIT_MARKER_START: &str = "# >>> whogitit hook start >>>";
const WHOGITIT_MARKER_END: &str = "# <<< whogitit hook end <<<";

fn install_post_commit_hook(hooks_dir: &std::path::Path) -> Result<()> {
    let hook_path = hooks_dir.join("post-commit");

    if hook_path.exists() {
        let content = fs::read_to_string(&hook_path)?;

        // Check for marker-based or legacy whogitit hook
        if content.contains(WHOGITIT_MARKER_START) || content.contains("whogitit post-commit") {
            println!("✓ whogitit post-commit hook already installed.");
            return Ok(());
        }

        // Append to existing hook with markers for idempotency
        let whogitit_section = format!(
            "\n\n{}\n# whogitit post-commit hook - Attaches AI attribution notes\nif command -v whogitit &> /dev/null; then\n    whogitit post-commit 2>/dev/null || true\nfi\n{}\n",
            WHOGITIT_MARKER_START,
            WHOGITIT_MARKER_END
        );
        let new_content = format!("{}{}", content.trim_end(), whogitit_section);
        fs::write(&hook_path, new_content)?;
        println!("✓ Added whogitit to existing post-commit hook.");
    } else {
        let hook_content = format!(
            r#"#!/bin/bash
{}
# whogitit post-commit hook
# Attaches AI attribution notes to the commit

if command -v whogitit &> /dev/null; then
    whogitit post-commit 2>/dev/null || true
elif [[ -x "$HOME/.cargo/bin/whogitit" ]]; then
    "$HOME/.cargo/bin/whogitit" post-commit 2>/dev/null || true
fi
{}
"#,
            WHOGITIT_MARKER_START, WHOGITIT_MARKER_END
        );
        fs::write(&hook_path, hook_content)?;
        make_executable(&hook_path)?;
        println!("✓ Installed whogitit post-commit hook.");
    }

    Ok(())
}

fn install_pre_push_hook(hooks_dir: &std::path::Path) -> Result<()> {
    let hook_path = hooks_dir.join("pre-push");

    if hook_path.exists() {
        let content = fs::read_to_string(&hook_path)?;

        // Check for marker-based or legacy whogitit hook
        if content.contains(WHOGITIT_MARKER_START) || content.contains("WHOGITIT_PUSHING_NOTES") {
            println!("✓ whogitit pre-push hook already installed.");
            return Ok(());
        }

        // Append to existing hook with markers for idempotency
        let whogitit_section = format!(
            "\n\n{}\n# whogitit pre-push hook - automatically push notes\n# Skip if already pushing notes (prevent recursion)\n[[ \"$WHOGITIT_PUSHING_NOTES\" == \"1\" ]] && exit 0\nremote=\"$1\"\nif git notes --ref=whogitit list &>/dev/null; then\n    WHOGITIT_PUSHING_NOTES=1 git push \"$remote\" refs/notes/whogitit 2>/dev/null || true\nfi\n{}\n",
            WHOGITIT_MARKER_START,
            WHOGITIT_MARKER_END
        );
        let new_content = format!("{}{}", content.trim_end(), whogitit_section);
        fs::write(&hook_path, new_content)?;
        println!("✓ Added whogitit to existing pre-push hook.");
    } else {
        let hook_content = format!(
            r#"#!/bin/bash
{}
# whogitit pre-push hook
# Automatically pushes whogitit notes alongside regular pushes

# Prevent recursion - skip if we're already pushing notes
[[ "$WHOGITIT_PUSHING_NOTES" == "1" ]] && exit 0

remote="$1"

# Only push notes if they exist
if git notes --ref=whogitit list &>/dev/null; then
    WHOGITIT_PUSHING_NOTES=1 git push "$remote" refs/notes/whogitit 2>/dev/null || true
fi
{}
"#,
            WHOGITIT_MARKER_START, WHOGITIT_MARKER_END
        );
        fs::write(&hook_path, hook_content)?;
        make_executable(&hook_path)?;
        println!("✓ Installed whogitit pre-push hook.");
    }

    Ok(())
}

fn install_post_rewrite_hook(hooks_dir: &std::path::Path) -> Result<()> {
    let hook_path = hooks_dir.join("post-rewrite");

    if hook_path.exists() {
        let content = fs::read_to_string(&hook_path)?;

        // Check for marker-based or legacy whogitit hook
        if content.contains(WHOGITIT_MARKER_START) || content.contains("whogitit") {
            println!("✓ whogitit post-rewrite hook already installed.");
            return Ok(());
        }

        // Append to existing hook with markers for idempotency
        let whogitit_section = format!(
            "\n\n{}\n# whogitit post-rewrite hook - preserve notes during rebase/amend\ncopied=0\nwhile read -r old_sha new_sha extra; do\n  [[ -z \"$old_sha\" || -z \"$new_sha\" ]] && continue\n  if git notes --ref=whogitit show \"$old_sha\" &>/dev/null; then\n    git notes --ref=whogitit copy \"$old_sha\" \"$new_sha\" 2>/dev/null && copied=$((copied + 1))\n  fi\ndone\n[[ $copied -gt 0 ]] && echo \"whogitit: Preserved attribution for $copied commit(s)\"\n{}\n",
            WHOGITIT_MARKER_START,
            WHOGITIT_MARKER_END
        );
        let new_content = format!("{}{}", content.trim_end(), whogitit_section);
        fs::write(&hook_path, new_content)?;
        println!("✓ Added whogitit to existing post-rewrite hook.");
    } else {
        let hook_content = format!(
            r#"#!/bin/bash
{}
# whogitit post-rewrite hook
# Preserves AI attribution notes during rebase/amend

copied=0
while read -r old_sha new_sha extra; do
  [[ -z "$old_sha" || -z "$new_sha" ]] && continue
  if git notes --ref=whogitit show "$old_sha" &>/dev/null; then
    git notes --ref=whogitit copy "$old_sha" "$new_sha" 2>/dev/null && copied=$((copied + 1))
  fi
done

[[ $copied -gt 0 ]] && echo "whogitit: Preserved attribution for $copied commit(s)"
{}
"#,
            WHOGITIT_MARKER_START, WHOGITIT_MARKER_END
        );
        fs::write(&hook_path, hook_content)?;
        make_executable(&hook_path)?;
        println!("✓ Installed whogitit post-rewrite hook.");
    }

    Ok(())
}

/// Make a file executable (Unix only - no-op on Windows)
#[cfg(unix)]
fn make_executable(path: &std::path::Path) -> Result<()> {
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)?;
    Ok(())
}

/// Make a file executable (no-op on Windows - scripts are executable by default)
#[cfg(not(unix))]
fn make_executable(_path: &std::path::Path) -> Result<()> {
    // On Windows, scripts don't need execute permission
    Ok(())
}

/// Configure git to automatically fetch whogitit notes
fn configure_git_fetch(repo: &git2::Repository) -> Result<bool> {
    let mut config = repo.config().context("Failed to open git config")?;

    let fetch_refspec = "+refs/notes/whogitit:refs/notes/whogitit";
    let mut existing_fetch = Vec::new();
    if let Ok(entries) = config.entries(Some("remote.origin.fetch")) {
        entries.for_each(|entry| {
            if let Some(value) = entry.value() {
                existing_fetch.push(value.to_string());
            }
        })?;
    }
    let fetch_configured = existing_fetch.iter().any(|v| v.contains("whogitit"));

    if !fetch_configured {
        let result = config.set_multivar("remote.origin.fetch", "^$", fetch_refspec);
        if result.is_err() {
            if existing_fetch.is_empty() {
                config
                    .set_str("remote.origin.fetch", fetch_refspec)
                    .context("Failed to configure fetch refspec")?;
            } else {
                eprintln!(
                    "whogitit: Warning - unable to add fetch refspec without overwriting existing settings."
                );
                eprintln!("whogitit: Please add this manually:\n  {}", fetch_refspec);
                return Ok(false);
            }
        }
        println!("✓ Configured git to fetch whogitit notes automatically.");
        return Ok(true);
    } else {
        println!("✓ Git already configured to fetch whogitit notes.");
    }

    Ok(false)
}

/// Add whogitit artifacts to git exclude list to avoid accidental commits
fn add_git_exclude(repo: &git2::Repository) -> Result<bool> {
    let git_dir = repo.path();
    let info_dir = git_dir.join("info");
    fs::create_dir_all(&info_dir).context("Failed to create .git/info directory")?;

    let exclude_path = info_dir.join("exclude");
    let existing = fs::read_to_string(&exclude_path).unwrap_or_default();

    if existing.contains("# >>> whogitit ignore >>>") {
        println!("✓ Git exclude already configured for whogitit artifacts.");
        return Ok(false);
    }

    let block = [
        "",
        "# >>> whogitit ignore >>>",
        "# whogitit local artifacts",
        ".whogitit-pending.json",
        ".whogitit-pending.lock",
        ".whogitit-pending.tmp",
        ".whogitit-pending.*",
        ".whogitit/",
        "# <<< whogitit ignore <<<",
        "",
    ]
    .join("\n");

    let new_content = format!("{}{}", existing.trim_end(), block);
    fs::write(&exclude_path, new_content).context("Failed to update git exclude")?;
    println!("✓ Added whogitit artifacts to .git/info/exclude.");

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_hooks_dir() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn test_whogitit_markers() {
        assert!(WHOGITIT_MARKER_START.contains("whogitit"));
        assert!(WHOGITIT_MARKER_END.contains("whogitit"));
        assert!(WHOGITIT_MARKER_START.contains(">>>"));
        assert!(WHOGITIT_MARKER_END.contains("<<<"));
    }

    #[test]
    fn test_install_post_commit_hook_new() {
        let dir = create_test_hooks_dir();
        install_post_commit_hook(dir.path()).unwrap();

        let hook_path = dir.path().join("post-commit");
        assert!(hook_path.exists());

        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains(WHOGITIT_MARKER_START));
        assert!(content.contains(WHOGITIT_MARKER_END));
        assert!(content.contains("whogitit post-commit"));
        assert!(content.starts_with("#!/bin/bash"));
    }

    #[test]
    fn test_install_post_commit_hook_idempotent() {
        let dir = create_test_hooks_dir();

        // Install twice
        install_post_commit_hook(dir.path()).unwrap();
        install_post_commit_hook(dir.path()).unwrap();

        let hook_path = dir.path().join("post-commit");
        let content = fs::read_to_string(&hook_path).unwrap();

        // Should only have one marker section
        let marker_count = content.matches(WHOGITIT_MARKER_START).count();
        assert_eq!(marker_count, 1);
    }

    #[test]
    fn test_install_post_commit_hook_append_to_existing() {
        let dir = create_test_hooks_dir();
        let hook_path = dir.path().join("post-commit");

        // Create existing hook
        fs::write(&hook_path, "#!/bin/bash\necho 'existing hook'\n").unwrap();

        install_post_commit_hook(dir.path()).unwrap();

        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains("existing hook"));
        assert!(content.contains(WHOGITIT_MARKER_START));
        assert!(content.contains("whogitit post-commit"));
    }

    #[test]
    fn test_install_pre_push_hook_new() {
        let dir = create_test_hooks_dir();
        install_pre_push_hook(dir.path()).unwrap();

        let hook_path = dir.path().join("pre-push");
        assert!(hook_path.exists());

        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains(WHOGITIT_MARKER_START));
        assert!(content.contains("WHOGITIT_PUSHING_NOTES"));
        assert!(content.contains("refs/notes/whogitit"));
    }

    #[test]
    fn test_install_pre_push_hook_idempotent() {
        let dir = create_test_hooks_dir();

        install_pre_push_hook(dir.path()).unwrap();
        install_pre_push_hook(dir.path()).unwrap();

        let hook_path = dir.path().join("pre-push");
        let content = fs::read_to_string(&hook_path).unwrap();

        let marker_count = content.matches(WHOGITIT_MARKER_START).count();
        assert_eq!(marker_count, 1);
    }

    #[test]
    fn test_install_post_rewrite_hook_new() {
        let dir = create_test_hooks_dir();
        install_post_rewrite_hook(dir.path()).unwrap();

        let hook_path = dir.path().join("post-rewrite");
        assert!(hook_path.exists());

        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains(WHOGITIT_MARKER_START));
        assert!(content.contains("git notes --ref=whogitit copy"));
        assert!(content.contains("Preserved attribution"));
    }

    #[test]
    fn test_install_post_rewrite_hook_idempotent() {
        let dir = create_test_hooks_dir();

        install_post_rewrite_hook(dir.path()).unwrap();
        install_post_rewrite_hook(dir.path()).unwrap();

        let hook_path = dir.path().join("post-rewrite");
        let content = fs::read_to_string(&hook_path).unwrap();

        let marker_count = content.matches(WHOGITIT_MARKER_START).count();
        assert_eq!(marker_count, 1);
    }

    #[test]
    fn test_install_post_rewrite_hook_append_to_existing() {
        let dir = create_test_hooks_dir();
        let hook_path = dir.path().join("post-rewrite");

        // Create existing hook
        fs::write(&hook_path, "#!/bin/bash\necho 'existing rewrite hook'\n").unwrap();

        install_post_rewrite_hook(dir.path()).unwrap();

        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains("existing rewrite hook"));
        assert!(content.contains(WHOGITIT_MARKER_START));
        assert!(content.contains("git notes --ref=whogitit copy"));
    }

    #[test]
    fn test_init_args_default() {
        let args = InitArgs { force: false };
        assert!(!args.force);
    }

    #[test]
    fn test_init_args_force() {
        let args = InitArgs { force: true };
        assert!(args.force);
    }

    #[test]
    fn test_capture_args_stdin() {
        let args = CaptureArgs {
            stdin: true,
            file: None,
            tool: None,
            prompt: None,
        };
        assert!(args.stdin);
        assert!(args.file.is_none());
    }

    #[test]
    fn test_capture_args_with_file() {
        let args = CaptureArgs {
            stdin: false,
            file: Some("test.rs".to_string()),
            tool: Some("Edit".to_string()),
            prompt: Some("Fix bug".to_string()),
        };
        assert!(!args.stdin);
        assert_eq!(args.file.as_deref(), Some("test.rs"));
        assert_eq!(args.tool.as_deref(), Some("Edit"));
        assert_eq!(args.prompt.as_deref(), Some("Fix bug"));
    }
}
