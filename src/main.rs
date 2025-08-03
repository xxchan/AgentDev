use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use colored::*;
use dialoguer::Confirm;
use directories::ProjectDirs;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser)]
#[command(name = "xlaude")]
#[command(about = "Manage Claude instances with git worktrees", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Open a new Claude instance with a git worktree
    Open {
        /// Name for the worktree (random BIP39 word if not provided)
        name: Option<String>,
    },
    /// Close a Claude instance and clean up its worktree
    Close {
        /// Name of the worktree to close (current if not provided)
        name: Option<String>,
    },
    /// Add current worktree to xlaude management
    Add {
        /// Name for the worktree (defaults to current branch name)
        name: Option<String>,
    },
    /// List all active Claude instances
    List,
}

#[derive(Debug, Serialize, Deserialize)]
struct WorktreeInfo {
    name: String,
    branch: String,
    path: PathBuf,
    repo_name: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct XlaudeState {
    worktrees: HashMap<String, WorktreeInfo>,
}

impl XlaudeState {
    fn load() -> Result<Self> {
        let config_path = get_config_path()?;
        if config_path.exists() {
            let content = fs::read_to_string(&config_path)
                .context("Failed to read config file")?;
            serde_json::from_str(&content)
                .context("Failed to parse config file")
        } else {
            Ok(Self::default())
        }
    }

    fn save(&self) -> Result<()> {
        let config_path = get_config_path()?;
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create config directory")?;
        }
        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize state")?;
        fs::write(&config_path, content)
            .context("Failed to write config file")?;
        Ok(())
    }
}

fn get_config_path() -> Result<PathBuf> {
    let proj_dirs = ProjectDirs::from("com", "xuanwo", "xlaude")
        .context("Failed to determine config directory")?;
    Ok(proj_dirs.config_dir().join("state.json"))
}

fn execute_git(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .context("Failed to execute git command")?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Git command failed: {}", stderr);
    }
}

fn get_repo_name() -> Result<String> {
    let toplevel = execute_git(&["rev-parse", "--show-toplevel"])?;
    let path = Path::new(&toplevel);
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .context("Failed to get repository name")
}

fn get_current_branch() -> Result<String> {
    execute_git(&["symbolic-ref", "--short", "HEAD"])
}

fn is_base_branch() -> Result<bool> {
    let current = get_current_branch()?;
    let base_branches = ["main", "master", "develop"];
    Ok(base_branches.contains(&current.as_str()))
}

fn is_working_tree_clean() -> Result<bool> {
    let status = execute_git(&["status", "--porcelain"])?;
    Ok(status.is_empty())
}

fn has_unpushed_commits() -> Result<bool> {
    match execute_git(&["log", "@{u}.."]) {
        Ok(output) => Ok(!output.is_empty()),
        Err(_) => Ok(false), // No upstream branch
    }
}

fn generate_random_name() -> Result<String> {
    // Generate 128 bits of entropy for a 12-word mnemonic
    let mut entropy = [0u8; 16];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut entropy);
    
    let mnemonic = bip39::Mnemonic::from_entropy(&entropy)?;
    let words: Vec<&str> = mnemonic.words().collect();
    words.choose(&mut rand::thread_rng())
        .map(|&word| word.to_string())
        .context("Failed to generate random name")
}

fn handle_open(name: Option<String>) -> Result<()> {
    // Check if we're in a git repository
    let repo_name = get_repo_name()
        .context("Not in a git repository")?;

    // Check if we're on a base branch
    if !is_base_branch()? {
        anyhow::bail!("Must be on a base branch (main, master, or develop) to open a new worktree");
    }

    // Generate name if not provided
    let worktree_name = match name {
        Some(n) => n,
        None => generate_random_name()?,
    };

    println!("{} Creating worktree '{}'...", "✨".green(), worktree_name.cyan());

    // Create branch
    execute_git(&["branch", &worktree_name])
        .context("Failed to create branch")?;

    // Create worktree
    let worktree_dir = format!("../{repo_name}-{worktree_name}");
    execute_git(&["worktree", "add", &worktree_dir, &worktree_name])
        .context("Failed to create worktree")?;

    // Get absolute path
    let worktree_path = std::env::current_dir()?
        .parent()
        .unwrap()
        .join(format!("{repo_name}-{worktree_name}"));

    // Save state
    let mut state = XlaudeState::load()?;
    state.worktrees.insert(
        worktree_name.clone(),
        WorktreeInfo {
            name: worktree_name.clone(),
            branch: worktree_name.clone(),
            path: worktree_path.clone(),
            repo_name: repo_name.clone(),
            created_at: Utc::now(),
        },
    );
    state.save()?;

    println!("{} Worktree created at: {}", "📁".green(), worktree_path.display());
    println!("{} Launching Claude...", "🚀".green());

    // Change to worktree directory and launch Claude
    std::env::set_current_dir(&worktree_path)
        .context("Failed to change directory")?;

    let mut cmd = Command::new("claude");
    cmd.arg("--dangerously-skip-permissions");
    
    // Inherit all environment variables
    cmd.envs(std::env::vars());

    let status = cmd.status()
        .context("Failed to launch Claude")?;

    if !status.success() {
        anyhow::bail!("Claude exited with error");
    }

    Ok(())
}

