use anyhow::{Context, Result};
use colored::Colorize;
use std::io;
use std::path::PathBuf;

use crate::input::{get_command_arg, smart_confirm};
use agentdev::discovery::GitWorktree;
use agentdev::git::{execute_git, has_unpushed_commits, is_working_tree_clean};
use agentdev::state::XlaudeState;
use agentdev::tmux::TmuxManager;
use agentdev::utils::execute_in_dir;

/// Represents the result of various checks performed before deletion
struct DeletionChecks {
    has_uncommitted_changes: bool,
    has_unpushed_commits: bool,
    branch_merged_via_git: bool,
    branch_merged_via_pr: bool,
}

impl DeletionChecks {
    fn branch_is_merged(&self) -> bool {
        self.branch_merged_via_git || self.branch_merged_via_pr
    }

    fn has_pending_work(&self) -> bool {
        self.has_uncommitted_changes || self.has_unpushed_commits
    }
}

/// Configuration for deletion behavior
struct DeletionConfig {
    is_interactive: bool,
    worktree_exists: bool,
    is_current_directory: bool,
}

impl DeletionConfig {
    fn from_git_worktree(git_wt: &GitWorktree) -> Result<Self> {
        let current_dir = std::env::current_dir()?;

        Ok(Self {
            is_interactive: std::env::var("XLAUDE_NON_INTERACTIVE").is_err(),
            worktree_exists: git_wt.path.exists(),
            is_current_directory: current_dir == git_wt.path,
        })
    }
}

pub fn handle_delete(name: Option<String>) -> Result<()> {
    let state = XlaudeState::load()?;

    // Get name from CLI args or pipe
    let target_name = get_command_arg(name)?;

    // Resolve worktree - returns GitWorktree (from git) and optional state_key
    let (state_key, git_wt) = resolve_worktree_for_delete(&state, target_name)?;
    let config = DeletionConfig::from_git_worktree(&git_wt)?;

    let display_name = git_wt.display_name();
    println!(
        "{} Checking worktree '{}'...",
        "🔍".yellow(),
        display_name.cyan()
    );

    // Proactively stop tmux session for this worktree if running
    let tmux = TmuxManager::new();
    let _ = tmux.kill_session(&display_name);

    // Handle case where worktree directory doesn't exist
    if !config.worktree_exists {
        if !handle_missing_worktree(&git_wt)? {
            println!("{} Cancelled", "❌".red());
            return Ok(());
        }
    } else {
        // Check branch status first (for output consistency)
        let branch_display = git_wt.branch.as_deref().unwrap_or("(detached)");
        println!(
            "{} Checking branch '{}'...",
            "🔍".yellow(),
            branch_display
        );

        // Perform deletion checks
        let checks = perform_deletion_checks(&git_wt)?;

        if !confirm_deletion(&git_wt, &checks, &config)? {
            println!("{} Cancelled", "❌".red());
            return Ok(());
        }
    }

    // Execute deletion
    perform_deletion(&git_wt, &config)?;

    // Update state only if this worktree was managed
    if let Some(key) = state_key {
        let mut state = XlaudeState::load()?;
        state.worktrees.remove(&key);
        state.save()?;
    }

    println!(
        "{} Worktree '{}' deleted successfully",
        "✅".green(),
        display_name.cyan()
    );
    Ok(())
}

/// Resolve worktree for deletion.
///
/// Returns `(Option<state_key>, GitWorktree)`:
/// - state_key is Some if the worktree is managed by agentdev (for cleanup)
/// - GitWorktree contains core info from git
fn resolve_worktree_for_delete(
    state: &XlaudeState,
    name: Option<String>,
) -> Result<(Option<String>, GitWorktree)> {
    if let Some(n) = name {
        // By name: only works for managed worktrees
        let (key, info) = state
            .worktrees
            .iter()
            .find(|(_, w)| w.name == n)
            .map(|(k, w)| (k.clone(), w.clone()))
            .context(format!("Worktree '{}' not found in agentdev state", n))?;

        // Build GitWorktree from the managed path
        let git_wt = GitWorktree::from_path(&info.path)?
            .ok_or_else(|| anyhow::anyhow!(
                "Path '{}' is not a git worktree",
                info.path.display()
            ))?;

        Ok((Some(key), git_wt))
    } else {
        // No name: try current directory
        let git_wt = GitWorktree::from_current_dir()?
            .ok_or_else(|| anyhow::anyhow!(
                "Current directory is not a git worktree. \
                 If you're in the main repository, specify the worktree name."
            ))?;

        // Try to find matching state entry
        let state_key = find_state_key_by_path(state, &git_wt.path);

        Ok((state_key, git_wt))
    }
}

