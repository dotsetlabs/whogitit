pub mod blame;
pub mod output;
pub mod prompt;
pub mod show;
pub mod summary;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::capture::hook;

/// AI-aware git blame tool for tracking AI-generated code
#[derive(Debug, Parser)]
#[command(name = "ai-blame")]
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

    /// Capture a file change (called by Claude Code hook)
    Capture(CaptureArgs),

    /// Finalize attribution after a commit (post-commit hook)
    PostCommit,

    /// Show pending changes status
    Status,

    /// Clear pending changes without committing
    Clear,

    /// Initialize ai-blame in a git repository (installs post-commit hook)
    Init,
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
        Commands::Capture(args) => run_capture(args),
        Commands::PostCommit => run_post_commit(),
        Commands::Status => run_status(),
        Commands::Clear => run_clear(),
        Commands::Init => run_init(),
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
    let repo_root = repo.workdir()
        .ok_or_else(|| anyhow::anyhow!("No working directory"))?;

    let hook_handler = crate::capture::CaptureHook::new(repo_root)?;
    let status = hook_handler.status()?;

    if status.has_pending {
        println!("Pending AI attribution:");
        println!("  Session: {}", status.session_id.as_deref().unwrap_or("unknown"));
        println!("  Files: {}", status.file_count);
        println!("  Edits: {}", status.edit_count);
        println!("  Lines: {}", status.line_count);
        if !status.age.is_empty() {
            println!("  Age: {}", status.age);
        }

        if status.is_stale {
            println!("\n⚠️  Warning: This pending buffer is stale (> 24 hours old).");
            println!("   Run 'ai-blame clear' if these changes are no longer relevant.");
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
    let repo_root = repo.workdir()
        .ok_or_else(|| anyhow::anyhow!("No working directory"))?;

    let hook_handler = crate::capture::CaptureHook::new(repo_root)?;
    hook_handler.clear_pending()?;

    println!("Cleared pending AI attribution.");

    Ok(())
}

fn run_init() -> Result<()> {
    let repo = git2::Repository::discover(".")
        .context("Not in a git repository")?;
    let repo_root = repo.workdir()
        .ok_or_else(|| anyhow::anyhow!("No working directory"))?;

    let hooks_dir = repo_root.join(".git/hooks");
    fs::create_dir_all(&hooks_dir)
        .context("Failed to create hooks directory")?;

    // Install post-commit hook (attaches attribution to commits)
    install_post_commit_hook(&hooks_dir)?;

    // Install pre-push hook (auto-pushes notes with regular git push)
    install_pre_push_hook(&hooks_dir)?;

    // Configure git to auto-fetch notes
    configure_git_fetch(&repo)?;

    println!("\nSetup complete! AI attribution will be tracked for commits in this repo.");
    println!("Notes will be automatically pushed with 'git push' and fetched with 'git fetch'.");
    println!("\nMake sure Claude Code hooks are configured in ~/.claude/settings.json");

    Ok(())
}

fn install_post_commit_hook(hooks_dir: &std::path::Path) -> Result<()> {
    let hook_path = hooks_dir.join("post-commit");

    if hook_path.exists() {
        let content = fs::read_to_string(&hook_path)?;
        if content.contains("ai-blame") {
            println!("✓ ai-blame post-commit hook already installed.");
            return Ok(());
        }

        // Append to existing hook
        let new_content = format!(
            "{}\n\n# ai-blame post-commit hook\nif command -v ai-blame &> /dev/null; then\n    ai-blame post-commit 2>/dev/null || true\nfi\n",
            content.trim_end()
        );
        fs::write(&hook_path, new_content)?;
        println!("✓ Added ai-blame to existing post-commit hook.");
    } else {
        let hook_content = r#"#!/bin/bash
# ai-blame post-commit hook
# Attaches AI attribution notes to the commit

if command -v ai-blame &> /dev/null; then
    ai-blame post-commit 2>/dev/null || true
elif [[ -x "$HOME/.cargo/bin/ai-blame" ]]; then
    "$HOME/.cargo/bin/ai-blame" post-commit 2>/dev/null || true
fi
"#;
        fs::write(&hook_path, hook_content)?;
        make_executable(&hook_path)?;
        println!("✓ Installed ai-blame post-commit hook.");
    }

    Ok(())
}

fn install_pre_push_hook(hooks_dir: &std::path::Path) -> Result<()> {
    let hook_path = hooks_dir.join("pre-push");

    if hook_path.exists() {
        let content = fs::read_to_string(&hook_path)?;
        if content.contains("ai-blame") {
            println!("✓ ai-blame pre-push hook already installed.");
            return Ok(());
        }

        // Append to existing hook
        let new_content = format!(
            "{}\n\n# ai-blame pre-push hook - automatically push notes\n# Skip if already pushing notes (prevent recursion)\n[[ \"$AI_BLAME_PUSHING_NOTES\" == \"1\" ]] && exit 0\nremote=\"$1\"\nif git notes --ref=ai-blame list &>/dev/null; then\n    AI_BLAME_PUSHING_NOTES=1 git push \"$remote\" refs/notes/ai-blame 2>/dev/null || true\nfi\n",
            content.trim_end()
        );
        fs::write(&hook_path, new_content)?;
        println!("✓ Added ai-blame to existing pre-push hook.");
    } else {
        let hook_content = r#"#!/bin/bash
# ai-blame pre-push hook
# Automatically pushes ai-blame notes alongside regular pushes

# Prevent recursion - skip if we're already pushing notes
[[ "$AI_BLAME_PUSHING_NOTES" == "1" ]] && exit 0

remote="$1"

# Only push notes if they exist
if git notes --ref=ai-blame list &>/dev/null; then
    AI_BLAME_PUSHING_NOTES=1 git push "$remote" refs/notes/ai-blame 2>/dev/null || true
fi
"#;
        fs::write(&hook_path, hook_content)?;
        make_executable(&hook_path)?;
        println!("✓ Installed ai-blame pre-push hook.");
    }

    Ok(())
}

fn make_executable(path: &std::path::Path) -> Result<()> {
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)?;
    Ok(())
}

/// Configure git to automatically fetch ai-blame notes
fn configure_git_fetch(repo: &git2::Repository) -> Result<()> {
    let mut config = repo.config()
        .context("Failed to open git config")?;

    let fetch_refspec = "+refs/notes/ai-blame:refs/notes/ai-blame";
    let fetch_configured = config
        .get_string("remote.origin.fetch")
        .map(|v| v.contains("ai-blame"))
        .unwrap_or(false);

    if !fetch_configured {
        config.set_multivar("remote.origin.fetch", "^$", fetch_refspec)
            .or_else(|_| {
                config.set_str("remote.origin.fetch", fetch_refspec)
            })
            .context("Failed to configure fetch refspec")?;
        println!("✓ Configured git to fetch ai-blame notes automatically.");
    } else {
        println!("✓ Git already configured to fetch ai-blame notes.");
    }

    Ok(())
}

