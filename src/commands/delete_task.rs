use anyhow::{Context, Result};
use colored::Colorize;

use crate::input::smart_confirm;
use crate::state::XlaudeState;
use crate::tmux::TmuxManager;

/// CLI entry for `delete-task` that can handle missing arguments gracefully.
/// If `task_name` is None, print available tasks and a usage hint.
pub fn handle_delete_task_cli(task_name: Option<String>) -> Result<()> {
    match task_name {
        Some(name) => handle_delete_task(name),
        None => {
            let state = XlaudeState::load()?;
            // Collect available tasks for a helpful message
            let mut tasks: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            for wt in state.worktrees.values() {
                if let Some(tid) = &wt.task_id {
                    tasks.insert(tid.clone());
                }
            }
            let available = if tasks.is_empty() {
                "(none)".to_string()
            } else {
                tasks
                    .into_iter()
                    .map(|t| format!("{}", t.cyan()))
                    .collect::<Vec<_>>()
                    .join(", ")
            };

            eprintln!("error: the following required arguments were not provided:\n  <TASK_NAME>");
            eprintln!("\nAvailable tasks: {}", available);
            eprintln!(
                "\nUsage: agentdev delete-task <TASK_NAME>\n\nFor more information, try '--help'."
            );
            anyhow::bail!("missing required argument <TASK_NAME>")
        }
    }
}

pub fn handle_delete_task(task_name: String) -> Result<()> {
    let mut state = XlaudeState::load()?;

    // Collect all worktrees for the task
    let mut targets: Vec<(String, crate::state::WorktreeInfo)> = state
        .worktrees
        .iter()
        .filter_map(|(k, v)| {
            if v.task_id.as_deref() == Some(task_name.as_str()) {
                Some((k.clone(), v.clone()))
            } else {
                None
            }
        })
        .collect();

    if targets.is_empty() {
        // Collect available tasks for a helpful message
        let mut tasks: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for wt in state.worktrees.values() {
            if let Some(tid) = &wt.task_id {
                tasks.insert(tid.clone());
            }
        }
        let available = if tasks.is_empty() {
            "(none)".to_string()
        } else {
            tasks
                .into_iter()
                .map(|t| format!("{}", t.cyan()))
                .collect::<Vec<_>>()
                .join(", ")
        };
        anyhow::bail!(
            "No worktrees found for task {}. Available tasks: {}",
            task_name.cyan(),
            available
        );
    }

    println!("{} Task: {}", "🧹".yellow(), task_name.cyan());
    println!("{} This will remove the following:", "⚠️ ".yellow());
    for (_, info) in &targets {
        println!("  - {}/{}", info.repo_name, info.name);
    }

    if !smart_confirm("Proceed to delete all worktrees for this task?", false)? {
        println!("{} Cancelled", "❌".red());
        return Ok(());
    }

    // Kill tmux sessions first
    let tmux = TmuxManager::new();
    for (_, info) in &targets {
        let _ = tmux.kill_session(&info.name);
    }

    // Ensure deletions don't prompt repeatedly
    unsafe {
        std::env::set_var("XLAUDE_YES", "1");
        std::env::set_var("XLAUDE_NON_INTERACTIVE", "1");
    }

    // Delete each worktree using existing delete logic
    for (_, info) in &targets {
        // Use the handler by name to reuse safety checks
        if let Err(e) = crate::commands::delete::handle_delete(Some(info.name.clone())) {
            eprintln!("{} Failed to delete {}: {}", "⚠️ ".yellow(), info.name, e);
        }
    }

    // Reload and remove any lingering entries for the task
    let mut state = XlaudeState::load()?;
    let keys: Vec<String> = state
        .worktrees
        .iter()
        .filter_map(|(k, v)| {
            if v.task_id.as_deref() == Some(task_name.as_str()) {
                Some(k.clone())
            } else {
                None
            }
        })
        .collect();
    for k in keys {
        state.worktrees.remove(&k);
    }
    state.save()?;

    println!("{} Task '{}' cleaned up", "✅".green(), task_name.cyan());
    Ok(())
}