/// Find the state key for a worktree by its path
fn find_state_key_by_path(state: &XlaudeState, path: &PathBuf) -> Option<String> {
    let path_canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());

    state
        .worktrees
        .iter()
        .find(|(_, w)| {
            let w_canon = std::fs::canonicalize(&w.path).unwrap_or_else(|_| w.path.clone());
            w_canon == path_canon
        })
        .map(|(k, _)| k.clone())
}

/// Handle the case where worktree directory doesn't exist
fn handle_missing_worktree(git_wt: &GitWorktree) -> Result<bool> {
    println!(
        "{} Worktree directory not found at {}",
        "⚠️ ".yellow(),
        git_wt.path.display()
    );
    println!(
        "  {} The worktree may have been manually deleted",
        "ℹ️".blue()
    );

    smart_confirm("Remove this worktree?", true)
}

/// Perform all checks needed before deletion
fn perform_deletion_checks(git_wt: &GitWorktree) -> Result<DeletionChecks> {
    execute_in_dir(&git_wt.path, || {
        let has_uncommitted_changes = !is_working_tree_clean()?;
        let has_unpushed_commits = has_unpushed_commits();

        // Check branch merge status in main repo
        let branch = git_wt.branch.as_deref().unwrap_or("");
        let (branch_merged_via_git, branch_merged_via_pr) =
            check_branch_merge_status(&git_wt.repo_root, branch)?;

        Ok(DeletionChecks {
            has_uncommitted_changes,
            has_unpushed_commits,
            branch_merged_via_git,
            branch_merged_via_pr,
        })
    })
}

/// Check if branch is merged via git or PR
fn check_branch_merge_status(
    main_repo_path: &std::path::Path,
    branch: &str,
) -> Result<(bool, bool)> {
    execute_in_dir(main_repo_path, || {
        // Check traditional git merge (use our git wrapper to capture logs)
        let merged_branches = execute_git(["branch", "--merged"].as_slice())
            .context("Failed to check merged branches")?;
        let is_merged_git = merged_branches
            .lines()
            .any(|line| line.trim().trim_start_matches('*').trim() == branch);

        // Check if merged via PR (works for squash merge)
        let is_merged_pr = check_branch_merged_via_pr(branch);

        Ok((is_merged_git, is_merged_pr))
    })
}

/// Check if branch was merged via GitHub PR
fn check_branch_merged_via_pr(branch: &str) -> bool {
    std::process::Command::new("gh")
        .args([
            "pr", "list", "--state", "merged", "--head", branch, "--json", "number",
        ])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|json| serde_json::from_str::<Vec<serde_json::Value>>(&json).ok())
        .map(|prs| !prs.is_empty())
        .unwrap_or(false)
}

/// Confirm deletion with the user based on checks
fn confirm_deletion(
    git_wt: &GitWorktree,
    checks: &DeletionChecks,
    _config: &DeletionConfig,
) -> Result<bool> {
    // Show warnings for pending work
    if checks.has_pending_work() {
        show_pending_work_warnings(checks);

        return smart_confirm("Are you sure you want to delete this worktree?", false);
    }

    // Show branch merge status
    if !checks.branch_is_merged() {
        show_unmerged_branch_warning(git_wt);
    } else if checks.branch_merged_via_pr && !checks.branch_merged_via_git {
        println!("  {} Branch was merged via PR", "ℹ️".blue());
    }

    // Ask for confirmation
    smart_confirm(&format!("Delete worktree '{}'?", git_wt.display_name()), true)
}

/// Show warnings for uncommitted changes or unpushed commits
fn show_pending_work_warnings(checks: &DeletionChecks) {
    println!();
    if checks.has_uncommitted_changes {
        println!("{} You have uncommitted changes", "⚠️ ".red());
    }
    if checks.has_unpushed_commits {
        println!("{} You have unpushed commits", "⚠️ ".red());
    }
}

