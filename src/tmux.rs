use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

pub struct TmuxManager {
    session_prefix: String,
}

impl TmuxManager {
    pub fn new() -> Self {
        TmuxManager {
            session_prefix: "agentdev".to_string(),
        }
    }

    /// Check if tmux is available
    pub fn is_available() -> bool {
        Command::new("which")
            .arg("tmux")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Create a new tmux session for a project
    pub fn create_session(&self, project: &str, work_dir: &Path) -> Result<()> {
        let session_name = self.make_session_name(project);

        // Check if session already exists
        if self.session_exists(project) {
            return Ok(());
        }

        // Create custom tmux config
        let config_path = self.create_custom_config()?;

        // Create detached tmux session with custom config and start agent directly
        let (program, args) = crate::utils::resolve_agent_command()?;

        let mut tmux_args: Vec<String> = vec![
            "-f".into(),
            config_path.clone(),
            "new-session".into(),
            "-d".into(),
            "-s".into(),
            session_name.clone(),
            "-c".into(),
            work_dir.to_str().unwrap().to_string(),
        ];
        tmux_args.push(program);
        tmux_args.extend(args);

        let output = Command::new("tmux")
            .args(tmux_args.iter().map(|s| s.as_str()))
            .output()
            .context("Failed to create tmux session")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create tmux session: {}", stderr);
        }

        // Configure key bindings and status bar for the session
        self.configure_session_keys(&session_name)?;
        self.configure_session_status(&session_name)?;

        Ok(())
    }

    /// Create a session but start a specific program + args (overrides global agent).
    pub fn create_session_with_command(
        &self,
        project: &str,
        work_dir: &Path,
        program: &str,
        args: &[String],
    ) -> Result<()> {
        let session_name = self.make_session_name(project);

        if self.session_exists(project) {
            return Ok(());
        }

        let config_path = self.create_custom_config()?;

        let mut tmux_args: Vec<String> = vec![
            "-f".into(),
            config_path.clone(),
            "new-session".into(),
            "-d".into(),
            "-s".into(),
            session_name.clone(),
            "-c".into(),
            work_dir.to_str().unwrap().to_string(),
            program.to_string(),
        ];
        tmux_args.extend(args.iter().cloned());

        let output = Command::new("tmux")
            .args(tmux_args.iter().map(|s| s.as_str()))
            .output()
            .context("Failed to create tmux session")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create tmux session: {}", stderr);
        }

        self.configure_session_keys(&session_name)?;
        self.configure_session_status(&session_name)?;

