use anyhow::{Context, Result};
use colored::Colorize;

use crate::config::{agent_config_path, load_agent_config, split_cmdline};
use crate::git::get_repo_name;
use crate::state::XlaudeState;
use crate::tmux::TmuxManager;
use crate::utils::{generate_random_name, sanitize_branch_name};

/// Generate a random task name using three BIP39 words
fn generate_task_name() -> Result<String> {
    let a = generate_random_name()?;
    let b = generate_random_name()?;
    let c = generate_random_name()?;
    Ok(format!("{}-{}-{}", a, b, c))
}

pub fn handle_start(prompt: String, agents: Option<String>, name: Option<String>) -> Result<()> {
    // Ensure tmux is available early with a friendly message
    if !TmuxManager::is_available() {
        anyhow::bail!(
            "tmux is not installed or not on PATH.\n\
Install tmux and retry. On macOS: `brew install tmux`. On Ubuntu: `sudo apt-get install tmux`"
        );
    }
    // Load agent pool
    let cfg = load_agent_config()?;
    if cfg.agents.is_empty() {
        anyhow::bail!("No agents configured in agentdev config");
    }

    // Determine agents to run
    let selected_agents: Vec<(String, String)> = if let Some(list) = agents {
        let aliases: Vec<String> = list
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let mut v = Vec::new();
        for alias in aliases {
            if let Some(cmd) = cfg.agents.get(&alias) {
                v.push((alias.clone(), cmd.clone()));
            } else {
                anyhow::bail!("Agent alias not found in config: {}", alias);
            }
        }
        v
    } else {
        cfg.agents
            .iter()
            .map(|(a, c)| (a.clone(), c.clone()))
            .collect()
    };

    if selected_agents.is_empty() {
        anyhow::bail!("No agents selected");
    }

    // Determine task name
    let task = name.unwrap_or_else(|| generate_task_name().unwrap_or_else(|_| "task".to_string()));
    let task = sanitize_branch_name(&task);

    let repo_name = get_repo_name().context("Not in a git repository")?;

    println!(
        "{} Starting task '{}' with {} agent(s)...",
        "🚀".green(),
        task.cyan(),
        selected_agents.len()
    );

    let mut created_worktrees: Vec<String> = Vec::new();

    // Create per-agent worktrees and sessions
    for (alias, cmdline) in &selected_agents {
        let branch = format!("{}-{}", task, alias);
        let branch = sanitize_branch_name(&branch);

        // Create worktree + branch
        let created_name =
            crate::commands::create::handle_create_in_dir_quiet(Some(branch.clone()), None, true)?;

        // Update task_id in state
        let mut state = XlaudeState::load()?;
        let key = XlaudeState::make_key(&repo_name, &created_name);
        if let Some(info) = state.worktrees.get_mut(&key) {
            info.task_id = Some(task.clone());
        }
        state.save()?;

        created_worktrees.push(created_name.clone());

        // Launch tmux session with specific agent command
        let (program, args) = split_cmdline(cmdline)?;
        let tmux = TmuxManager::new();
        let info = state.worktrees.get(&key).context("worktree info missing")?;
        tmux.create_session_with_command(&created_name, &info.path, &program, &args)?;

        // Give the agent a moment to start and verify the session is still alive
        std::thread::sleep(std::time::Duration::from_millis(2000));
        if !tmux.session_exists(&created_name) {
            let cfg_path = agent_config_path();
            anyhow::bail!(
                "Failed to start tmux session for agent '{alias}'.\n\
Command: {cmd}\n\
The process likely exited immediately (e.g., command not found or crashed).\n\
Next steps:\n\
- Check sessions: `tmux ls`\n\
- Verify the agent command exists: `which {program}`\n\
- Edit agent config: {cfg}\n\
- Try a specific installed agent: `xlaude start --agents codex \"...\"`",
                alias = alias,
                cmd = cmdline,
                program = program,
                cfg = cfg_path.display(),
            );
        }

        // Agent session exists; send the initial prompt
        tmux.send_text(&created_name, &prompt)?;
        // Many CLIs accept submission on a blank line; send two Enters with a short delay
        tmux.send_enter(&created_name)?;
        std::thread::sleep(std::time::Duration::from_millis(500));
        tmux.send_enter(&created_name)?;
        // Optional small delay for prompt processing
        std::thread::sleep(std::time::Duration::from_millis(150));
    }

    println!("{} Created worktrees:", "📁".green());
    for wt in &created_worktrees {
        println!("  - {} / {}", repo_name, wt);
    }
    println!(
        "{} Use '{}' to view and compare results",
        "💡".cyan(),
        "agentdev dashboard"
    );

    Ok(())
}