/// Show warning for unmerged branch
fn show_unmerged_branch_warning(git_wt: &GitWorktree) {
    let branch = git_wt.branch.as_deref().unwrap_or("(unknown)");
    println!(
        "{} Branch '{}' is not fully merged",
        "⚠️ ".yellow(),
        branch.cyan()
    );
    println!("  {} No merged PR found for this branch", "ℹ️".blue());
}

/// Perform the actual deletion of worktree and branch
fn perform_deletion(git_wt: &GitWorktree, config: &DeletionConfig) -> Result<()> {
    // Change to main repo if we're deleting current directory
    if config.is_current_directory {
        std::env::set_current_dir(&git_wt.repo_root)
            .context("Failed to change to main repository")?;
    }

    execute_in_dir(&git_wt.repo_root, || {
        // Remove or prune worktree
        remove_worktree(git_wt, config)?;

        // Delete branch
        delete_branch(git_wt, config)?;

        Ok(())
    })
}

/// Remove the worktree from git
fn remove_worktree(git_wt: &GitWorktree, config: &DeletionConfig) -> Result<()> {
    if config.worktree_exists {
        println!("{} Removing worktree...", "🗑️ ".yellow());

        let path_str = git_wt.path.to_str().context("Path contains invalid UTF-8")?;

        match execute_git(&["worktree", "remove", path_str]) {
            Ok(_) => {}
            Err(err) if is_not_worktree_error(&err) => {
                cleanup_stale_worktree(git_wt)?;
            }
            Err(_) => {
                println!(
                    "{} Standard removal failed, trying force removal...",
                    "⚠️ ".yellow()
                );
                match execute_git(&["worktree", "remove", "--force", path_str]) {
                    Ok(_) => {}
                    Err(force_err) if is_not_worktree_error(&force_err) => {
                        cleanup_stale_worktree(git_wt)?;
                    }
                    Err(force_err) => {
                        return Err(force_err).context("Failed to force remove worktree");
                    }
                }
            }
        }
    } else {
        println!("{} Pruning non-existent worktree...", "🗑️ ".yellow());
        execute_git(&["worktree", "prune"]).context("Failed to prune worktree")?;
    }
    Ok(())
}

fn is_not_worktree_error(err: &anyhow::Error) -> bool {
    err.to_string().contains("is not a working tree")
}

fn cleanup_stale_worktree(git_wt: &GitWorktree) -> Result<()> {
    println!(
        "{} Git no longer recognizes this directory as a worktree; cleaning up stale state",
        "ℹ️".blue()
    );

    if git_wt.path.exists() {
        match std::fs::remove_dir_all(&git_wt.path) {
            Ok(_) => {}
            Err(fs_err) if fs_err.kind() == io::ErrorKind::NotFound => {}
            Err(fs_err) => {
                return Err(fs_err).with_context(|| {
                    format!(
                        "Failed to remove stale worktree directory at {}",
                        git_wt.path.display()
                    )
                });
            }
        }
    }

    execute_git(&["worktree", "prune"]).context("Failed to prune stale worktree entries")?;
    Ok(())
}

/// Delete the branch from git
fn delete_branch(git_wt: &GitWorktree, config: &DeletionConfig) -> Result<()> {
    let Some(branch) = &git_wt.branch else {
        println!("{} No branch to delete (detached HEAD)", "ℹ️ ".blue());
        return Ok(());
    };

    println!(
        "{} Deleting branch '{}'...",
        "🗑️ ".yellow(),
        branch
    );

    // First try safe delete
    if execute_git(&["branch", "-d", branch]).is_ok() {
        println!("{} Branch deleted", "✅".green());
        return Ok(());
    }

    // Branch is not fully merged, ask for force delete
    if !config.is_interactive {
        println!("{} Branch kept (not fully merged)", "ℹ️ ".blue());
        return Ok(());
    }

    let force_delete = smart_confirm("Branch is not fully merged. Force delete?", false)?;

    if force_delete {
        execute_git(&["branch", "-D", branch])
            .context("Failed to force delete branch")?;
        println!("{} Branch force deleted", "✅".green());
    } else {
        println!("{} Branch kept", "ℹ️ ".blue());
    }

    Ok(())
}
