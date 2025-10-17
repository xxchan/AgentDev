use anyhow::{Context, Result};
use colored::Colorize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;

use unicode_width::UnicodeWidthStr;

use agentdev::config::{agent_config_path, load_agent_config, split_cmdline};
use agentdev::git::get_repo_name;
use agentdev::state::XlaudeState;
use agentdev::tmux::TmuxManager;
use agentdev::utils::{generate_random_name, sanitize_branch_name};
use dialoguer::Select;

use crate::commands::handle_dashboard;

/// Generate a random task name using three BIP39 words
fn generate_task_name() -> Result<String> {
    let a = generate_random_name()?;
    let b = generate_random_name()?;
    let c = generate_random_name()?;
    Ok(format!("{}-{}-{}", a, b, c))
}

/// Pad or truncate a label for aligned display (Unicode-aware).
fn pad_label(label: &str, width: usize) -> String {
    let w = UnicodeWidthStr::width(label);
    if w == width {
        label.to_string()
    } else if w < width {
        let pad = " ".repeat(width - w);
        format!("{}{}", label, pad)
    } else {
        truncate_middle(label, width)
    }
}

/// Truncate with a middle ellipsis to desired display width.
fn truncate_middle(s: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(s) <= max_width || max_width <= 1 {
        return s.chars().take(max_width).collect();
    }
    if max_width <= 3 {
        return "…".repeat(max_width / 3);
    }
    // Keep left/right halves
    let left_keep = (max_width - 1) / 2;
    let right_keep = max_width - 1 - left_keep;
    let mut left = String::new();
    let mut right = String::new();
    // Build left
    for ch in s.chars() {
        if UnicodeWidthStr::width(left.as_str()) + UnicodeWidthStr::width(ch.to_string().as_str())
            > left_keep
        {
            break;
        }
        left.push(ch);
    }
    // Build right from end
    for ch in s.chars().rev() {
        if UnicodeWidthStr::width(right.as_str()) + UnicodeWidthStr::width(ch.to_string().as_str())
            > right_keep
        {
            break;
        }
        right.insert(0, ch);
    }
    format!("{}{}{}", left, "…", right)
}

fn quote_arg_for_display(arg: &str) -> String {
    // Minimal quoting for readability in logs
    if arg
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '(' | ')' | '"' | '\\'))
    {
        let escaped = arg.replace('"', "\\\"").replace('\\', "\\\\");
        format!("\"{}\"", escaped)
    } else {
        arg.to_string()
    }
}