fn handle_close(name: Option<String>) -> Result<()> {
    let mut state = XlaudeState::load()?;

    // Determine which worktree to close
    let worktree_name = match name {
        Some(n) => n,
        None => {
            // Get current directory name to find current worktree
            let current_dir = std::env::current_dir()?;
            let dir_name = current_dir
                .file_name()
                .and_then(|n| n.to_str())
                .context("Failed to get current directory name")?;

            // Find matching worktree
            state.worktrees
                .values()
                .find(|w| w.path.file_name().and_then(|n| n.to_str()) == Some(dir_name))
                .map(|w| w.name.clone())
                .context("Current directory is not a managed worktree")?
        }
    };

    let worktree_info = state.worktrees.get(&worktree_name)
        .context("Worktree not found")?;

    println!("{} Checking worktree '{}'...", "🔍".yellow(), worktree_name.cyan());

    // Change to worktree directory to check status
    let original_dir = std::env::current_dir()?;
    std::env::set_current_dir(&worktree_info.path)
        .context("Failed to change to worktree directory")?;

    // Check for uncommitted changes
    let has_changes = !is_working_tree_clean()?;
    let has_unpushed = has_unpushed_commits()?;

    if has_changes || has_unpushed {
        println!();
        if has_changes {
            println!("{} You have uncommitted changes", "⚠️ ".red());
        }
        if has_unpushed {
            println!("{} You have unpushed commits", "⚠️ ".red());
        }

        let confirmed = Confirm::new()
            .with_prompt("Are you sure you want to close this worktree?")
            .default(false)
            .interact()?;

        if !confirmed {
            println!("{} Cancelled", "❌".red());
            return Ok(());
        }
    }

    // Change back to original directory
    std::env::set_current_dir(&original_dir)?;

    // Remove worktree
    println!("{} Removing worktree...", "🗑️ ".yellow());
    execute_git(&["worktree", "remove", worktree_info.path.to_str().unwrap()])
        .context("Failed to remove worktree")?;

    // Try to delete branch (will fail if not merged)
    let _ = execute_git(&["branch", "-d", &worktree_info.branch]);

    // Update state
    state.worktrees.remove(&worktree_name);
    state.save()?;

    println!("{} Worktree '{}' closed successfully", "✅".green(), worktree_name.cyan());
    Ok(())
}

#[derive(Debug)]
struct SessionInfo {
    last_user_message: String,
    last_timestamp: Option<DateTime<Utc>>,
}

