use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Default)]
pub struct AgentConfig {
    /// Map of agent alias -> full command line string
    pub agents: HashMap<String, String>,
}

/// Return the path to the agentdev config file.
pub fn agent_config_path() -> PathBuf {
    // ~/.config/agentdev/config.toml on Unix/macOS
    // %APPDATA%\agentdev\config\config.toml on Windows
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata)
                .join("agentdev")
                .join("config")
                .join("config.toml");
        }
        return PathBuf::from("config.toml");
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join(".config")
                .join("agentdev")
                .join("config.toml");
        }
        PathBuf::from(".agentdev.config.toml")
    }
}

/// Load agent pool configuration.
/// If config file is missing, return defaults with claude and codex.
pub fn load_agent_config() -> Result<AgentConfig> {
    let path = agent_config_path();
    if !path.exists() {
        // Generate a default config file on first run
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .with_context(|| format!("Failed to create config directory: {}", dir.display()))?;
        }

        let default_toml = r#"# agentdev configuration

# Define your agent pool. Left side is the alias you use with --agents, right side is the full command.
# Notes:
# - Commands are executed directly by tmux; shell aliases are NOT expanded.
# - Prefer absolute paths or prefix with `env KEY=VAL ...` to pass environment variables.
# - If shell features are required, wrap with a shell like: `bash -lic '<cmd>'`.
# - If a path contains spaces or parentheses, quote it accordingly.

[agents]
# Simple command
codex = "codex"

# Claude with explicit binary
claude = "/usr/local/bin/claude --dangerously-skip-permissions"

# With environment variables via `env` (no shell needed)
claude_env = "env ANTHROPIC_BASE_URL=https://api.anthropic.com ANTHROPIC_API_KEY=sk-xxx claude --dangerously-skip-permissions"

# With a shell (if you rely on aliases or need to source files)
# claude_bash = "bash -lic 'source ~/.secrets && claude --dangerously-skip-permissions'"

# Python project via uv (pyproject-based console script)
# Replace the project path and script name with your own.
my_py_agent = "uv run --project ~/code/agents/swe-bot swe-bot --mode code"

# Or use a module entry point if no console script is defined
# my_py_agent_mod = "uv run --project ~/code/agents/swe-bot python -m swe_bot.cli"

# If the path contains spaces or parentheses, quote it:
# my_py_agent_quoted = "uv run --project \"~/code/Agents (Py)/swe-bot\" swe-bot"
"#;
        fs::write(&path, default_toml)
            .with_context(|| format!("Failed to write default config: {}", path.display()))?;
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config: {}", path.display()))?;
    let cfg: AgentConfig = toml::from_str(&content).context("Failed to parse config.toml")?;
    Ok(cfg)
}

/// Parse a full command string into program + args using shell-style splitting.
pub fn split_cmdline(cmdline: &str) -> Result<(String, Vec<String>)> {
    let parts = shell_words::split(cmdline)
        .map_err(|e| anyhow::anyhow!("Invalid command line: {} ({e})", cmdline))?;
    if parts.is_empty() {
        anyhow::bail!("Command line is empty");
    }
    Ok((parts[0].clone(), parts[1..].to_vec()))
}
