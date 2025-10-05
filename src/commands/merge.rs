use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use colored::Colorize;

use super::delete::handle_delete;
use crate::git::{
    ahead_behind, execute_git, get_current_branch, get_default_branch, is_working_tree_clean,
};
use crate::input::{get_command_arg, smart_confirm};
use crate::state::{WorktreeInfo, XlaudeState};
use crate::utils::execute_in_dir;

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum MergeStrategy {
    FfOnly,
    Merge,
    Squash,
}

pub fn handle_merge(
    name: Option<String>,
    push: bool,
    cleanup: bool,
    strategy: Option<MergeStrategy>,
    squash_flag: bool,
) -> Result<()> {
    let state = XlaudeState::load()?;
    let target_name = get_command_arg(name)?;
    let worktree_info = resolve_worktree(&state, target_name)?;

    if !worktree_info.path.exists() {
        bail!(
            "Worktree directory not found at {}. If it was removed manually, run 'agentdev worktree delete {}' to clean up state.",
            worktree_info.path.display(),
            worktree_info.name
        );
    }

    let main_repo_path = get_main_repo_path(&worktree_info)?;
    if !main_repo_path.exists() {
        bail!(
            "Main repository directory not found at {}.",
            main_repo_path.display()
        );
    }

    ensure_clean(&worktree_info.path, "worktree")
        .with_context(|| format!("Worktree '{}' has pending changes", worktree_info.name))?;
    ensure_clean(&main_repo_path, "main repository")
        .context("Main repository has pending changes")?;

    let resolved_strategy = resolve_strategy(strategy, squash_flag)?;

    println!(
        "{} Merging '{}' into default branch for '{}'.",
        "🔀".green(),
        worktree_info.branch.cyan(),
        worktree_info.repo_name.cyan()
    );

    fetch_origin(&main_repo_path)?;
    let default_branch = determine_default_branch(&main_repo_path)?;

    checkout_base_branch(&main_repo_path, &default_branch)?;
    update_base_branch(&main_repo_path, &default_branch)?;

    let outcome = merge_branch(
        &main_repo_path,
        &worktree_info,
        &default_branch,
        resolved_strategy,
    )?;

    if push {
        push_default_branch(&main_repo_path, &default_branch)?;
    }

    println!(
        "{} '{}' merged into '{}' successfully",
        "✅".green(),
        worktree_info.branch.cyan(),
        default_branch.cyan()
    );

    if let Some(message) = outcome.squash_commit_message {
        println!("  {} Created squash commit: {}", "ℹ️".blue(), message);
    }

    if !push {
        println!(
            "  {} Run `git push origin {}` to publish the merge",
            "ℹ️".blue(),
            default_branch
        );
    }

    let delete_now = smart_confirm(
        &format!("Delete worktree '{}' now?", worktree_info.name),
        cleanup,
    )?;

    if delete_now {
        handle_delete(Some(worktree_info.name.clone()))?;
    } else {
        println!(
            "  {} Run `agentdev worktree delete {}` to clean up the worktree",
            "ℹ️".blue(),
            worktree_info.name
        );
    }

    Ok(())
}

fn resolve_worktree(state: &XlaudeState, target_name: Option<String>) -> Result<WorktreeInfo> {
    if let Some(name) = target_name {
        state
            .worktrees
            .values()
            .find(|info| info.name == name)
            .cloned()
            .with_context(|| format!("Worktree '{}' not found", name))
    } else {
        resolve_current_worktree(state)
    }
}

fn resolve_current_worktree(state: &XlaudeState) -> Result<WorktreeInfo> {
    let current_dir = std::env::current_dir()?;
    let dir_name = current_dir
        .file_name()
        .and_then(|n| n.to_str())
        .context("Failed to get current directory name")?;

    state
        .worktrees
        .values()
        .find(|info| info.path.file_name().and_then(|n| n.to_str()) == Some(dir_name))
        .cloned()
        .context("Current directory is not a managed worktree")
}

fn resolve_strategy(strategy: Option<MergeStrategy>, squash_flag: bool) -> Result<MergeStrategy> {
    if squash_flag {
        if let Some(s) = strategy {
            if s != MergeStrategy::Squash {
                bail!("--squash conflicts with --strategy {:?}", s);
            }
        }
        return Ok(MergeStrategy::Squash);
    }

    Ok(strategy.unwrap_or(MergeStrategy::FfOnly))
}

fn ensure_clean(path: &Path, label: &str) -> Result<()> {
    execute_in_dir(path, || {
        if is_working_tree_clean()? {
            Ok(())
        } else {
            bail!("{} at {} has uncommitted changes", label, path.display());
        }
    })
}

