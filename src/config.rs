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

        let default_toml =
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/config.example.toml"));
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
