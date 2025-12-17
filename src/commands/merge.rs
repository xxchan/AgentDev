use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use colored::Colorize;

use super::delete::handle_delete;
use crate::input::{get_command_arg, smart_confirm};
use agentdev::discovery::GitWorktree;
use agentdev::git::{
    ahead_behind, execute_git, get_current_branch, get_default_branch, is_working_tree_clean,
};
use agentdev::state::XlaudeState;
use agentdev::utils::execute_in_dir;

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

    // Resolve worktree - returns GitWorktree (from git) and optional managed name
    let (git_wt, managed_name) = resolve_worktree_for_merge(&state, target_name)?;

    if !git_wt.path.exists() {
        bail!(
            "Worktree directory not found at {}.",
            git_wt.path.display()
        );
    }

    if !git_wt.repo_root.exists() {
        bail!(
            "Main repository directory not found at {}.",
            git_wt.repo_root.display()
        );
    }

    let branch = git_wt.branch.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Cannot merge: worktree is in detached HEAD state")
    })?;

    ensure_clean(&git_wt.path, "worktree")
        .with_context(|| format!("Worktree '{}' has pending changes", git_wt.display_name()))?;
    ensure_clean(&git_wt.repo_root, "main repository")
        .context("Main repository has pending changes")?;

    let resolved_strategy = resolve_strategy(strategy, squash_flag)?;

    println!(
        "{} Merging '{}' into default branch for '{}'.",
        "🔀".green(),
        branch.cyan(),
        git_wt.repo_name().cyan()
    );

    fetch_origin(&git_wt.repo_root)?;
    let default_branch = determine_default_branch(&git_wt.repo_root)?;

    checkout_base_branch(&git_wt.repo_root, &default_branch)?;
    update_base_branch(&git_wt.repo_root, &default_branch)?;

    let outcome = merge_branch(
        &git_wt.repo_root,
        branch,
        &default_branch,
        resolved_strategy,
    )?;

    if push {
        push_default_branch(&git_wt.repo_root, &default_branch)?;
    }

    println!(
        "{} '{}' merged into '{}' successfully",
        "✅".green(),
        branch.cyan(),
        default_branch.cyan()
    );

    if let Some(subject) = outcome.squash_commit_subject {
        println!("  {} Created squash commit: {}", "ℹ️".blue(), subject);
        if let Some(detail) = outcome.squash_detail {
            if detail != subject {
                println!("    {}", detail);
            }
        }
    }

    if !push {
        println!(
            "  {} Run `git push origin {}` to publish the merge",
            "ℹ️".blue(),
            default_branch
        );
    }

    // For cleanup, use managed name if available, otherwise use None (delete from current dir)
    let display_name = managed_name.clone().unwrap_or_else(|| git_wt.display_name());
    let delete_now = smart_confirm(
        &format!("Delete worktree '{}' now?", display_name),
        cleanup,
    )?;

    if delete_now {
        // Pass managed name if available, None otherwise (delete will use current dir)
        handle_delete(managed_name)?;
    } else {
        println!(
            "  {} Run `agentdev worktree delete` to clean up the worktree",
            "ℹ️".blue()
        );
    }

    Ok(())
}

/// Resolve worktree for merge operation.
///
/// Returns `(GitWorktree, Option<managed_name>)`:
/// - GitWorktree contains core info from git
/// - managed_name is Some if the worktree is in agentdev state (for cleanup)
fn resolve_worktree_for_merge(
    state: &XlaudeState,
    target_name: Option<String>,
) -> Result<(GitWorktree, Option<String>)> {
    if let Some(name) = target_name {
        // By name: only works for managed worktrees
        let info = state
            .worktrees
            .values()
            .find(|info| info.name == name)
            .cloned()
            .context(format!("Worktree '{}' not found in agentdev state", name))?;

        // Build GitWorktree from the managed path
        let git_wt = GitWorktree::from_path(&info.path)?
            .ok_or_else(|| anyhow::anyhow!(
                "Path '{}' is not a git worktree",
                info.path.display()
            ))?;

        return Ok((git_wt, Some(info.name)));
    }

    // No name: try current directory
    let git_wt = GitWorktree::from_current_dir()?
        .ok_or_else(|| anyhow::anyhow!(
            "Current directory is not a git worktree. \
             If you're in the main repository, specify the worktree name."
        ))?;

    // Try to find matching state entry for managed name
    let managed_name = find_managed_name_by_path(state, &git_wt.path);

    Ok((git_wt, managed_name))
}