fn get_claude_sessions(project_path: &Path) -> Vec<SessionInfo> {
    // Get home directory
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return vec![],
    };
    
    // Construct path to Claude projects directory
    let claude_projects_dir = Path::new(&home).join(".claude").join("projects");
    
    // Get canonical path of the project
    let canonical_path = match project_path.canonicalize() {
        Ok(p) => p,
        Err(_) => return vec![],
    };
    
    // Convert path to Claude's format (replace / with -)
    let encoded_path = canonical_path
        .to_string_lossy()
        .replace('/', "-");
    
    let project_dir = claude_projects_dir.join(&encoded_path);
    
    // List session files (.jsonl files)
    let mut sessions = vec![];
    if let Ok(entries) = fs::read_dir(&project_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".jsonl") {
                    
                    // Read session data from the file
                    let mut last_user_message = String::new();
                    let mut last_timestamp = None;
                    
                    if let Ok(file) = fs::File::open(entry.path()) {
                        let reader = BufReader::new(file);
                        let mut user_messages = Vec::new();
                        
                        for line in reader.lines().flatten() {
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                                if json.get("type").and_then(|t| t.as_str()) == Some("user") {
                                    // Extract timestamp
                                    if let Some(ts_str) = json.get("timestamp").and_then(|t| t.as_str()) {
                                        if let Ok(ts) = DateTime::parse_from_rfc3339(ts_str) {
                                            last_timestamp = Some(ts.with_timezone(&Utc));
                                        }
                                    }
                                    
                                    // Extract message content
                                    if let Some(message) = json.get("message") {
                                        let content = if let Some(content_str) = message.get("content").and_then(|c| c.as_str()) {
                                            content_str.to_string()
                                        } else if let Some(content_arr) = message.get("content").and_then(|c| c.as_array()) {
                                            content_arr.iter()
                                                .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                                                .collect::<Vec<_>>()
                                                .join(" ")
                                        } else {
                                            String::new()
                                        };
                                        
                                        // Filter out system messages and empty content
                                        if !content.is_empty() 
                                            && !content.starts_with("<local-command")
                                            && !content.starts_with("<command-")
                                            && !content.starts_with("Caveat:")
                                            && !content.contains("[Request interrupted") {
                                            user_messages.push(content);
                                        }
                                    }
                                }
                            }
                        }
                        
                        // Get the last meaningful user message
                        if let Some(msg) = user_messages.last() {
                            last_user_message = msg.clone();
                        }
                    }
                    
                    // Only add sessions with user messages
                    if !last_user_message.is_empty() {
                        sessions.push(SessionInfo {
                            last_user_message,
                            last_timestamp,
                        });
                    }
                }
            }
        }
    }
    
    // Sort by timestamp (most recent first)
    sessions.sort_by(|a, b| {
        match (&b.last_timestamp, &a.last_timestamp) {
            (Some(b_ts), Some(a_ts)) => b_ts.cmp(a_ts),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    });
    sessions
}

fn handle_list() -> Result<()> {
    let state = XlaudeState::load()?;

    if state.worktrees.is_empty() {
        println!("{} No active worktrees", "📭".yellow());
        return Ok(());
    }

    println!("{} Active worktrees:", "📋".cyan());
    println!();

    for (_, info) in state.worktrees.iter() {
        println!("  {} {}", "•".green(), info.name.cyan());
        println!("    {} {}", "Repository:".bright_black(), info.repo_name);
        println!("    {} {}", "Path:".bright_black(), info.path.display());
        println!("    {} {}", "Created:".bright_black(), info.created_at.format("%Y-%m-%d %H:%M:%S"));
        
        // Get Claude sessions for this worktree
        let sessions = get_claude_sessions(&info.path);
        if !sessions.is_empty() {
            println!("    {} {} session(s):", "Claude:".bright_black(), sessions.len());
            for session in sessions.iter().take(3) {
                // Format time
                let time_str = if let Some(ts) = &session.last_timestamp {
                    let now = Utc::now();
                    let diff = now.signed_duration_since(*ts);
                    
                    if diff.num_minutes() < 60 {
                        format!("{}m ago", diff.num_minutes())
                    } else if diff.num_hours() < 24 {
                        format!("{}h ago", diff.num_hours())
                    } else {
                        format!("{}d ago", diff.num_days())
                    }
                } else {
                    "unknown".to_string()
                };
                
                // Truncate message if too long
                let message = if session.last_user_message.len() > 60 {
                    format!("{}...", &session.last_user_message[..57])
                } else {
                    session.last_user_message.clone()
                };
                
                println!("      {} {} {}", 
                    "-".bright_black(), 
                    time_str.bright_black(), 
                    message.bright_black()
                );
            }
            if sessions.len() > 3 {
                println!("      {} ... and {} more", "-".bright_black(), sessions.len() - 3);
            }
        }
        
        println!();
    }

    Ok(())
}

fn is_in_worktree() -> Result<bool> {
    // Check if we're in a worktree by looking for .git file (not directory)
    let git_path = Path::new(".git");
    if git_path.exists() && git_path.is_file() {
        return Ok(true);
    }
    
    // Alternative: check git worktree list
    match execute_git(&["rev-parse", "--git-common-dir"]) {
        Ok(common_dir) => {
            let current_git_dir = execute_git(&["rev-parse", "--git-dir"])?;
            Ok(common_dir != current_git_dir)
        }
        Err(_) => Ok(false),
    }
}

fn handle_add(name: Option<String>) -> Result<()> {
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Open { name } => handle_open(name),
        Commands::Close { name } => handle_close(name),
        Commands::Add { name } => handle_add(name),
        Commands::List => handle_list(),
    }
}