pub fn handle_start(prompt: String, agents: Option<String>, name: Option<String>) -> Result<()> {
    // Ctrl+C cancellation flag
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let c = cancelled.clone();
        let _ = ctrlc::set_handler(move || {
            c.store(true, Ordering::SeqCst);
            eprintln!(
                "\n{} Aborting — finishing current steps (press again to force).",
                "✋".yellow()
            );
        });
    }
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

    // Phase 1: create all worktrees sequentially to avoid state.json races
    let total_agents = selected_agents.len();
    struct Created {
        alias: String,
        created_name: String,
        key: String,
        program: String,
        args: Vec<String>,
    }
    let mut created: Vec<Created> = Vec::new();
    let alias_col = selected_agents
        .iter()
        .map(|(a, _)| UnicodeWidthStr::width(a.as_str()))
        .max()
        .unwrap_or(4)
        .max(6);

    for (idx, (alias, cmdline)) in selected_agents.iter().enumerate() {
        if cancelled.load(Ordering::SeqCst) {
            println!("{} Start aborted before creation.", "⚠".yellow());
            break;
        }

        let label = pad_label(alias, alias_col);
        let steps_total = 5u8;
        println!(
            "{} [{}/{}] {} {}",
            "▶".blue(),
            (idx + 1),
            total_agents,
            "Agent".bold(),
            label.cyan()
        );

        let branch = format!("{}-{}", task, alias);
        let branch = sanitize_branch_name(&branch);
        println!("  {} [1/{}] Create worktree", "→".blue(), steps_total);
        let created_name = crate::commands::create::handle_create_in_dir_quiet(
            Some(branch.clone()),
            None,
            true,
            None,
        )?;
        println!(
            "    {} name: {}",
            "✓".green(),
            truncate_middle(&created_name, 40)
        );

        // Update task_id and initial prompt in state
        let mut state = XlaudeState::load()?;
        let key = XlaudeState::make_key(&repo_name, &created_name);
        if let Some(info) = state.worktrees.get_mut(&key) {
            info.task_id = Some(task.clone());
            info.initial_prompt = Some(prompt.clone());
        }
        state.save()?;

        // Parse command (once) for display and later use
        let (program, args) = split_cmdline(cmdline)?;
        created_worktrees.push(created_name.clone());
        created.push(Created {
            alias: alias.clone(),
            created_name: created_name.clone(),
            key,
            program,
            args,
        });
    }

    if cancelled.load(Ordering::SeqCst) {
        println!(
            "{} Aborted by user. Skipping session launch.",
            "✋".yellow()
        );
        return Ok(());
    }

    // Phase 2: launch sessions concurrently
    let (tx, rx) = mpsc::channel::<String>();
    thread::scope(|scope| {
        for c in &created {
            let tx = tx.clone();
            let cancelled = cancelled.clone();
            let created_name = c.created_name.clone();
            let alias = c.alias.clone();
            let program = c.program.clone();
            let args = c.args.clone();
            let prompt = prompt.clone();
            let state = XlaudeState::load().expect("state load");
            let info = state
                .worktrees
                .get(&c.key)
                .cloned()
                .expect("worktree info missing");
            let label = pad_label(&alias, alias_col);
            scope.spawn(move || {
                let steps_total = 5u8;
                let tmux = TmuxManager::new();

                let args_disp: String = if args.is_empty() {
                    String::new()
                } else {
                    let parts: Vec<String> =
                        args.iter().map(|a| quote_arg_for_display(a)).collect();
                    format!(" {}", parts.join(" "))
                };
                let _ = tx.send(format!(
                    "  {} [{}] {} [2/{}] Launch session (cmd: {}{})",
                    "→".blue(),
                    label.cyan(),
                    truncate_middle(&created_name, 28),
                    steps_total,
                    quote_arg_for_display(&program),
                    args_disp
                ));
                if let Err(e) =
                    tmux.create_session_with_command(&created_name, &info.path, &program, &args)
                {
                    let _ = tx.send(format!(
                        "    {} [{}] launch failed: {}",
                        "✗".red(),
                        label.cyan(),
                        e
                    ));
                    return;
                }

                // Startup grace period
                let start_wait_ms: u64 = std::env::var("AGENTDEV_START_WAIT_MS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(2000);
                let _ = tx.send(format!(
                    "    {} [{}] Wait {}ms for process startup",
                    "…".yellow(),
                    label.cyan(),
                    start_wait_ms
                ));
                std::thread::sleep(std::time::Duration::from_millis(start_wait_ms));
                if cancelled.load(Ordering::SeqCst) {
                    let _ = tx.send(format!(
                        "    {} [{}] cancelled",
                        "✋".yellow(),
                        label.cyan()
                    ));
                    return;
                }
                if !tmux.session_exists(&created_name) {
                    let cfg_path = agent_config_path();
                    let _ = tx.send(format!(
                        "    {} [{}] session missing after launch. Edit config: {}",
                        "✗".red(),
                        label.cyan(),
                        cfg_path.display()
                    ));
                    return;
                }

                let _ = tx.send(format!(
                    "  {} [{}] [3/{}] Wait for UI to be ready",
                    "→".blue(),
                    label.cyan(),
                    steps_total
                ));
                let _ = wait_for_input_ready_cancel(&tmux, &created_name, &cancelled);
                if cancelled.load(Ordering::SeqCst) {
                    let _ = tx.send(format!(
                        "    {} [{}] cancelled",
                        "✋".yellow(),
                        label.cyan()
                    ));
                    return;
                }
                let _ = tx.send(format!(
                    "    {} [{}] Ready for input",
                    "✓".green(),
                    label.cyan()
                ));

                // Send initial prompt
                let _ = tx.send(format!(
                    "  {} [{}] [4/{}] Send initial prompt",
                    "→".blue(),
                    label.cyan(),
                    steps_total
                ));
                if let Err(e) = tmux.send_text(&created_name, &prompt) {
                    let _ = tx.send(format!(
                        "    {} [{}] send failed: {}",
                        "✗".red(),
                        label.cyan(),
                        e
                    ));
                    return;
                }
                let _ = tmux.send_enter(&created_name);
                std::thread::sleep(std::time::Duration::from_millis(300));
                let _ = tmux.send_enter(&created_name);

                let verify_delay_ms: u64 = std::env::var("AGENTDEV_VERIFY_DELAY_MS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(600);
                let _ = tx.send(format!(
                    "    {} [{}] Verifying echo ({}ms)",
                    "…".yellow(),
                    label.cyan(),
                    verify_delay_ms
                ));
                std::thread::sleep(std::time::Duration::from_millis(verify_delay_ms));
                if cancelled.load(Ordering::SeqCst) {
                    let _ = tx.send(format!(
                        "    {} [{}] cancelled",
                        "✋".yellow(),
                        label.cyan()
                    ));
                    return;
                }
                if !prompt_echoed(&tmux, &created_name, &prompt) {
                    let _ = tx.send(format!(
                        "    {} [{}] Echo not detected — using slow-type",
                        "⚠".yellow(),
                        label.cyan()
                    ));
                    let _ = tmux.send_enter(&created_name);
                    let _ = slow_type_cancel(&tmux, &created_name, &prompt, &cancelled);
                    let _ = tmux.send_enter(&created_name);
                }
                let _ = tx.send(format!(
                    "{} [{}] Agent '{}' ready",
                    "✅".green(),
                    label.cyan(),
                    alias.cyan()
                ));
            });
        }
        drop(tx);
        // Print progress as it arrives
        for msg in rx.iter() {
            println!("{}", msg);
        }
    });

    // Give the agent a moment to start and verify the session is still alive
    // Tunable via env for slower startups.

    println!("{} Created worktrees:", "📁".green());
    for wt in &created_worktrees {
        println!("  - {} / {}", repo_name, wt);
    }
    println!(
        "{} Use 'agentdev dashboard' to view and compare results",
        "💡".cyan()
    );

    // Post-start: prompt to enter dashboard (yes/no/always)
    let mut state = XlaudeState::load()?;
    let non_interactive = std::env::var("XLAUDE_NON_INTERACTIVE").is_ok();
    let piped = crate::input::is_piped_input();

    if state.auto_open_dashboard_after_start {
        println!("{} Opening dashboard (always)...", "🖥".green());
        handle_dashboard()?;
        return Ok(());
    }

    if !non_interactive && !piped {
        let choices = vec!["Yes", "No", "Always"];
        let sel = Select::new()
            .with_prompt("Open dashboard now?")
            .items(&choices)
            .default(0)
            .interact()?;

        match sel {
            0 => {
                // Yes
                handle_dashboard()?;
            }
            1 => {
                // No -> nothing to do
            }
            2 => {
                // Always
                state.auto_open_dashboard_after_start = true;
                let _ = state.save();
                println!("{} Opening dashboard (set to always)", "🖥".green());
                handle_dashboard()?;
            }
            _ => {}
        }
    }

    Ok(())
}

/// Cancellable variant of wait_for_input_ready
fn wait_for_input_ready_cancel(
    tmux: &TmuxManager,
    project: &str,
    cancelled: &AtomicBool,
) -> Result<()> {
    let timeout_ms: u64 = std::env::var("AGENTDEV_READY_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8000);
    let interval = std::time::Duration::from_millis(200);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let _ = tmux.send_enter(project);
    while std::time::Instant::now() < deadline {
        if cancelled.load(Ordering::SeqCst) {
            return Ok(());
        }
        let out = tmux.capture_pane(project, 240)?;
        let lower = out.to_lowercase();
        let ready = lower.contains("add a follow-up")
            || lower.contains("human:")
            || out.ends_with('▌')
            || out.ends_with('█')
            || lower.contains("cursor agent")
            || lower.contains("press enter")
            || lower.contains("│ >");
        if ready {
            std::thread::sleep(std::time::Duration::from_millis(200));
            return Ok(());
        }
        std::thread::sleep(interval);
    }
    Ok(())
}

/// Check if the prompt (or a representative slice) appears in the pane output.
fn prompt_echoed(tmux: &TmuxManager, project: &str, prompt: &str) -> bool {
    // Build a compact needle from the first 24 non-newline chars
    let needle: String = prompt
        .chars()
        .filter(|&c| c != '\n' && c != '\r')
        .take(24)
        .collect();
    if needle.is_empty() {
        return true;
    }
    if let Ok(out) = tmux.capture_pane(project, 400) {
        let norm_out = out.replace(['\n', '\r'], " ");
        return norm_out.contains(&needle);
    }
    false
}

/// Cancellable slow-type
fn slow_type_cancel(
    tmux: &TmuxManager,
    project: &str,
    text: &str,
    cancelled: &AtomicBool,
) -> Result<()> {
    let chunk_size: usize = std::env::var("AGENTDEV_SLOW_TYPE_CHUNK")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(80);
    let delay_ms: u64 = std::env::var("AGENTDEV_SLOW_TYPE_DELAY_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(40);
    let mut i = 0;
    let bytes: Vec<char> = text.chars().collect();
    while i < bytes.len() {
        if cancelled.load(Ordering::SeqCst) {
            break;
        }
        let end = std::cmp::min(i + chunk_size, bytes.len());
        let chunk: String = bytes[i..end].iter().collect();
        tmux.send_text(project, &chunk)?;
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        i = end;
    }
    Ok(())
}