        Ok(())
    }

    /// Check if a session exists
    pub fn session_exists(&self, project: &str) -> bool {
        let session_name = self.make_session_name(project);

        Command::new("tmux")
            .args(["has-session", "-t", &session_name])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Attach to a session
    pub fn attach_session(&self, project: &str) -> Result<()> {
        let session_name = self.make_session_name(project);

        if !self.session_exists(project) {
            anyhow::bail!("Session {} does not exist", session_name);
        }

        // Ensure status bar is configured before attaching
        self.configure_session_status(&session_name)?;

        // Set key bindings before attaching
        self.configure_session_keys(&session_name)?;

        // Directly attach to the session without transition screen
        let status = Command::new("tmux")
            .args(["attach-session", "-t", &session_name])
            .status()
            .context("Failed to attach to tmux session")?;

        // After detach, clear screen and flush output
        print!("\x1b[2J\x1b[H");
        std::io::Write::flush(&mut std::io::stdout())?;

        if !status.success() {
            eprintln!("Warning: tmux attach exited with non-zero status");
        }

        Ok(())
    }

    /// Kill a session
    pub fn kill_session(&self, project: &str) -> Result<()> {
        let session_name = self.make_session_name(project);

        if !self.session_exists(project) {
            return Ok(()); // Already gone
        }

        Command::new("tmux")
            .args(["kill-session", "-t", &session_name])
            .output()
            .context("Failed to kill tmux session")?;

        Ok(())
    }

    /// List all xlaude sessions
    pub fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        let output = Command::new("tmux")
            .args([
                "list-sessions",
                "-F",
                "#{session_name}:#{session_created}:#{session_attached}:#{session_activity}",
            ])
            .output()
            .context("Failed to list tmux sessions")?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut sessions = Vec::new();

        for line in output_str.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 4 {
                let full_name = parts[0];

                // Only process xlaude sessions (format: xlaude_project)
                let prefix = format!("{}_", self.session_prefix);
                if !full_name.starts_with(&prefix) {
                    continue;
                }

                // Extract project name from session name (xlaude_project)
                let project = if let Some(proj) = full_name.strip_prefix(&prefix) {
                    // Keep the safe name as-is for matching
                    proj.to_string()
                } else {
                    continue;
                };

                sessions.push(SessionInfo {
                    project,
                    created_at: parts[1].parse().unwrap_or(0),
                    is_attached: parts[2] != "0",
                    last_activity: parts[3].parse().unwrap_or(0),
                });
            }
        }

        Ok(sessions)
    }

    /// Capture recent output from a session
    pub fn capture_pane(&self, project: &str, lines: usize) -> Result<String> {
        let session_name = self.make_session_name(project);

        let output = Command::new("tmux")
            .args([
                "capture-pane",
                "-t",
                &session_name,
                "-p", // print to stdout
                "-S",
                &format!("-{}", lines), // last N lines
            ])
            .output()
            .context("Failed to capture pane")?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Send literal text to the session's pane
    pub fn send_text(&self, project: &str, text: &str) -> Result<()> {
        let session_name = self.make_session_name(project);
        let output = Command::new("tmux")
            .args(["send-keys", "-t", &session_name, "-l", text])
            .output()
            .context("Failed to send text to tmux session")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Provide a friendlier error when the pane/session is missing
            if stderr.contains("can't find pane") || stderr.contains("no such pane") {
                anyhow::bail!(
                    "tmux pane for session '{sess}' not found.\n\
This usually means the agent process exited immediately (e.g., binary not found).\n\
Tips:\n\
- Check sessions: `tmux ls`\n\
- Verify your agent command in the config file\n\
- Ensure the binary exists on PATH\n\
Original tmux error: {err}",
                    sess = session_name,
                    err = stderr.trim()
                );
            }
            anyhow::bail!("Failed to send keys: {}", stderr.trim());
        }
        Ok(())
    }

    /// Send Enter key
    pub fn send_enter(&self, project: &str) -> Result<()> {
        let session_name = self.make_session_name(project);
        let output = Command::new("tmux")
            .args(["send-keys", "-t", &session_name, "Enter"])
            .output()
            .context("Failed to send Enter to tmux session")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("can't find pane") || stderr.contains("no such pane") {
                anyhow::bail!(
                    "tmux pane for session '{sess}' not found.\n\
This usually means the agent process exited immediately (e.g., binary not found).\n\
Tips:\n\
- Check sessions: `tmux ls`\n\
- Verify your agent command in the config file\n\
- Ensure the binary exists on PATH\n\
Original tmux error: {err}",
                    sess = session_name,
                    err = stderr.trim()
                );
            }
            anyhow::bail!("Failed to send Enter: {}", stderr.trim());
        }
        Ok(())
    }

    /// Configure key bindings for a specific session
    fn configure_session_keys(&self, session_name: &str) -> Result<()> {
        // Set Ctrl+Q to detach (session-specific)
        Command::new("tmux")
            .args([
                "send-keys",
                "-t",
                session_name,
                "-X",
                "cancel", // Cancel any current mode
            ])
            .output()?;

        // Bind keys directly in the session
        Command::new("tmux")
            .args(["bind-key", "-n", "C-q", "detach-client"])
            .output()?;

        // Set Ctrl+T to toggle terminal pane on the right side
        // Using split-window with toggle logic to avoid flashing
        // Also focus on the new pane when opening
        let toggle_cmd = r#"if-shell "[ $(tmux list-panes | wc -l) -eq 1 ]" "split-window -h -l 50% -c '#{pane_current_path}' \; select-pane -t 1" "select-pane -t 0 \; kill-pane -t 1""#;
        Command::new("tmux")
            .args(["bind-key", "-n", "C-t", toggle_cmd])
            .output()?;

        // Set Ctrl+O to open editor
        // Check if editor is configured in xlaude state
        if let Ok(state) = crate::state::XlaudeState::load()
            && let Some(editor) = &state.editor
        {
            // Editor is configured, bind the key
            let editor_cmd = format!(r#"run-shell "{} '#{{pane_current_path}}' &""#, editor);
            Command::new("tmux")
                .args(["bind-key", "-n", "C-o", &editor_cmd])
                .output()?;
        }
        // If no editor configured, don't bind Ctrl+O - user should configure in dashboard

        // Configure pane borders for better visual separation
        Command::new("tmux")
            .args([
                "set-option",
                "-g",
                "pane-border-style",
                "fg=colour240,bg=colour235",
            ])
            .output()?;

        Command::new("tmux")
            .args([
                "set-option",
                "-g",
                "pane-active-border-style",
                "fg=colour45,bg=colour235",
            ])
            .output()?;

        Ok(())
    }

    /// Configure status bar for a specific session
    fn configure_session_status(&self, session_name: &str) -> Result<()> {
        // Set status bar on
        Command::new("tmux")
            .args(["set-option", "-t", session_name, "status", "on"])
            .output()?;

        // Set status position to top
        Command::new("tmux")
            .args(["set-option", "-t", session_name, "status-position", "top"])
            .output()?;

        // Set status style (menu bar style)
        Command::new("tmux")
            .args([
                "set-option",
                "-t",
                session_name,
                "status-style",
                "bg=colour238,fg=colour250",
            ])
            .output()?;

        // Set left section with project name
        let project = session_name
            .strip_prefix("agentdev_")
            .or_else(|| session_name.strip_prefix("xlaude_"))
            .unwrap_or(session_name);
        let left_text = format!(" 📂 agentdev: {} ", project.replace('_', "-"));
        Command::new("tmux")
            .args(["set-option", "-t", session_name, "status-left", &left_text])
            .output()?;

        // Set right section with shortcut hint
        Command::new("tmux")
            .args([
                "set-option",
                "-t",
                session_name,
                "status-right",
                " Ctrl+T: Terminal | Ctrl+O: Editor | Ctrl+Q: Dashboard ",
            ])
            .output()?;

        // Set lengths
        Command::new("tmux")
            .args(["set-option", "-t", session_name, "status-left-length", "50"])
            .output()?;

        Command::new("tmux")
            .args([
                "set-option",
                "-t",
                session_name,
                "status-right-length",
                "50",
            ])
            .output()?;

        // Clear center content
        Command::new("tmux")
            .args([
                "set-window-option",
                "-t",
                session_name,
                "window-status-current-format",
                "",
            ])
            .output()?;

        Command::new("tmux")
            .args([
                "set-window-option",
                "-t",
                session_name,
                "window-status-format",
                "",
            ])
            .output()?;

        Ok(())
    }

    /// Create custom tmux configuration
    fn create_custom_config(&self) -> Result<String> {
        let config = r##"# Menu bar style status at top
set -g status on
set -g status-position top
set -g status-style "bg=colour238,fg=colour250"
set -g status-left " 📂 agentdev "
set -g status-right " Ctrl+T: Terminal | Ctrl+O: Editor | Ctrl+Q: Dashboard "
set -g status-left-length 50
set -g status-right-length 50
set -g window-status-current-format ""
set -g window-status-format ""

# Hide other UI elements
set -g pane-border-status off
set -g display-panes-time 1
set -g display-time 1
set -g visual-activity off
set -g visual-bell off
set -g visual-silence off
set -g bell-action none

# Single key to return to dashboard
bind-key -n C-q detach-client

# Toggle terminal with Ctrl+T (right-side panel)
bind-key -n C-t if-shell "[ $(tmux list-panes | wc -l) -eq 1 ]" "split-window -h -l 50% -c '#{pane_current_path}' \; select-pane -t 1" "select-pane -t 0 \; kill-pane -t 1"

# Pane border styling for visual separation
set -g pane-border-style "fg=colour240,bg=colour235"
set -g pane-active-border-style "fg=colour45,bg=colour235"

# No prefix key - this is important for Ctrl+Q to work
set -g prefix None
unbind C-b

# Better colors
set -g default-terminal "screen-256color"
set -ga terminal-overrides ",*256col*:Tc"

# Large history
set -g history-limit 50000

# No delays
set -s escape-time 0

# Mouse support
set -g mouse on"##;

        // Use xlaude config directory instead of /tmp
        let config_dir = crate::state::get_config_dir()?;
        fs::create_dir_all(&config_dir)?;
        let config_path = config_dir.join("tmux.conf");
        fs::write(&config_path, config)?;
        Ok(config_path.to_string_lossy().to_string())
    }

    fn make_session_name(&self, project: &str) -> String {
        // Replace special characters that tmux doesn't like in session names
        let safe_project = project.replace(['-', '.'], "_");
        format!("{}_{}", self.session_prefix, safe_project)
    }

    /// Public helper to get the full tmux session name for a project/worktree
    pub fn session_name(&self, project: &str) -> String {
        self.make_session_name(project)
    }
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub project: String,
    pub created_at: i64,
    pub is_attached: bool,
    pub last_activity: i64,
}