/// Find the managed worktree name for a path
fn find_managed_name_by_path(state: &XlaudeState, path: &std::path::Path) -> Option<String> {
    let path_canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

    state
        .worktrees
        .values()
        .find(|w| {
            let w_canon = std::fs::canonicalize(&w.path).unwrap_or_else(|_| w.path.clone());
            w_canon == path_canon
        })
        .map(|w| w.name.clone())
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
    squash_commit_subject: Option<String>,
    squash_detail: Option<String>,
}

struct CommitMessageParts {
    subject: String,
    body: Option<String>,
    detail: Option<String>,
}

fn merge_branch(
    main_repo_path: &Path,
    branch: &str,
    default_branch: &str,
    strategy: MergeStrategy,
) -> Result<MergeOutcome> {
    let branch = branch.to_string();
    let merge_result = execute_in_dir(main_repo_path, || match strategy {
        MergeStrategy::FfOnly => {
            println!("  {} Fast-forward merging {}", "→".blue(), branch.cyan());
            execute_git(&["merge", "--ff-only", &branch])?;
            Ok(MergeOutcome {
                squash_commit_subject: None,
                squash_detail: None,
            })
        }
        MergeStrategy::Merge => {
            println!("  {} Merging {}", "→".blue(), branch.cyan());
            execute_git(&["merge", "--no-ff", &branch])?;
            Ok(MergeOutcome {
                squash_commit_subject: None,
                squash_detail: None,
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

            let message = build_squash_commit_message(&branch, default_branch)?;

            if let Some(body) = &message.body {
                execute_git(&["commit", "-m", &message.subject, "-m", body])?;
            } else {
                execute_git(&["commit", "-m", &message.subject])?;
            }

            Ok(MergeOutcome {
                squash_commit_subject: Some(message.subject),
                squash_detail: message.detail,
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

fn build_squash_commit_message(branch: &str, default_branch: &str) -> Result<CommitMessageParts> {
    let detail = format!("Squash merge {} into {}", branch, default_branch);

    let commit_count_output = execute_git(&[
        "rev-list",
        "--count",
        &format!("{}..{}", default_branch, branch),
    ])?;
    let commit_count: u64 = commit_count_output
        .trim()
        .parse()
        .context("Failed to parse commit count for squash merge")?;

    if commit_count == 1 {
        let raw_message = execute_git(&["log", "-1", "--pretty=%B", branch])?;
        let trimmed = raw_message.trim_end_matches(|c| c == '\n' || c == '\r');

        let mut parts = trimmed.splitn(2, '\n');
        let subject = parts.next().unwrap_or_default().trim().to_string();
        let remainder = parts.next().map(|s| s.to_string());

        let body = remainder
            .map(|mut s| {
                while s.starts_with('\n') {
                    s.remove(0);
                }
                s
            })
            .filter(|s| !s.is_empty());

        let merged_body = match body {
            Some(existing) if !existing.trim().is_empty() => {
                Some(format!("{existing}\n\n{detail}"))
            }
            _ => Some(detail.clone()),
        };

        return Ok(CommitMessageParts {
            subject,
            body: merged_body,
            detail: Some(detail),
        });
    }

    Ok(CommitMessageParts {
        subject: detail,
        body: None,
        detail: None,
    })
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
