use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use colored::Colorize;

use super::delete::handle_delete;
use crate::input::{get_command_arg, smart_confirm, smart_select};
use agentdev::git::{
    ahead_behind, execute_git, get_current_branch, get_default_branch, is_working_tree_clean,
};
use agentdev::state::{WorktreeInfo, XlaudeState};
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
    let current_dir = std::env::current_dir()?;

    if let Some(name) = target_name {
        if let Some(info) = state
            .worktrees
            .values()
            .find(|info| info.name == name)
            .cloned()
        {
            return Ok(info);
        }

        if let Some(info) = resolve_selection_from_input(state, &current_dir, &name)? {
            return Ok(info);
        }

        bail!("Worktree '{}' not found", name);
    }

    if let Some(info) = find_worktree_for_dir(state, &current_dir)? {
        return Ok(info);
    }

    match select_worktree_for_repo(state, &current_dir)? {
        WorktreeSelection::Selected(info) => Ok(info),
        WorktreeSelection::NoWorktrees => {
            bail!(
                "No managed worktrees found for the current repository. Provide a worktree name or create one before merging."
            );
        }
        WorktreeSelection::Cancelled => {
            bail!(
                "No worktree selected. Provide a worktree name or run the command from a worktree directory."
            );
        }
    }
}

fn find_worktree_for_dir(state: &XlaudeState, current_dir: &Path) -> Result<Option<WorktreeInfo>> {
    let dir_name = match current_dir.file_name().and_then(|n| n.to_str()) {
        Some(name) => name,
        None => return Ok(None),
    };

    let dir_canon = current_dir
        .canonicalize()
        .unwrap_or_else(|_| current_dir.to_path_buf());

    let worktree = state.worktrees.values().find_map(|info| {
        let Some(info_name) = info.path.file_name().and_then(|n| n.to_str()) else {
            return None;
        };
        if info_name != dir_name {
            return None;
        }

        let info_canon = info
            .path
            .canonicalize()
            .unwrap_or_else(|_| info.path.clone());
        if info_canon == dir_canon {
            Some(info.clone())
        } else {
            None
        }
    });

    Ok(worktree)
}

fn select_worktree_for_repo(state: &XlaudeState, current_dir: &Path) -> Result<WorktreeSelection> {
    let (repo_name, mut candidates) = worktrees_for_repo(state, current_dir)?;

    if candidates.is_empty() {
        return Ok(WorktreeSelection::NoWorktrees);
    }

    if candidates.len() == 1 {
        return Ok(WorktreeSelection::Selected(candidates.remove(0)));
    }

    let repo_display = repo_name.unwrap_or_else(|| current_dir.display().to_string());
    let prompt = format!("Select worktree to merge for repo '{}'", repo_display);
    match smart_select(&prompt, &candidates, |info| {
        format!("{} [{}]", info.name, info.branch)
    })? {
        Some(index) => Ok(WorktreeSelection::Selected(
            candidates
                .get(index)
                .cloned()
                .expect("selection index must be valid"),
        )),
        None => Ok(WorktreeSelection::Cancelled),
    }
}

fn resolve_selection_from_input(
    state: &XlaudeState,
    current_dir: &Path,
    raw_input: &str,
) -> Result<Option<WorktreeInfo>> {
    let (_, candidates) = worktrees_for_repo(state, current_dir)?;
    if candidates.is_empty() {
        return Ok(None);
    }

    let input = raw_input.trim();

    if let Ok(index) = input.parse::<usize>() {
        return Ok(candidates.get(index).cloned());
    }

    for info in &candidates {
        let display = format!("{} [{}]", info.name, info.branch);
        if display == input {
            return Ok(Some(info.clone()));
        }
    }

    Ok(None)
}

fn worktrees_for_repo(
    state: &XlaudeState,
    current_dir: &Path,
) -> Result<(Option<String>, Vec<WorktreeInfo>)> {
    let repo_name = current_dir
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string());

    let repo_canon = current_dir
        .canonicalize()
        .unwrap_or_else(|_| current_dir.to_path_buf());

    let mut candidates: Vec<WorktreeInfo> = state
        .worktrees
        .values()
        .filter_map(|info| {
            let parent = info.path.parent()?;
            let repo_path = parent.join(&info.repo_name);
            let repo_path_canon = repo_path.canonicalize().unwrap_or(repo_path.clone());
            if repo_path_canon == repo_canon {
                Some(info.clone())
            } else {
                None
            }
        })
        .collect();

    candidates.sort_by(|a, b| a.name.cmp(&b.name));

    Ok((repo_name, candidates))
}

enum WorktreeSelection {
    Selected(WorktreeInfo),
    NoWorktrees,
    Cancelled,
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

fn get_main_repo_path(worktree_info: &WorktreeInfo) -> Result<PathBuf> {
    worktree_info
        .path
        .parent()
        .map(|parent| parent.join(&worktree_info.repo_name))
        .context("Failed to resolve main repository path")
}