fn fetch_origin(main_repo_path: &Path) -> Result<()> {
    execute_in_dir(main_repo_path, || {
        println!("  {} Fetching origin", "→".blue());
        execute_git(&["fetch", "origin"])?;
        Ok(())
    })
}

fn determine_default_branch(main_repo_path: &Path) -> Result<String> {
    execute_in_dir(main_repo_path, || {
        let branch = get_default_branch()?;
        println!(
            "  {} Default branch detected: {}",
            "→".blue(),
            branch.cyan()
        );
        Ok(branch)
    })
}

fn checkout_base_branch(main_repo_path: &Path, default_branch: &str) -> Result<()> {
    execute_in_dir(main_repo_path, || {
        let current = get_current_branch()?;
        if current != default_branch {
            println!("  {} Checking out {}", "→".blue(), default_branch.cyan());
            execute_git(&["checkout", default_branch])?;
        }
        Ok(())
    })
}

fn update_base_branch(main_repo_path: &Path, default_branch: &str) -> Result<()> {
    execute_in_dir(main_repo_path, || {
        let upstream_ref = format!("origin/{}", default_branch);
        let counts = ahead_behind(default_branch, &upstream_ref)?;

        match (counts.behind, counts.ahead) {
            (0, a) if a > 0 => {
                println!(
                    "  {} Local {} ahead of origin; skipping pull",
                    "ℹ️".blue(),
                    default_branch.cyan()
                );
                Ok(())
            }
            (0, _) => {
                println!(
                    "  {} {} already up to date with origin",
                    "ℹ️".blue(),
                    default_branch.cyan()
                );
                Ok(())
            }
            (b, 0) if b > 0 => {
                println!("  {} Pulling latest {}", "→".blue(), default_branch.cyan());
                execute_git(&["pull", "--ff-only", "origin", default_branch])?;
                Ok(())
            }
            _ => bail!(
                "Local {default_branch} and {upstream_ref} have diverged. Resolve manually before retrying."
            ),
        }
    })
}

struct MergeOutcome {
    squash_commit_message: Option<String>,
}

fn merge_branch(
    main_repo_path: &Path,
    worktree_info: &WorktreeInfo,
    default_branch: &str,
    strategy: MergeStrategy,
) -> Result<MergeOutcome> {
    let branch = worktree_info.branch.clone();
    let merge_result = execute_in_dir(main_repo_path, || match strategy {
        MergeStrategy::FfOnly => {
            println!("  {} Fast-forward merging {}", "→".blue(), branch.cyan());
            execute_git(&["merge", "--ff-only", &branch])?;
            Ok(MergeOutcome {
                squash_commit_message: None,
            })
        }
        MergeStrategy::Merge => {
            println!("  {} Merging {}", "→".blue(), branch.cyan());
            execute_git(&["merge", "--no-ff", &branch])?;
            Ok(MergeOutcome {
                squash_commit_message: None,
            })
        }
        MergeStrategy::Squash => {
            println!("  {} Squash merging {}", "→".blue(), branch.cyan());
            execute_git(&["merge", "--squash", &branch])?;

            let staged = execute_git(&["diff", "--cached", "--name-only"])?;
            if staged.trim().is_empty() {
                bail!(
                    "Squash merge produced no staged changes. Branch '{}' may already be merged into '{}'",
                    branch,
                    default_branch
                );
            }

            let commit_message = format!("Squash merge {} into {}", branch, default_branch);
            execute_git(&["commit", "-m", &commit_message])?;

            Ok(MergeOutcome {
                squash_commit_message: Some(commit_message),
            })
        }
    });

    match merge_result {
        Ok(outcome) => Ok(outcome),
        Err(err) => {
            if strategy == MergeStrategy::FfOnly
                && err
                    .chain()
                    .any(|cause| cause.to_string().contains("Not possible to fast-forward"))
            {
                bail!(
                    "Fast-forward merge failed. Rebase '{}' onto '{}' or rerun with '--strategy merge'.",
                    branch,
                    default_branch
                );
            }
            Err(err)
        }
    }
}

fn push_default_branch(main_repo_path: &Path, default_branch: &str) -> Result<()> {
    execute_in_dir(main_repo_path, || {
        println!(
            "  {} Pushing {} to origin",
            "→".blue(),
            default_branch.cyan()
        );
        execute_git(&["push", "origin", default_branch])?;
        Ok(())
    })
}

fn get_main_repo_path(worktree_info: &WorktreeInfo) -> Result<PathBuf> {
    worktree_info
        .path
        .parent()
        .map(|parent| parent.join(&worktree_info.repo_name))
        .context("Failed to resolve main repository path")
}
