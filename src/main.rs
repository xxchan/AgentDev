use agentdev::load_agent_config;
use anyhow::Result;
use clap::{Parser, Subcommand};
use clap_complete::Shell;

mod commands;
mod completions;
mod input;

use commands::{
    MergeStrategy, handle_add, handle_clean, handle_create, handle_delete, handle_delete_task_cli,
    handle_dir, handle_exec, handle_list, handle_merge, handle_open, handle_rename,
    handle_sessions_list, handle_start, handle_ui,
};

#[derive(Parser)]
#[command(name = "agentdev")]
#[command(
    about = "Manage agent instances with git worktrees",
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
    #[command(alias = "wt", alias = "x")]
    Worktree {
        #[command(subcommand)]
        cmd: WorktreeCommands,
    },
    /// Session inspection commands
    Sessions {
        #[command(subcommand)]
        cmd: SessionCommands,
    },
    // Backward-compatible top-level commands (temporarily retained)
    #[command(hide = true)]
    Create {
        /// Name for the worktree (random BIP39 word if not provided)
        name: Option<String>,
        /// Agent command to use (overrides global config)
        #[arg(long)]
        agent: Option<String>,
    },
    #[command(hide = true)]
    Open {
        /// Name of the worktree to open (interactive selection if not provided)
        name: Option<String>,
        /// Agent command to use (overrides global config)
        #[arg(long)]
        agent: Option<String>,
    },
    #[command(hide = true, alias = "rm")]
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
    #[command(hide = true, alias = "ls")]
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
    #[command(alias = "dash")]
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
    /// Launch web UI for agent management
    Ui {
        /// Port to run the web server on
        #[arg(long, default_value = "3000")]
        port: u16,
        /// Host to bind the web server to (e.g. 0.0.0.0 to allow remote access)
        #[arg(long)]
        host: Option<std::net::IpAddr>,
    },
}

fn main() -> Result<()> {
    // Eagerly ensure agent config exists and is loadable on every invocation
    let _ = load_agent_config();

    let cli = Cli::parse();

    match cli.command {
        Commands::Worktree { cmd } => match cmd {
            WorktreeCommands::Create { name, agent } => handle_create(name, agent),
            WorktreeCommands::Open { name, agent } => handle_open(name, agent),
            WorktreeCommands::Delete { name } => handle_delete(name),
            WorktreeCommands::Add { name } => handle_add(name),
            WorktreeCommands::Rename { old_name, new_name } => handle_rename(old_name, new_name),
            WorktreeCommands::List { json } => handle_list(json),
            WorktreeCommands::Clean => handle_clean(),
            WorktreeCommands::Dir { name } => handle_dir(name),
            WorktreeCommands::Exec { worktree, command } => handle_exec(worktree, command),
            WorktreeCommands::Merge {
                name,
                push,
                cleanup,
                strategy,
                squash,
            } => handle_merge(name, push, cleanup, strategy, squash),
        },
        Commands::Sessions { cmd } => match cmd {
            SessionCommands::List {
                worktree,
                all,
                json,
            } => handle_sessions_list(worktree, all, json),
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
        Commands::Ui { port, host } => handle_ui(port, host),
        // Backward-compatible routing
        Commands::Create { name, agent } => handle_create(name, agent),
        Commands::Open { name, agent } => handle_open(name, agent),
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
        /// Agent command to use (overrides global config)
        #[arg(long)]
        agent: Option<String>,
    },
    /// Open an existing worktree and launch Claude
    Open {
        /// Name of the worktree to open (interactive selection if not provided)
        name: Option<String>,
        /// Agent command to use (overrides global config)
        #[arg(long)]
        agent: Option<String>,
    },
    /// Delete a worktree and clean up
    #[command(alias = "rm")]
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
    /// List all active instances
    #[command(alias = "ls")]
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
    /// Execute a command inside a worktree
    Exec {
        /// Name of the worktree to target (interactive selection if omitted)
        #[arg(long)]
        worktree: Option<String>,
        /// Command to execute inside the worktree
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Merge a worktree branch into the default branch
    Merge {
        /// Name of the worktree to merge (current if not provided)
        name: Option<String>,
        /// Push the default branch after a successful merge
        #[arg(long)]
        push: bool,
        /// Delete the worktree after a successful merge
        #[arg(long)]
        cleanup: bool,
        /// Merge strategy (defaults to ff-only unless --squash)
        #[arg(long, value_enum)]
        strategy: Option<MergeStrategy>,
        /// Shortcut for --strategy squash
        #[arg(long)]
        squash: bool,
    },
}

#[derive(Subcommand)]
enum SessionCommands {
    /// List known sessions grouped by provider and worktree
    List {
        /// Filter sessions by worktree key or name
        #[arg(long)]
        worktree: Option<String>,
        /// Include sessions without a tracked worktree association
        #[arg(long)]
        all: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}
