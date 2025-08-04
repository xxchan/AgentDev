use anyhow::{Context, Result};
use chrono::Utc;
use colored::*;

use crate::git::{get_current_branch, get_repo_name, is_in_worktree};
use crate::state::{WorktreeInfo, XlaudeState};

pub fn handle_add(name: Option<String>) -> Result<()> {
    // Check if we're in a git repository
    let repo_name = get_repo_name()
        .context("Not in a git repository")?;
    
    // Check if we're in a worktree
    if !is_in_worktree()? {
        anyhow::bail!("Current directory is not a git worktree");
    }
    
    // Get current branch name
    let current_branch = get_current_branch()?;
    
    // Use provided name or default to branch name
    let worktree_name = name.unwrap_or_else(|| current_branch.clone());
    
    // Get current directory
    let current_dir = std::env::current_dir()?;
    
    // Check if already managed
    let mut state = XlaudeState::load()?;
    if state.worktrees.contains_key(&worktree_name) {
        anyhow::bail!("Worktree '{}' is already managed by xlaude", worktree_name);
    }
    
    println!("{} Adding worktree '{}' to xlaude management...", "➕".green(), worktree_name.cyan());
    
    // Add to state
    state.worktrees.insert(
        worktree_name.clone(),
        WorktreeInfo {
            name: worktree_name.clone(),
            branch: current_branch,
            path: current_dir.clone(),
            repo_name: repo_name.clone(),
            created_at: Utc::now(),
        },
    );
    state.save()?;
    
    println!("{} Worktree '{}' added successfully", "✅".green(), worktree_name.cyan());
    println!("  {} {}", "Path:".bright_black(), current_dir.display());
    
    Ok(())
}