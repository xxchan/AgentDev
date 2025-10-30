use anyhow::{Context, Result};
use chrono::Utc;
use colored::Colorize;
use dialoguer::Input;
use std::fs;
use std::path::PathBuf;

use crate::commands::open::handle_open;
use crate::input::{get_command_arg, is_piped_input, smart_confirm};
use agentdev::git::{
    execute_git, extract_repo_name_from_url, get_repo_name, list_worktrees, update_submodules,
};
use agentdev::state::{WorktreeInfo, XlaudeState};
use agentdev::utils::sanitize_branch_name;

pub fn handle_create(name: Option<String>, agent: Option<String>) -> Result<()> {
    handle_create_in_dir(name, None, agent)
}

pub fn handle_create_in_dir(
    name: Option<String>,
    repo_path: Option<PathBuf>,
    agent: Option<String>,
) -> Result<()> {
    handle_create_in_dir_quiet(name, repo_path, false, agent)?;
    Ok(())
}

// Create worktree quietly without prompting for open, returns the created worktree name
pub fn handle_create_in_dir_quiet(
    name: Option<String>,
    repo_path: Option<PathBuf>,
    quiet: bool,
    agent: Option<String>,
) -> Result<String> {
    // Helper to execute git in the right directory using git -C
    let exec_git = |args: &[&str]| -> Result<String> {
        if let Some(ref path) = repo_path {
            // Use git -C to execute in specified directory
            let mut full_args = vec!["-C", path.to_str().unwrap()];
            full_args.extend_from_slice(args);
            execute_git(&full_args)
        } else {
            execute_git(args)
        }
    };

    // Get repo name from the target directory
    let repo_name = if let Some(ref path) = repo_path {
        // Get repo name from the specified path using git -C
        let output = execute_git(&["-C", path.to_str().unwrap(), "remote", "get-url", "origin"])?;
        if let Some(name) = extract_repo_name_from_url(&output) {
            name
        } else {
            // Fallback to directory name
            path.file_name()
                .and_then(|n| n.to_str())
                .map(String::from)
                .context("Failed to get repository name")?
        }
    } else {
        get_repo_name().context("Not in a git repository")?
    };

    // Only check base branch if no repo_path is provided (i.e., running from CLI in current directory)
    // When invoked programmatically (e.g. from the web UI) with a specific repo_path,
    // we don't need this check as we'll create the worktree from the default branch.
    if repo_path.is_none() {
        let current_branch = exec_git(&["branch", "--show-current"])?;
        let default_branch = exec_git(&["symbolic-ref", "refs/remotes/origin/HEAD"])
            .ok()
            .and_then(|s| s.strip_prefix("refs/remotes/origin/").map(String::from))
            .unwrap_or_else(|| "main".to_string());

        let base_branches = ["main", "master", "develop", &default_branch];
        if !base_branches.contains(&current_branch.as_str()) {
            anyhow::bail!(
                "Must be on a base branch (main, master, or develop) to create a new worktree. Current branch: {}",
                current_branch
            );
        }
    }

    // Determine target branch/worktree name
    let branch_name = match get_command_arg(name)? {
        Some(n) => n,
        None => {
            if std::env::var("XLAUDE_NON_INTERACTIVE").is_ok() || is_piped_input() {
                anyhow::bail!(
                    "Worktree name is required. Please provide one as an argument or via piped input.",
                );
            }

            // Interactive prompt when no name was supplied and we're in TTY mode
            let input: String = Input::new()
                .with_prompt("Enter a name for the new worktree")
                .validate_with(|value: &String| -> Result<(), &str> {
                    if value.trim().is_empty() {
                        Err("Worktree name cannot be empty")
                    } else {
                        Ok(())
                    }
                })
                .interact_text()
                .context("Failed to read worktree name")?;
            input.trim().to_string()
        }
    };

    // Sanitize the branch name for use in directory names
    let worktree_name = sanitize_branch_name(&branch_name);

    // Check if a worktree with this name already exists in xlaude state
    let state = XlaudeState::load()?;
    let key = XlaudeState::make_key(&repo_name, &worktree_name);
    if state.worktrees.contains_key(&key) {
        anyhow::bail!(
            "A worktree named '{}' already exists for repository '{}' (tracked by xlaude). Please choose a different name.",
            worktree_name,
            repo_name
        );
    }

    // Compute the base directory for worktrees (../<repo>.worktrees/<name>)
    let worktree_base_dir = if let Some(ref path) = repo_path {
        path.parent()
            .context("Repository path has no parent directory")?
            .join(format!("{repo_name}.worktrees"))
    } else {
        std::env::current_dir()?
            .parent()
            .context("Repository directory has no parent directory")?
            .join(format!("{repo_name}.worktrees"))
    };
    let worktree_dir_path = worktree_base_dir.join(&worktree_name);

    // Check if the directory already exists
    if worktree_dir_path.exists() {
        anyhow::bail!(
            "Directory '{}' already exists. Please choose a different name or remove the existing directory.",
            worktree_dir_path.display()
        );
    }

    // Check if a git worktree already exists at this path
    // Need to run git worktree list in the correct directory
    let existing_worktrees = if let Some(ref path) = repo_path {
        // Parse git worktree list output from the specified directory
        let output = execute_git(&[
            "-C",
            path.to_str().unwrap(),
            "worktree",
            "list",
            "--porcelain",
        ])?;
        let mut worktrees = Vec::new();
        for line in output.lines() {
            if let Some(worktree_path) = line.strip_prefix("worktree ") {
                worktrees.push(PathBuf::from(worktree_path));
            }
        }
        worktrees
    } else {
        list_worktrees()?
    };

    if existing_worktrees.iter().any(|w| w == &worktree_dir_path) {
        anyhow::bail!(
            "A git worktree already exists at '{}'. Please choose a different name or remove the existing worktree.",
            worktree_dir_path.display()
        );
    }

    // Check if the branch already exists
    let branch_already_exists = exec_git(&[
        "show-ref",
        "--verify",
        &format!("refs/heads/{}", branch_name),
    ])
    .is_ok();

    if branch_already_exists {
        if !quiet {
            println!(
                "{} Creating worktree '{}' from existing branch '{}'...",
                "✨".green(),
                worktree_name.cyan(),
                branch_name.cyan()
            );
        }
    } else {
        if !quiet {
            println!(
                "{} Creating worktree '{}' with new branch '{}'...",
                "✨".green(),
                worktree_name.cyan(),
                branch_name.cyan()
            );
        }

        // When repo_path is provided, create branch from the default branch
        // Otherwise create from current branch
        if repo_path.is_some() {
            // Get the default branch
            let default_branch = exec_git(&["symbolic-ref", "refs/remotes/origin/HEAD"])
                .ok()
                .and_then(|s| s.strip_prefix("refs/remotes/origin/").map(String::from))
                .unwrap_or_else(|| "main".to_string());

            // Create branch from the default branch
            exec_git(&[
                "branch",
                &branch_name,
                &format!("origin/{}", default_branch),
            ])
            .context("Failed to create branch from default branch")?;
        } else {
            // Create branch from current branch (original behavior for CLI)
            exec_git(&["branch", &branch_name]).context("Failed to create branch")?;
        }
    }

    // Ensure the worktree root directory exists before adding the worktree
    fs::create_dir_all(&worktree_base_dir).with_context(|| {
        format!(
            "Failed to create worktree root at {}",
            worktree_base_dir.display()
        )
    })?;

    // Create worktree with sanitized directory name
    let worktree_dir = format!("../{repo_name}.worktrees/{worktree_name}");
    exec_git(&["worktree", "add", &worktree_dir, &branch_name])
        .context("Failed to create worktree")?;

    // Get absolute path
    let worktree_path = worktree_dir_path.clone();

    // Update submodules if they exist
    if let Err(e) = update_submodules(&worktree_path) {
        if !quiet {
            println!(
                "{} Warning: Failed to update submodules: {}",
                "⚠️".yellow(),
                e
            );
        }
    } else {
        // Check if submodules were actually updated
        let gitmodules = worktree_path.join(".gitmodules");
        if gitmodules.exists() && !quiet {
            println!("{} Updated submodules", "📦".green());
        }
    }

    // Copy CLAUDE.local.md if it exists
    let claude_local_md = if let Some(ref path) = repo_path {
        path.join("CLAUDE.local.md")
    } else {
        PathBuf::from("CLAUDE.local.md")
    };
    if claude_local_md.exists() {
        let target_path = worktree_path.join("CLAUDE.local.md");
        fs::copy(claude_local_md, &target_path).context("Failed to copy CLAUDE.local.md")?;
        if !quiet {
            println!("{} Copied CLAUDE.local.md to worktree", "📄".green());
        }
    }

    // Save state
    let mut state = XlaudeState::load()?;
    let key = XlaudeState::make_key(&repo_name, &worktree_name);
    state.worktrees.insert(
        key,
        WorktreeInfo {
            name: worktree_name.clone(),
            branch: branch_name.clone(),
            path: worktree_path.clone(),
            repo_name,
            created_at: Utc::now(),
            task_id: None,
            task_name: None,
            initial_prompt: None,
            agent_alias: None,
        },
    );
    state.save()?;

    if !quiet {
        println!(
            "{} Worktree created at: {}",
            "✅".green(),
            worktree_path.display()
        );
    }

    // Ask if user wants to open the worktree (skip in quiet mode)
    if !quiet {
        // Skip opening in test mode or when explicitly disabled
        let should_open = if std::env::var("XLAUDE_TEST_MODE").is_ok()
            || std::env::var("XLAUDE_NO_AUTO_OPEN").is_ok()
        {
            println!(
                "  {} To open it, run: {} {}",
                "💡".cyan(),
                "agentdev worktree open".cyan(),
                worktree_name.cyan()
            );
            false
        } else {
            smart_confirm("Would you like to open the worktree now?", true)?
        };

        if should_open {
            handle_open(Some(worktree_name.clone()), agent)?;
        } else if std::env::var("XLAUDE_NON_INTERACTIVE").is_err() {
            println!(
                "  {} To open it later, run: {} {}",
                "💡".cyan(),
                "agentdev worktree open".cyan(),
                worktree_name.cyan()
            );
        }
    }

    Ok(worktree_name)
}