impl SessionInfo {
    pub fn format_time(timestamp: i64) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Convert to i64 safely, clamping to i64::MAX if necessary
        let now = std::cmp::min(now, i64::MAX as u64) as i64;

        let diff = now - timestamp;

        if diff < 60 {
            format!("{diff}s ago")
        } else if diff < 3600 {
            format!("{}m ago", diff / 60)
        } else if diff < 86400 {
            format!("{}h ago", diff / 3600)
        } else {
            format!("{}d ago", diff / 86400)
        }
    }

    /// Format a human-friendly duration since the given timestamp.
    /// Examples: "42s", "5m", "2h 14m", "3d 6h"
    pub fn format_duration_since(timestamp: i64) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let now = std::cmp::min(now, i64::MAX as u64) as i64;

        let secs = now.saturating_sub(timestamp).max(0);

        if secs < 60 {
            return format!("{}s", secs);
        }

        let minutes = secs / 60;
        if minutes < 60 {
            return format!("{}m", minutes);
        }

        let hours = minutes / 60;
        if hours < 24 {
            let rem_m = minutes % 60;
            if rem_m == 0 {
                return format!("{}h", hours);
            }
            return format!("{}h {}m", hours, rem_m);
        }

        let days = hours / 24;
        let rem_h = hours % 24;
        if rem_h == 0 {
            format!("{}d", days)
        } else {
            format!("{}d {}h", days, rem_h)
        }
    }
}
