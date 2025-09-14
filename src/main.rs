use anyhow::Result;
use clap::{Parser, Subcommand};
use clap_complete::Shell;

mod claude;
mod claude_status;
mod commands;
mod completions;
mod config;
mod git;
mod input;
mod state;
mod tmux;
mod utils;

use commands::{
    handle_add, handle_clean, handle_create, handle_delete, handle_delete_task,
    handle_delete_task_cli, handle_dir, handle_list, handle_open, handle_rename, handle_start,
};

#[derive(Parser)]
#[command(name = "agentdev")]
#[command(
    about = "Manage Claude instances with git worktrees",
    long_about = None,
    after_help = "\
Config file:\n\
- macOS/Linux: ~/.config/agentdev/config.toml\n\
- Windows: %APPDATA%\\agentdev\\config.toml\n\
\n\
A reference config is generated on first run.\n"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Worktree management commands
    #[command(alias = "wt")]
    Worktree {
        #[command(subcommand)]
        cmd: WorktreeCommands,
    },
    // Backward-compatible top-level commands (temporarily retained)
    #[command(hide = true)]
    Create {
        /// Name for the worktree (random BIP39 word if not provided)
        name: Option<String>,
    },
    #[command(hide = true)]
    Open {
        /// Name of the worktree to open (interactive selection if not provided)
        name: Option<String>,
    },
    #[command(hide = true)]
    Delete {
        /// Name of the worktree to delete (current if not provided)
        name: Option<String>,
    },
    #[command(hide = true)]
    Add {
        /// Name for the worktree (defaults to current branch name)
        name: Option<String>,
    },
    #[command(hide = true)]
    Rename {
        /// Current name of the worktree
        old_name: String,
        /// New name for the worktree
        new_name: String,
    },
    #[command(hide = true)]
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    #[command(hide = true)]
    Clean,
    #[command(hide = true)]
    Dir {
        /// Name of the worktree (interactive selection if not provided)
        name: Option<String>,
    },
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Output worktree info for shell completions (hidden)
    #[command(hide = true)]
    CompleteWorktrees {
        /// Output format: simple or detailed
        #[arg(long, default_value = "simple")]
        format: String,
    },
    /// Launch interactive dashboard for managing Claude sessions
    Dashboard,
    /// Start a multi-agent task and send an initial prompt
    Start {
        /// Initial task prompt (quoted)
        prompt: String,
        /// Comma separated agent aliases (default: all agents in config)
        #[arg(long)]
        agents: Option<String>,
        /// Task name (default: random words)
        #[arg(long)]
        name: Option<String>,
    },
    /// Delete all resources for a given task
    #[command(alias = "delete-tasks")]
    DeleteTask {
        /// Task name
        task_name: Option<String>,
    },
}

fn main() -> Result<()> {
    // Eagerly ensure agent config exists and is loadable on every invocation
    let _ = crate::config::load_agent_config();

    let cli = Cli::parse();

    match cli.command {
        Commands::Worktree { cmd } => match cmd {
            WorktreeCommands::Create { name } => handle_create(name),
            WorktreeCommands::Open { name } => handle_open(name),
            WorktreeCommands::Delete { name } => handle_delete(name),
            WorktreeCommands::Add { name } => handle_add(name),
            WorktreeCommands::Rename { old_name, new_name } => handle_rename(old_name, new_name),
            WorktreeCommands::List { json } => handle_list(json),
            WorktreeCommands::Clean => handle_clean(),
            WorktreeCommands::Dir { name } => handle_dir(name),
        },
        Commands::Completions { shell } => completions::handle_completions(shell),
        Commands::CompleteWorktrees { format } => commands::handle_complete_worktrees(&format),
        Commands::Dashboard => commands::handle_dashboard(),
        Commands::Start {
            prompt,
            agents,
            name,
        } => handle_start(prompt, agents, name),
        Commands::DeleteTask { task_name } => handle_delete_task_cli(task_name),
        // Backward-compatible routing
        Commands::Create { name } => handle_create(name),
        Commands::Open { name } => handle_open(name),
        Commands::Delete { name } => handle_delete(name),
        Commands::Add { name } => handle_add(name),
        Commands::Rename { old_name, new_name } => handle_rename(old_name, new_name),
        Commands::List { json } => handle_list(json),
        Commands::Clean => handle_clean(),
        Commands::Dir { name } => handle_dir(name),
    }
}

#[derive(Subcommand)]
enum WorktreeCommands {
    /// Create a new git worktree
    Create {
        /// Name for the worktree (random BIP39 word if not provided)
        name: Option<String>,
    },
    /// Open an existing worktree and launch Claude
    Open {
        /// Name of the worktree to open (interactive selection if not provided)
        name: Option<String>,
    },
    /// Delete a worktree and clean up
    Delete {
        /// Name of the worktree to delete (current if not provided)
        name: Option<String>,
    },
    /// Add current worktree to xlaude management
    Add {
        /// Name for the worktree (defaults to current branch name)
        name: Option<String>,
    },
    /// Rename a worktree
    Rename {
        /// Current name of the worktree
        old_name: String,
        /// New name for the worktree
        new_name: String,
    },
    /// List all active Claude instances
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Clean up invalid worktrees from state
    Clean,
    /// Get the directory path of a worktree
    Dir {
        /// Name of the worktree (interactive selection if not provided)
        name: Option<String>,
    },
}
