// Public modules for agentdev library
pub mod claude;
pub mod claude_status;
pub mod config;
pub mod git;
pub mod process_registry;
pub mod sessions;
pub mod state;
pub mod tmux;
pub mod utils;
pub mod web;

// Re-export commonly used types and functions
pub use config::{load_agent_config, split_cmdline};
pub use git::{execute_git, get_diff_for_path, get_repo_name, update_submodules};
pub use state::{WorktreeInfo, XlaudeState};
pub use tmux::TmuxManager;
pub use utils::{generate_random_name, resolve_agent_command};
