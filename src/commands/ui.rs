use anyhow::{Context, Result};
use std::env;
use std::process::Command;

pub fn handle_ui(port: u16) -> Result<()> {
    println!("🚀 Starting AgentDev Web UI server on port {}...", port);

    // Check if we're in the right directory structure
    let current_dir = env::current_dir().context("Failed to get current directory")?;
    let backend_path = current_dir.join("apps").join("backend");

    if !backend_path.exists() {
        anyhow::bail!(
            "UI backend not found. Expected to find backend at: {}\n\
            Make sure you're running this command from the agentdev project root.",
            backend_path.display()
        );
    }

    // Build and run the UI backend server
    println!("📦 Building UI backend...");
    let build_status = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .arg("--manifest-path")
        .arg(backend_path.join("Cargo.toml"))
        .status()
        .context("Failed to execute cargo build")?;

    if !build_status.success() {
        anyhow::bail!("Failed to build UI backend");
    }

    println!("🌐 Starting UI server...");

    // Set environment variables
    let mut cmd = Command::new("cargo");
    cmd.arg("run")
        .arg("--release")
        .arg("--manifest-path")
        .arg(backend_path.join("Cargo.toml"))
        .arg("--bin")
        .arg("agentdev-ui")
        .env("PORT", port.to_string())
        .current_dir(&backend_path);

    // Execute the server (this will block until interrupted)
    let status = cmd.status().context("Failed to start UI server")?;

    if !status.success() {
        anyhow::bail!("UI server exited with error");
    }

    println!("✅ AgentDev UI server stopped");
    Ok(())
}
