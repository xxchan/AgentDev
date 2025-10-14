use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use colored::Colorize;

use agentdev::process_registry::{
    MAX_PROCESSES_PER_WORKTREE, ProcessRecord, ProcessRegistry, ProcessStatus,
};

use crate::input::smart_select;
use crate::state::{WorktreeInfo, XlaudeState};

/// Execute an arbitrary command inside a managed worktree.
pub fn handle_exec(worktree_flag: Option<String>, mut raw_args: Vec<String>) -> Result<()> {
    if raw_args.is_empty() {
        bail!("Command to execute is required");
    }

    let state = XlaudeState::load()?;
    if state.worktrees.is_empty() {
        bail!("No worktrees found. Create one first with 'agentdev worktree create'");
    }

    // Allow '--worktree' flag to take priority and strip an implicit worktree argument
    // like `agentdev x exec feature-x pnpm dev`.
    let mut selected_worktree = worktree_flag;
    if selected_worktree.is_none() && raw_args.len() > 1 {
        if let Some(candidate) = raw_args.first() {
            if state.worktrees.values().any(|info| info.name == *candidate) {
                selected_worktree = Some(candidate.clone());
                raw_args.remove(0);
            }
        }
    }

    if raw_args.is_empty() {
        bail!("Command to execute is required");
    }

    let worktree = resolve_target_worktree(&state, selected_worktree)?;
    let command_tokens = normalize_command_tokens(&raw_args)?;

    let display_cmd = format_command(&command_tokens);
    println!(
        "{} Running {} in {}/{} ({})",
        "🚀".green(),
        display_cmd.cyan(),
        worktree.repo_name,
        worktree.name.cyan(),
        worktree.path.display()
    );

    let (program, args) = command_tokens
        .split_first()
        .context("Command tokens unexpectedly empty")?;

    let mut registry = ProcessRegistry::load()?;
    let worktree_key = XlaudeState::make_key(&worktree.repo_name, &worktree.name);
    let mut record = ProcessRecord::new(
        worktree_key,
        worktree.name.clone(),
        worktree.repo_name.clone(),
        command_tokens.clone(),
        Some(worktree.path.clone()),
        ProcessStatus::Running,
    );
    record.description = Some("Launched via agentdev worktree exec".to_string());
    let process_id = record.id.clone();
    registry.insert(record);
    registry.retain_recent(MAX_PROCESSES_PER_WORKTREE);
    registry
        .save()
        .context("Failed to persist process registry after launch")?;

    let status = Command::new(program)
        .args(args)
        .current_dir(&worktree.path)
        .output();

    match status {
        Ok(output) => {
            let stdout_text = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr_text = String::from_utf8_lossy(&output.stderr).to_string();

            if !stdout_text.is_empty() {
                print!("{stdout_text}");
            }
            if !stderr_text.is_empty() {
                eprint!("{stderr_text}");
            }

            let stdout_option = if stdout_text.is_empty() {
                None
            } else {
                Some(stdout_text)
            };
            let stderr_option = if stderr_text.is_empty() {
                None
            } else {
                Some(stderr_text)
            };

            let outcome = if output.status.success() {
                ProcessStatus::Succeeded
            } else {
                ProcessStatus::Failed
            };
            registry.update(&process_id, |record| {
                record.mark_finished(
                    outcome,
                    output.status.code(),
                    None,
                    stdout_option.clone(),
                    stderr_option.clone(),
                );
            })?;
            registry.retain_recent(MAX_PROCESSES_PER_WORKTREE);
            registry
                .save()
                .context("Failed to persist process registry after completion")?;

            if !output.status.success() {
                if let Some(code) = output.status.code() {
                    bail!("Command exited with status {code}");
                } else {
                    bail!("Command terminated by signal");
                }
            }
            Ok(())
        }
        Err(err) => {
            let error_message = format!("Failed to spawn '{program}': {err}");
            registry.update(&process_id, |record| {
                record.mark_finished(
                    ProcessStatus::Failed,
                    None,
                    Some(error_message.clone()),
                    None,
                    None,
                );
            })?;
            registry.retain_recent(MAX_PROCESSES_PER_WORKTREE);
            registry
                .save()
                .context("Failed to persist process registry after spawn error")?;
            Err(err).with_context(|| format!("Failed to spawn '{program}'"))
        }
    }
}

fn resolve_target_worktree(state: &XlaudeState, explicit: Option<String>) -> Result<WorktreeInfo> {
    if let Some(name) = explicit {
        return state
            .worktrees
            .values()
            .find(|info| info.name == name)
            .cloned()
            .with_context(|| format!("Worktree '{name}' not found"));
    }

    if let Some(info) = find_worktree_by_path(state, &std::env::current_dir()?) {
        return Ok(info);
    }

    let worktree_list = state.prioritized_worktree_list();

    let selection = smart_select("Select a worktree", &worktree_list, |(_, info)| {
        format!("{}/{}", info.repo_name, info.name)
    })?;

    match selection {
        Some(idx) => Ok(worktree_list[idx].1.clone()),
        None => bail!(
            "Interactive selection not available. Please specify a worktree using '--worktree <name>'."
        ),
    }
}

fn find_worktree_by_path(state: &XlaudeState, path: &Path) -> Option<WorktreeInfo> {
    let target = canonicalize_lossy(path);
    state
        .worktrees
        .values()
        .find(|info| canonicalize_lossy(&info.path) == target)
        .cloned()
}

fn canonicalize_lossy(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn normalize_command_tokens(raw: &[String]) -> Result<Vec<String>> {
    if raw.len() == 1 {
        let parsed = shell_words::split(&raw[0])
            .map_err(|e| anyhow::anyhow!("Invalid command string '{}': {e}", raw[0]))?;
        if parsed.is_empty() {
            bail!("Command to execute is required");
        }
        Ok(parsed)
    } else {
        Ok(raw.to_vec())
    }
}

fn format_command(tokens: &[String]) -> String {
    tokens
        .iter()
        .map(|t| shell_words::quote(t))
        .collect::<Vec<_>>()
        .join(" ")
}
