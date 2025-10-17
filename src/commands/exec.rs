use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

use anyhow::{Context, Result, anyhow, bail};
use colored::Colorize;

use agentdev::process_registry::{
    MAX_PROCESSES_PER_WORKTREE, ProcessRecord, ProcessRegistry, ProcessStatus,
};

use crate::input::smart_select;
use agentdev::state::{WorktreeInfo, XlaudeState};

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
    let record_to_store = record.clone();
    ProcessRegistry::mutate(move |registry| {
        registry.insert(record_to_store);
        registry.retain_recent(MAX_PROCESSES_PER_WORKTREE);
        Ok(())
    })
    .context("Failed to persist process registry after launch")?;

    let spawn_result = Command::new(program)
        .args(args)
        .current_dir(&worktree.path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match spawn_result {
        Ok(child) => child,
        Err(err) => {
            let error_message = format!("Failed to spawn '{program}': {err}");
            ProcessRegistry::mutate(|registry| {
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
                Ok(())
            })
            .context("Failed to persist process registry after spawn error")?;
            return Err(err).with_context(|| format!("Failed to spawn '{program}'"));
        }
    };

    let stdout_handle = child.stdout.take().map(|mut stdout_pipe| {
        thread::spawn(move || -> Result<Vec<u8>> {
            let mut captured = Vec::new();
            let mut buffer = [0u8; 8192];
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            loop {
                let read = stdout_pipe.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                handle.write_all(&buffer[..read])?;
                handle.flush()?;
                captured.extend_from_slice(&buffer[..read]);
            }
            Ok(captured)
        })
    });

    let stderr_handle = child.stderr.take().map(|mut stderr_pipe| {
        thread::spawn(move || -> Result<Vec<u8>> {
            let mut captured = Vec::new();
            let mut buffer = [0u8; 8192];
            let stderr = io::stderr();
            let mut handle = stderr.lock();
            loop {
                let read = stderr_pipe.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                handle.write_all(&buffer[..read])?;
                handle.flush()?;
                captured.extend_from_slice(&buffer[..read]);
            }
            Ok(captured)
        })
    });

    let wait_result = child.wait();

    let stdout_bytes = match stdout_handle {
        Some(handle) => handle
            .join()
            .map_err(|_| anyhow!("Stdout capture thread panicked"))??,
        None => Vec::new(),
    };

    let stderr_bytes = match stderr_handle {
        Some(handle) => handle
            .join()
            .map_err(|_| anyhow!("Stderr capture thread panicked"))??,
        None => Vec::new(),
    };

    let stdout_option = if stdout_bytes.is_empty() {
        None
    } else {
        Some(String::from_utf8_lossy(&stdout_bytes).to_string())
    };
    let stderr_option = if stderr_bytes.is_empty() {
        None
    } else {
        Some(String::from_utf8_lossy(&stderr_bytes).to_string())
    };

    match wait_result {
        Ok(status) => {
            let outcome = if status.success() {
                ProcessStatus::Succeeded
            } else {
                ProcessStatus::Failed
            };
            ProcessRegistry::mutate(|registry| {
                registry.update(&process_id, |record| {
                    record.mark_finished(
                        outcome,
                        status.code(),
                        None,
                        stdout_option.clone(),
                        stderr_option.clone(),
                    );
                })?;
                registry.retain_recent(MAX_PROCESSES_PER_WORKTREE);
                Ok(())
            })
            .context("Failed to persist process registry after completion")?;

            if !status.success() {
                if let Some(code) = status.code() {
                    bail!("Command exited with status {code}");
                } else {
                    bail!("Command terminated by signal");
                }
            }
            Ok(())
        }
        Err(err) => {
            let error_message = format!("Failed to wait for '{program}': {err}");
            ProcessRegistry::mutate(|registry| {
                registry.update(&process_id, |record| {
                    record.mark_finished(
                        ProcessStatus::Failed,
                        None,
                        Some(error_message.clone()),
                        stdout_option.clone(),
                        stderr_option.clone(),
                    );
                })?;
                registry.retain_recent(MAX_PROCESSES_PER_WORKTREE);
                Ok(())
            })
            .context("Failed to persist process registry after wait error")?;
            Err(err).with_context(|| format!("Failed to wait on '{program}'"))
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
