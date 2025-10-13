// Public modules for agentdev library
pub mod config;
pub mod git;
pub mod state;
pub mod tmux;
pub mod utils;

// Re-export commonly used types and functions
pub use config::{load_agent_config, split_cmdline};
pub use git::{get_diff_for_path, get_repo_name, execute_git, update_submodules};
pub use state::{XlaudeState, WorktreeInfo};
pub use tmux::TmuxManager;
pub use utils::{resolve_agent_command, generate_random_name};


