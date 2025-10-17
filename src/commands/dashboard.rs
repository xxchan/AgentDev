use anyhow::{Context, Result};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use std::io;
use std::time::{Duration, Instant};

use crate::commands::{MergeStrategy, handle_delete, handle_delete_task, handle_merge};
use agentdev::claude_status::{ClaudeStatus, ClaudeStatusDetector, PanelStatus, to_panel_status};
use agentdev::git::{execute_git, get_diff_for_path, recent_git_logs, recent_git_logs_for_path};
use agentdev::state::XlaudeState;
use agentdev::tmux::{SessionInfo, TmuxManager};

pub struct Dashboard {
    tmux: TmuxManager,
    state: XlaudeState,
    sessions: Vec<SessionInfo>,
    worktrees: Vec<WorktreeDisplay>,
    selected: usize,
    list_index_map: Vec<Option<usize>>, // Maps list index to worktree index
    preview_cache: std::collections::HashMap<String, String>,
    list_state: ListState,
    show_help: bool,
    create_mode: bool,
    create_input: String,
    create_repo: Option<String>, // Repository context for creating worktree
    // Follow-up broadcast dialog
    follow_mode: bool,
    follow_input: String,
    status_message: Option<String>, // Status message to display
    status_message_timer: u8,       // Timer to clear status message
    status_detector: ClaudeStatusDetector,
    claude_statuses: std::collections::HashMap<String, ClaudeStatus>,
    config_mode: bool,
    config_editor_input: String,
    // Map list indices that are headers to their task_id
    header_task_map: std::collections::HashMap<usize, String>,
    // Vertical scroll offset for the preview pane
    preview_scroll: u16,
    // Last preview area for mouse targeting
    preview_area: Option<Rect>,
    // Debug toggles and metrics
    debug_mode: bool,
    dbg_last_frame_ms: u128,
    dbg_recent_lines: usize,
    dbg_diff_lines: usize,
    dbg_total_lines: usize,
    dbg_tmux_capture_ms: Option<u128>,
    dbg_tmux_throttled: bool,
    dbg_mouse_scroll_count: u32,
    dbg_mouse_window_start: Instant,
    // Cache raw diff text per worktree key (styled at render time)
    diff_cache: std::collections::HashMap<String, String>,
    // Auto-refresh diff: last status check per worktree key
    diff_last_check: std::collections::HashMap<String, std::time::Instant>,
    // Auto-refresh diff: last status fingerprint per worktree key
    diff_status_fingerprint: std::collections::HashMap<String, String>,
    // Throttle live tmux capture per worktree name
    preview_last_capture: std::collections::HashMap<String, std::time::Instant>,
    // In-dashboard confirmation modal for actions (delete/merge)
    confirm_dialog: Option<ConfirmDialog>,
}

struct WorktreeDisplay {
    name: String,
    repo: String,
    key: String,
    panel_status: PanelStatus,
    task_id: String,
}

impl Dashboard {
    fn current_selected_worktree_name(&self) -> String {
        if let Some(Some(idx)) = self.list_index_map.get(self.selected)
            && let Some(w) = self.worktrees.get(*idx)
        {
            return w.name.clone();
        }
        String::new()
    }
    // Refresh selected worktree's diff cache if status changed or timer elapsed
    fn maybe_refresh_diff_for(&mut self, key: &str, path: &std::path::Path) {
        let now = Instant::now();
        let should_check = self
            .diff_last_check
            .get(key)
            .map(|t| now.duration_since(*t) >= Duration::from_secs(5))
            .unwrap_or(true);
        if !should_check {
            return;
        }
        let repo = match path.to_str() {
            Some(s) => s,
            None => return,
        };
        if let Ok(status) = execute_git(&[
            "-C",
            repo,
            "-c",
            "core.quotepath=false",
            "status",
            "--porcelain",
            "-z",
        ]) {
            let changed = self
                .diff_status_fingerprint
                .get(key)
                .map(|old| old != &status)
                .unwrap_or(true);
            if changed {
                if let Ok(diff) = get_diff_for_path(path) {
                    self.diff_cache.insert(key.to_string(), diff);
                }
                self.diff_status_fingerprint.insert(key.to_string(), status);
            }
        }
        self.diff_last_check.insert(key.to_string(), now);
    }
    fn style_diff_lines(diff: &str, max_lines: usize) -> Vec<Line<'static>> {
        let mut out = Vec::new();
        for ln in diff.lines().take(max_lines) {
            let line = if ln.starts_with("+++") || ln.starts_with("---") {
                Line::from(Span::styled(
                    ln.to_string(),
                    Style::default().fg(Color::DarkGray),
                ))
            } else if ln.starts_with("diff ") {
                Line::from(Span::styled(
                    ln.to_string(),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ))
            } else if ln.starts_with("index ") {
                Line::from(Span::styled(
                    ln.to_string(),
                    Style::default().fg(Color::DarkGray),
                ))
            } else if ln.starts_with("@@") {
                Line::from(Span::styled(
                    ln.to_string(),
                    Style::default().fg(Color::Yellow),
                ))
            } else if ln.starts_with('+') {
                Line::from(Span::styled(
                    ln.to_string(),
                    Style::default().fg(Color::Green),
                ))
            } else if ln.starts_with('-') {
                Line::from(Span::styled(
                    ln.to_string(),
                    Style::default().fg(Color::Red),
                ))
            } else {
                Line::from(ln.to_string())
            };
            out.push(line);
        }
        out
    }
    pub fn new() -> Result<Self> {
        let tmux = TmuxManager::new();
        let state = XlaudeState::load()?;
        let sessions = tmux.list_sessions().unwrap_or_default();

        let mut dashboard = Dashboard {
            tmux,
            state,
            sessions,
            worktrees: Vec::new(),
            selected: 0,
            list_index_map: Vec::new(),
            preview_cache: std::collections::HashMap::new(),
            list_state: ListState::default(),
            show_help: false,
            create_mode: false,
            create_input: String::new(),
            create_repo: None,
            follow_mode: false,
            follow_input: String::new(),
            status_message: None,
            status_message_timer: 0,
            status_detector: ClaudeStatusDetector::new(),
            claude_statuses: std::collections::HashMap::new(),
            config_mode: false,
            config_editor_input: String::new(),
            header_task_map: std::collections::HashMap::new(),
            preview_scroll: 0,
            preview_area: None,
            debug_mode: false,
            dbg_last_frame_ms: 0,
            dbg_recent_lines: 0,
            dbg_diff_lines: 0,
            dbg_total_lines: 0,
            dbg_tmux_capture_ms: None,
            dbg_tmux_throttled: false,
            dbg_mouse_scroll_count: 0,
            dbg_mouse_window_start: Instant::now(),
            diff_cache: std::collections::HashMap::new(),
            diff_last_check: std::collections::HashMap::new(),
            diff_status_fingerprint: std::collections::HashMap::new(),
            preview_last_capture: std::collections::HashMap::new(),
            confirm_dialog: None,
        };

        dashboard.refresh_worktrees();
        dashboard.list_state.select(Some(0));

        Ok(dashboard)
    }

    fn refresh_worktrees(&mut self) {
        self.worktrees.clear();
        self.header_task_map.clear();

        // Collect all valid worktree names for cleanup
        let valid_worktree_names: std::collections::HashSet<String> = self
            .state
            .worktrees
            .values()
            .map(|info| info.name.clone())
            .collect();

        // Clean up tmux sessions for worktrees that no longer exist
        for session in &self.sessions {
            // Find the original worktree name from the session
            let worktree_name = self
                .state
                .worktrees
                .values()
                .find(|w| {
                    let safe_name = w.name.replace(['-', '.'], "_");
                    safe_name == session.project || w.name == session.project
                })
                .map(|w| w.name.clone());

            // If session exists but corresponding worktree doesn't, kill the session
            if worktree_name.is_none() {
                // Try to reconstruct the original name from session.project
                // session.project is the safe name (with underscores)
                // We need to check if any valid worktree matches this pattern
                let session_matches_any = valid_worktree_names
                    .iter()
                    .any(|name| name.replace(['-', '.'], "_") == session.project);

                if !session_matches_any {
                    // This session doesn't correspond to any existing worktree, kill it
                    if let Err(e) = self.tmux.kill_session(&session.project) {
                        eprintln!(
                            "Failed to clean up orphaned tmux session {}: {}",
                            session.project, e
                        );
                    }
                }
            }
        }

        // Get all worktrees from state
        for (key, info) in &self.state.worktrees {
            // Match session by converting name to safe format (same as tmux session name)
            let safe_name = info.name.replace(['-', '.'], "_");
            let session = self
                .sessions
                .iter()
                .find(|s| s.project == safe_name || s.project == info.name);

            let has_session = session.is_some();
            let panel_status =
                to_panel_status(has_session, self.claude_statuses.get(&info.name).cloned());

            self.worktrees.push(WorktreeDisplay {
                name: info.name.clone(),
                repo: info.repo_name.clone(),
                key: key.clone(),
                panel_status,
                task_id: info.task_id.clone().unwrap_or_else(|| info.name.clone()),
            });
        }

        // Sort by task then name for stable grouping
        self.worktrees
            .sort_by(|a, b| a.task_id.cmp(&b.task_id).then(a.name.cmp(&b.name)));
    }

    pub fn refresh(&mut self) -> Result<()> {
        // Reload state
        self.state = XlaudeState::load()?;

        // Refresh tmux sessions
        self.sessions = self.tmux.list_sessions().unwrap_or_default();

        // Update worktree list (this will also clean up orphaned sessions)
        self.refresh_worktrees();

        // Re-fetch sessions after cleanup
        self.sessions = self.tmux.list_sessions().unwrap_or_default();

        // Update Claude statuses for all running sessions
        self.claude_statuses.clear();
        for session in &self.sessions {
            // Find the original worktree name
            let worktree_name = self
                .state
                .worktrees
                .values()
                .find(|w| {
                    let safe_name = w.name.replace(['-', '.'], "_");
                    safe_name == session.project || w.name == session.project
                })
                .map(|w| w.name.clone())
                .unwrap_or(session.project.clone());

            // Capture pane output and analyze status
            if let Ok(output) = self.tmux.capture_pane(&worktree_name, 100) {
                let status = self.status_detector.analyze_output(&output);
                self.claude_statuses.insert(worktree_name.clone(), status);

                // Cache preview for inactive sessions
                if !session.is_attached {
                    self.preview_cache.insert(worktree_name, output);
                }
            }
        }

        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Initial refresh
        self.refresh()?;

        let result = self.run_loop(&mut terminal);

        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            DisableMouseCapture,
            LeaveAlternateScreen
        )?;
        terminal.show_cursor()?;

        result
    }

    fn run_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        loop {
            let frame_start = Instant::now();
            terminal.draw(|f| self.render(f))?;
            self.dbg_last_frame_ms = frame_start.elapsed().as_millis();

            // Clear status message after a few renders
            if self.status_message.is_some() {
                if self.status_message_timer > 0 {
                    self.status_message_timer -= 1;
                } else {
                    self.status_message = None;
                }
            }

            // Handle events with shorter timeout for more responsive updates
            if event::poll(Duration::from_millis(500))? {
                match event::read()? {
                    Event::Key(key) => match self.handle_input(key)? {
                        InputResult::Exit => break,
                        InputResult::Attach(project) => {
                            // Clean up terminal before attaching
                            disable_raw_mode()?;
                            execute!(
                                terminal.backend_mut(),
                                DisableMouseCapture,
                                LeaveAlternateScreen
                            )?;
                            terminal.show_cursor()?;

                            // Attach to tmux session
                            if let Err(e) = self.attach_to_project(&project) {
                                eprintln!("Failed to attach: {}", e);
                            }

                            // Restore terminal after detach
                            enable_raw_mode()?;
                            execute!(
                                terminal.backend_mut(),
                                EnterAlternateScreen,
                                EnableMouseCapture
                            )?;
                            terminal.hide_cursor()?;

                            // Force clear and redraw
                            terminal.clear()?;

                            // Refresh state after returning
                            self.refresh()?;
                        }
                        InputResult::DeleteWorktree(name) => {
                            // Leave TUI to run deletion safely (we already confirmed inside TUI)
                            disable_raw_mode()?;
                            execute!(
                                terminal.backend_mut(),
                                DisableMouseCapture,
                                LeaveAlternateScreen
                            )?;
                            terminal.show_cursor()?;
                            // Ensure downstream delete flows don't prompt again
                            let old_yes = std::env::var("XLAUDE_YES").ok();
                            let old_non = std::env::var("XLAUDE_NON_INTERACTIVE").ok();
                            unsafe {
                                std::env::set_var("XLAUDE_YES", "1");
                                std::env::set_var("XLAUDE_NON_INTERACTIVE", "1");
                            }
                            let res = handle_delete(Some(name.clone()));
                            // Restore env
                            unsafe {
                                if let Some(v) = old_yes {
                                    std::env::set_var("XLAUDE_YES", v);
                                } else {
                                    std::env::remove_var("XLAUDE_YES");
                                }
                                if let Some(v) = old_non {
                                    std::env::set_var("XLAUDE_NON_INTERACTIVE", v);
                                } else {
                                    std::env::remove_var("XLAUDE_NON_INTERACTIVE");
                                }
                            }

                            enable_raw_mode()?;
                            execute!(
                                terminal.backend_mut(),
                                EnterAlternateScreen,
                                EnableMouseCapture
                            )?;
                            terminal.hide_cursor()?;
                            terminal.clear()?;

                            match res {
                                Ok(_) => {
                                    self.status_message =
                                        Some(format!("✅ Deleted worktree: {}", name));
                                    self.status_message_timer = 5;
                                }
                                Err(e) => {
                                    self.status_message = Some(format!("❌ Delete failed: {}", e));
                                    self.status_message_timer = 8;
                                }
                            }
                            self.refresh()?;
                        }
                        InputResult::MergeWorktree(name) => {
                            disable_raw_mode()?;
                            execute!(
                                terminal.backend_mut(),
                                DisableMouseCapture,
                                LeaveAlternateScreen
                            )?;
                            terminal.show_cursor()?;

                            let res = handle_merge(
                                Some(name.clone()),
                                false,
                                false,
                                Some(MergeStrategy::Squash),
                                false,
                            );

                            enable_raw_mode()?;
                            execute!(
                                terminal.backend_mut(),
                                EnterAlternateScreen,
                                EnableMouseCapture
                            )?;
                            terminal.hide_cursor()?;
                            terminal.clear()?;

                            match res {
                                Ok(_) => {
                                    self.status_message =
                                        Some(format!("✅ Squash merged worktree: {}", name));
                                    self.status_message_timer = 6;
                                }
                                Err(e) => {
                                    self.status_message = Some(format!("❌ Merge failed: {}", e));
                                    self.status_message_timer = 8;
                                }
                            }
                            self.refresh()?;
                        }
                        InputResult::DeleteTask(task) => {
                            disable_raw_mode()?;
                            execute!(
                                terminal.backend_mut(),
                                DisableMouseCapture,
                                LeaveAlternateScreen
                            )?;
                            terminal.show_cursor()?;

                            // Ensure downstream delete flows don't prompt again
                            let old_yes = std::env::var("XLAUDE_YES").ok();
                            let old_non = std::env::var("XLAUDE_NON_INTERACTIVE").ok();
                            unsafe {
                                std::env::set_var("XLAUDE_YES", "1");
                                std::env::set_var("XLAUDE_NON_INTERACTIVE", "1");
                            }
                            let res = handle_delete_task(task.clone());
                            unsafe {
                                if let Some(v) = old_yes {
                                    std::env::set_var("XLAUDE_YES", v);
                                } else {
                                    std::env::remove_var("XLAUDE_YES");
                                }
                                if let Some(v) = old_non {
                                    std::env::set_var("XLAUDE_NON_INTERACTIVE", v);
                                } else {
                                    std::env::remove_var("XLAUDE_NON_INTERACTIVE");
                                }
                            }

                            enable_raw_mode()?;
                            execute!(
                                terminal.backend_mut(),
                                EnterAlternateScreen,
                                EnableMouseCapture
                            )?;
                            terminal.hide_cursor()?;
                            terminal.clear()?;

                            match res {
                                Ok(_) => {
                                    self.status_message = Some("✅ Deleted task".to_string());
                                    self.status_message_timer = 5;
                                }
                                Err(e) => {
                                    self.status_message = Some(format!("❌ Delete failed: {}", e));
                                    self.status_message_timer = 8;
                                }
                            }
                            self.refresh()?;
                        }
                        InputResult::CreateWorktree(name, repo) => {
                            // Find the repo path if specified
                            let repo_path = if let Some(repo_name) = &repo {
                                // Find the repo path from existing worktrees
                                // If worktree is at /path/parent/repo-worktree
                                // Then main repo should be at /path/parent/repo
                                if let Some(worktree) =
                                    self.worktrees.iter().find(|w| w.repo == *repo_name)
                                    && let Some(info) = self.state.worktrees.get(&worktree.key)
                                    && let Some(parent) = info.path.parent()
                                {
                                    let path = parent.join(repo_name);
                                    if path.exists() {
                                        Some(path)
                                    } else {
                                        // If not found, maybe it's the current directory
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            };

                            // Create the worktree quietly in background
                            let created_name =
                                match crate::commands::create::handle_create_in_dir_quiet(
                                    name, repo_path, true, None,
                                ) {
                                    Ok(name) => name,
                                    Err(e) => {
                                        // Show error message in status area (we'll add this)
                                        self.status_message =
                                            Some(format!("Failed to create worktree: {}", e));
                                        continue;
                                    }
                                };

                            // Refresh to show new worktree
                            self.refresh()?;

                            // Auto-focus on the newly created worktree
                            if let Some(repo_name) = repo {
                                let key = format!("{}/{}", repo_name, created_name);
                                // Find the new worktree in the list and set focus
                                for (idx, mapped_idx) in self.list_index_map.iter().enumerate() {
                                    if let Some(worktree_idx) = mapped_idx
                                        && let Some(worktree) = self.worktrees.get(*worktree_idx)
                                        && (worktree.key == key || worktree.name == created_name)
                                    {
                                        self.selected = idx;
                                        self.list_state.select(Some(idx));
                                        break;
                                    }
                                }
                            }

                            // Show success message
                            self.status_message =
                                Some(format!("✅ Created worktree: {}", created_name));
                            self.status_message_timer = 5; // Show for 5 seconds
                        }
                        InputResult::Continue => {}
                    },
                    Event::Mouse(me) => {
                        if let Some(area) = self.preview_area {
                            // crossterm uses 1-based coordinates
                            let x = me.column.saturating_sub(1);
                            let y = me.row.saturating_sub(1);
                            let in_preview = x >= area.x
                                && x < area.x + area.width
                                && y >= area.y
                                && y < area.y + area.height;
                            if in_preview {
                                match me.kind {
                                    MouseEventKind::ScrollDown => {
                                        self.preview_scroll = self.preview_scroll.saturating_add(3);
                                        // debug: count wheel events per second
                                        let now = Instant::now();
                                        if now.duration_since(self.dbg_mouse_window_start)
                                            >= Duration::from_secs(1)
                                        {
                                            self.dbg_mouse_window_start = now;
                                            self.dbg_mouse_scroll_count = 0;
                                        }
                                        self.dbg_mouse_scroll_count =
                                            self.dbg_mouse_scroll_count.saturating_add(1);
                                    }
                                    MouseEventKind::ScrollUp => {
                                        self.preview_scroll = self.preview_scroll.saturating_sub(3);
                                        let now = Instant::now();
                                        if now.duration_since(self.dbg_mouse_window_start)
                                            >= Duration::from_secs(1)
                                        {
                                            self.dbg_mouse_window_start = now;
                                            self.dbg_mouse_scroll_count = 0;
                                        }
                                        self.dbg_mouse_scroll_count =
                                            self.dbg_mouse_scroll_count.saturating_add(1);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    _ => {}
                }
            } else {
                // No automatic heavy refresh to keep scrolling smooth.
                // Live preview/status for the selected session is handled in render with throttling.
            }
        }

        Ok(())
    }

    fn handle_input(&mut self, key: KeyEvent) -> Result<InputResult> {
        // If a confirm modal is open, handle its input exclusively
        if let Some(ref mut confirm) = self.confirm_dialog {
            match key.code {
                KeyCode::Esc => {
                    // Cancel
                    self.confirm_dialog = None;
                    return Ok(InputResult::Continue);
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    confirm.yes_selected = true;
                    return Ok(InputResult::Continue);
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    confirm.yes_selected = false;
                    return Ok(InputResult::Continue);
                }
                KeyCode::Char('y' | 'Y') => {
                    let target = confirm.target.clone();
                    self.confirm_dialog = None;
                    return Ok(match target {
                        ConfirmTarget::DeleteWorktree(n) => InputResult::DeleteWorktree(n),
                        ConfirmTarget::DeleteTask(t) => InputResult::DeleteTask(t),
                        ConfirmTarget::MergeWorktree(n) => InputResult::MergeWorktree(n),
                    });
                }
                KeyCode::Char('n' | 'N') => {
                    self.confirm_dialog = None;
                    return Ok(InputResult::Continue);
                }
                KeyCode::Enter => {
                    let target = confirm.target.clone();
                    let yes = confirm.yes_selected;
                    self.confirm_dialog = None;
                    if yes {
                        return Ok(match target {
                            ConfirmTarget::DeleteWorktree(n) => InputResult::DeleteWorktree(n),
                            ConfirmTarget::DeleteTask(t) => InputResult::DeleteTask(t),
                            ConfirmTarget::MergeWorktree(n) => InputResult::MergeWorktree(n),
                        });
                    } else {
                        return Ok(InputResult::Continue);
                    }
                }
                _ => return Ok(InputResult::Continue),
            }
        }

        // Handle follow-up broadcast input
        if self.follow_mode {
            match key.code {
                KeyCode::Esc => {
                    // Cancel follow-up mode
                    self.follow_mode = false;
                    self.follow_input.clear();
                }
                KeyCode::Enter => {
                    // Send follow-up to all running agent sessions
                    let text = self.follow_input.trim().to_string();
                    self.follow_mode = false;
                    self.follow_input.clear();

                    if text.is_empty() {
                        self.status_message = Some("⚠️ Empty follow-up — cancelled".to_string());
                        self.status_message_timer = 5;
                        return Ok(InputResult::Continue);
                    }

                    // Broadcast: iterate over all known sessions and send text
                    let mut ok = 0usize;
                    let total = self.sessions.len();
                    for sess in &self.sessions {
                        // Wake the pane and slow-type the text, then Enter
                        // Use the tmux-safe project name directly
                        let _ = self.tmux.send_enter(&sess.project);
                        if slow_type(&self.tmux, &sess.project, &text).is_ok()
                            && self.tmux.send_enter(&sess.project).is_ok()
                        {
                            ok += 1;
                        }
                        // small spacing between sessions to avoid input clobbering
                        std::thread::sleep(std::time::Duration::from_millis(120));
                    }

                    self.status_message =
                        Some(format!("✅ Sent follow-up to {ok}/{total} agent(s)"));
                    self.status_message_timer = 6;
                }
                KeyCode::Backspace => {
                    self.follow_input.pop();
                }
                KeyCode::Char(c) => {
                    // Allow typical text input; basic filtering like create is not needed
                    self.follow_input.push(c);
                }
                _ => {}
            }
            return Ok(InputResult::Continue);
        }

        if self.show_help {
            self.show_help = false;
            return Ok(InputResult::Continue);
        }

        // Handle config mode input
        if self.config_mode {
            match key.code {
                KeyCode::Esc => {
                    // Cancel config mode without saving
                    self.config_mode = false;
                    // Restore original editor value
                    self.config_editor_input = self.state.editor.clone().unwrap_or_default();
                }
                KeyCode::Enter => {
                    // Save configuration
                    let editor = self.config_editor_input.trim();
                    if !editor.is_empty() {
                        self.state.editor = Some(editor.to_string());
                        self.state.save()?;
                        self.status_message = Some(format!("✅ Editor set to: {}", editor));
                        self.status_message_timer = 5;
                    }
                    self.config_mode = false;
                }
                KeyCode::Backspace => {
                    self.config_editor_input.pop();
                }
                KeyCode::Char(c) => {
                    self.config_editor_input.push(c);
                }
                _ => {}
            }
            return Ok(InputResult::Continue);
        }

        // Handle create mode input
        if self.create_mode {
            match key.code {
                KeyCode::Esc => {
                    // Cancel create mode
                    self.create_mode = false;
                    self.create_input.clear();
                }
                KeyCode::Enter => {
                    // Create worktree with entered name or use None for random name
                    let name = if self.create_input.trim().is_empty() {
                        None
                    } else {
                        Some(self.create_input.trim().to_string())
                    };
                    let repo = self.create_repo.clone();

                    // Exit create mode
                    self.create_mode = false;
                    self.create_input.clear();

                    // Create the worktree
                    return Ok(InputResult::CreateWorktree(name, repo));
                }
                KeyCode::Backspace => {
                    self.create_input.pop();
                }
                KeyCode::Char(c) => {
                    // Only allow alphanumeric, dash, and underscore
                    if c.is_alphanumeric() || c == '-' || c == '_' {
                        self.create_input.push(c);
                    }
                }
                _ => {}
            }
            return Ok(InputResult::Continue);
        }

        match key.code {
            KeyCode::Char('q' | 'Q') => {
                return Ok(InputResult::Exit);
            }
            KeyCode::Char('?' | 'h') => {
                self.show_help = true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected > 0 {
                    self.selected -= 1;
                    self.list_state.select(Some(self.selected));
                    self.preview_scroll = 0;
                    self.preview_last_capture
                        .remove(&self.current_selected_worktree_name());
                    // Force diff refresh for the newly selected worktree
                    if let Some(Some(worktree_idx)) = self.list_index_map.get(self.selected)
                        && let Some(worktree) = self.worktrees.get(*worktree_idx)
                    {
                        self.diff_cache.remove(&worktree.key);
                        self.diff_last_check.remove(&worktree.key);
                        self.diff_status_fingerprint.remove(&worktree.key);
                    }
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected + 1 < self.list_index_map.len() {
                    self.selected += 1;
                    self.list_state.select(Some(self.selected));
                    self.preview_scroll = 0;
                    self.preview_last_capture
                        .remove(&self.current_selected_worktree_name());
                    // Force diff refresh for the newly selected worktree
                    if let Some(Some(worktree_idx)) = self.list_index_map.get(self.selected)
                        && let Some(worktree) = self.worktrees.get(*worktree_idx)
                    {
                        self.diff_cache.remove(&worktree.key);
                        self.diff_last_check.remove(&worktree.key);
                        self.diff_status_fingerprint.remove(&worktree.key);
                    }
                }
            }
            KeyCode::PageDown | KeyCode::Char('J') => {
                // Scroll preview down by a chunk
                let inc: u16 = 5;
                // saturating add to avoid overflow
                self.preview_scroll = self.preview_scroll.saturating_add(inc);
            }
            KeyCode::PageUp | KeyCode::Char('K') => {
                // Scroll preview up by a chunk
                let dec: u16 = 5;
                self.preview_scroll = self.preview_scroll.saturating_sub(dec);
            }
            KeyCode::Home => {
                self.preview_scroll = 0;
            }
            KeyCode::Enter => {
                // Get the actual worktree index from the mapping
                if let Some(Some(worktree_idx)) = self.list_index_map.get(self.selected)
                    && let Some(worktree) = self.worktrees.get(*worktree_idx)
                {
                    return Ok(InputResult::Attach(worktree.name.clone()));
                }
                // If a header (task) is selected, do nothing (preview will show combined diff)
            }
            KeyCode::Char('n' | 'N') => {
                // Enter create mode with dialog
                self.create_mode = true;
                self.create_input.clear();

                // Determine repository context from current selection
                if let Some(Some(worktree_idx)) = self.list_index_map.get(self.selected)
                    && let Some(worktree) = self.worktrees.get(*worktree_idx)
                {
                    self.create_repo = Some(worktree.repo.clone());
                } else {
                    // Find the first repository in the list if no specific selection
                    self.create_repo = self.worktrees.first().map(|w| w.repo.clone());
                }
            }
            KeyCode::Char('d' | 'D') => {
                // Worktree selected -> confirm delete worktree; Task header selected -> confirm delete task
                if let Some(Some(worktree_idx)) = self.list_index_map.get(self.selected)
                    && let Some(worktree) = self.worktrees.get(*worktree_idx)
                {
                    self.confirm_dialog = Some(ConfirmDialog {
                        target: ConfirmTarget::DeleteWorktree(worktree.name.clone()),
                        yes_selected: false, // default to No
                    });
                } else if let Some(task) = self.header_task_map.get(&self.selected) {
                    self.confirm_dialog = Some(ConfirmDialog {
                        target: ConfirmTarget::DeleteTask(task.clone()),
                        yes_selected: false,
                    });
                }
            }
            KeyCode::Char('m' | 'M') => {
                if let Some(Some(worktree_idx)) = self.list_index_map.get(self.selected)
                    && let Some(worktree) = self.worktrees.get(*worktree_idx)
                {
                    self.confirm_dialog = Some(ConfirmDialog {
                        target: ConfirmTarget::MergeWorktree(worktree.name.clone()),
                        yes_selected: true, // default to Yes for merge convenience
                    });
                }
            }
            KeyCode::Char('f' | 'F') => {
                // Enter follow-up mode
                self.follow_mode = true;
                self.follow_input.clear();
            }
            KeyCode::Char('r' | 'R') => {
                self.refresh()?;
                self.preview_scroll = 0;
                self.diff_cache.clear();
                self.preview_last_capture.clear();
            }
            KeyCode::Char('g' | 'G') => {
                self.debug_mode = !self.debug_mode;
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                // Enter config mode
                self.config_mode = true;
                self.config_editor_input = self.state.editor.clone().unwrap_or_default();
            }
            _ => {}
        }

        Ok(InputResult::Continue)
    }

    fn attach_to_project(&mut self, project: &str) -> Result<()> {
        // Get worktree info
        let worktree = self
            .worktrees
            .iter()
            .find(|w| w.name == project)
            .context("Worktree not found")?;

        let info = self
            .state
            .worktrees
            .get(&worktree.key)
            .context("Worktree info not found")?;

        // Check if path exists
        if !info.path.exists() {
            anyhow::bail!("Worktree path does not exist: {}", info.path.display());
        }

        // Create session if it doesn't exist
        if !self.tmux.session_exists(project) {
            println!("Creating new tmux session for {}...", project);
            self.tmux.create_session(project, &info.path)?;
            // Give tmux time to initialize
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        // Attach to the session
        self.tmux.attach_session(project)?;

        Ok(())
    }

    fn render(&mut self, f: &mut Frame) {
        if self.show_help {
            self.render_help(f);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Menu bar
                Constraint::Min(10),   // Main content
                Constraint::Length(if self.status_message.is_some() { 3 } else { 2 }), // Help/status bar
            ])
            .split(f.area());

        // Menu bar (matching tmux style)
        let menu_bar = Paragraph::new(" 📂 agentdev: Dashboard").style(
            Style::default()
                .bg(Color::Rgb(68, 68, 68))
                .fg(Color::Rgb(250, 250, 250)),
        );
        f.render_widget(menu_bar, chunks[0]);

        // Main content area - split horizontally
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40), // Project list
                Constraint::Percentage(60), // Preview
            ])
            .split(chunks[1]);

        // Project list
        self.render_project_list(f, main_chunks[0]);

        // Preview pane
        self.preview_area = Some(main_chunks[1]);
        self.render_preview(f, main_chunks[1]);

        // Help/Status bar
        if let Some(ref status) = self.status_message {
            // Split the help area into status and help
            let help_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Status message
                    Constraint::Length(1), // Help line
                ])
                .split(chunks[2]);

            // Show status message
            let status_widget =
                Paragraph::new(format!(" {}", status)).style(Style::default().fg(Color::Green));
            f.render_widget(status_widget, help_chunks[0]);

            // Show help bar
            let mut spans = vec![
                Span::raw(" "),
                Span::styled("Enter", Style::default().fg(Color::Yellow)),
                Span::raw(" Open  "),
                Span::styled("n", Style::default().fg(Color::Yellow)),
                Span::raw(" New  "),
                Span::styled("d", Style::default().fg(Color::Yellow)),
                Span::raw(" Delete  "),
                Span::styled("m", Style::default().fg(Color::Yellow)),
                Span::raw(" Merge (squash)  "),
                Span::styled("f", Style::default().fg(Color::Yellow)),
                Span::raw(" Follow-up (all)  "),
                Span::styled("c", Style::default().fg(Color::Yellow)),
                Span::raw(" Config  "),
                Span::styled("r", Style::default().fg(Color::Yellow)),
                Span::raw(" Refresh  "),
                Span::styled("Mouse/Shift+J/K", Style::default().fg(Color::Yellow)),
                Span::raw(" Scroll right pane  "),
                Span::styled("g", Style::default().fg(Color::Yellow)),
                Span::raw(" Debug  "),
                Span::styled("?", Style::default().fg(Color::Yellow)),
                Span::raw(" Help  "),
                Span::styled("q", Style::default().fg(Color::Yellow)),
                Span::raw(" Quit "),
            ];
            if self.debug_mode {
                let wheel_elapsed =
                    Instant::now().saturating_duration_since(self.dbg_mouse_window_start);
                let wheel_rate = if wheel_elapsed.as_millis() > 0 {
                    (self.dbg_mouse_scroll_count as u128 * 1000 / wheel_elapsed.as_millis()) as u64
                } else {
                    0
                };
                let tmux_info = if let Some(ms) = self.dbg_tmux_capture_ms {
                    if self.dbg_tmux_throttled {
                        format!("{}ms thr", ms)
                    } else {
                        format!("{}ms", ms)
                    }
                } else if self.dbg_tmux_throttled {
                    "thr".to_string()
                } else {
                    "-".to_string()
                };
                let dbg_str = format!(
                    " | Dbg: frame {}ms lines {} (out {}, diff {}) tmux {} wheel {}/s",
                    self.dbg_last_frame_ms,
                    self.dbg_total_lines,
                    self.dbg_recent_lines,
                    self.dbg_diff_lines,
                    tmux_info,
                    wheel_rate
                );
                spans.push(Span::raw(dbg_str));
            }
            let help =
                Paragraph::new(Line::from(spans)).style(Style::default().fg(Color::DarkGray));
            f.render_widget(help, help_chunks[1]);
        } else {
            // Just show help bar
            let mut spans = vec![
                Span::raw(" "),
                Span::styled("Enter", Style::default().fg(Color::Yellow)),
                Span::raw(" Open  "),
                Span::styled("n", Style::default().fg(Color::Yellow)),
                Span::raw(" New  "),
                Span::styled("d", Style::default().fg(Color::Yellow)),
                Span::raw(" Delete  "),
                Span::styled("m", Style::default().fg(Color::Yellow)),
                Span::raw(" Merge (squash)  "),
                Span::styled("f", Style::default().fg(Color::Yellow)),
                Span::raw(" Follow-up (all)  "),
                Span::styled("c", Style::default().fg(Color::Yellow)),
                Span::raw(" Config  "),
                Span::styled("r", Style::default().fg(Color::Yellow)),
                Span::raw(" Refresh  "),
                Span::styled("Mouse/Shift+J/K", Style::default().fg(Color::Yellow)),
                Span::raw(" Scroll right pane  "),
                Span::styled("g", Style::default().fg(Color::Yellow)),
                Span::raw(" Debug  "),
                Span::styled("?", Style::default().fg(Color::Yellow)),
                Span::raw(" Help  "),
                Span::styled("q", Style::default().fg(Color::Yellow)),
                Span::raw(" Quit "),
            ];
            if self.debug_mode {
                let wheel_elapsed =
                    Instant::now().saturating_duration_since(self.dbg_mouse_window_start);
                let wheel_rate = if wheel_elapsed.as_millis() > 0 {
                    (self.dbg_mouse_scroll_count as u128 * 1000 / wheel_elapsed.as_millis()) as u64
                } else {
                    0
                };
                let tmux_info = if let Some(ms) = self.dbg_tmux_capture_ms {
                    if self.dbg_tmux_throttled {
                        format!("{}ms thr", ms)
                    } else {
                        format!("{}ms", ms)
                    }
                } else if self.dbg_tmux_throttled {
                    "thr".to_string()
                } else {
                    "-".to_string()
                };
                let dbg_str = format!(
                    " | Dbg: frame {}ms lines {} (out {}, diff {}) tmux {} wheel {}/s",
                    self.dbg_last_frame_ms,
                    self.dbg_total_lines,
                    self.dbg_recent_lines,
                    self.dbg_diff_lines,
                    tmux_info,
                    wheel_rate
                );
                spans.push(Span::raw(dbg_str));
            }
            let help =
                Paragraph::new(Line::from(spans)).style(Style::default().fg(Color::DarkGray));
            f.render_widget(help, chunks[2]);
        }

        // Render create dialog if in create mode
        if self.create_mode {
            self.render_create_dialog(f);
        }

        // Render config dialog if in config mode
        if self.config_mode {
            self.render_config_dialog(f);
        }

        // Render follow-up dialog if in follow mode
        if self.follow_mode {
            self.render_follow_dialog(f);
        }

        // Render confirm delete dialog if present
        if self.confirm_dialog.is_some() {
            self.render_confirm_dialog(f);
        }
    }

    fn render_project_list(&mut self, f: &mut Frame, area: Rect) {
        let mut items = Vec::new();
        self.list_index_map.clear();
        self.header_task_map.clear();

        // Group by task_id
        let mut groups: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (idx, wt) in self.worktrees.iter().enumerate() {
            groups.entry(wt.task_id.clone()).or_default().push(idx);
        }

        for (task, members) in groups {
            // Task header
            items.push(ListItem::new(Line::from(vec![Span::styled(
                format!("🧩 {}", task),
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            )])));
            let header_index = items.len() - 1;
            self.list_index_map.push(None);
            self.header_task_map.insert(header_index, task.clone());

            for worktree_idx in members {
                let worktree = &self.worktrees[worktree_idx];
                let status = worktree.panel_status.display_icon();
                let status_color = worktree.panel_status.color();

                // Try to display just agent alias if name is task-alias
                let display = if let Some(rest) = worktree.name.strip_prefix(&format!("{}-", task))
                {
                    rest.to_string()
                } else {
                    worktree.name.clone()
                };

                let item = Line::from(vec![
                    Span::raw("  "),
                    Span::styled(status, Style::default().fg(status_color)),
                    Span::raw(" "),
                    Span::raw(display),
                    Span::raw("  "),
                    Span::styled(
                        format!("({})", worktree.repo),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                items.push(ListItem::new(item));
                self.list_index_map.push(Some(worktree_idx));
            }
        }

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Tasks (↑↓ to navigate) "),
            )
            .highlight_style(Style::default().bg(Color::DarkGray))
            .highlight_symbol("> ");

        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_preview(&mut self, f: &mut Frame, area: Rect) {
        // Get the actual worktree from mapping
        let worktree_idx = self.list_index_map.get(self.selected).and_then(|idx| *idx);

        if let Some(idx) = worktree_idx {
            // Auto-refresh diff for selected worktree before borrowing it immutably for render
            if let Some(key) = self.worktrees.get(idx).map(|w| w.key.clone())
                && let Some(path) = self.state.worktrees.get(&key).map(|i| i.path.clone())
            {
                self.maybe_refresh_diff_for(&key, &path);
            }

            let worktree = match self.worktrees.get(idx) {
                Some(w) => w,
                None => return,
            };
            // reset debug counters for this frame
            self.dbg_recent_lines = 0;
            self.dbg_diff_lines = 0;
            self.dbg_tmux_capture_ms = None;
            self.dbg_tmux_throttled = false;
            let mut lines = vec![
                Line::from(vec![
                    Span::styled("Project: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(&worktree.name),
                ]),
                Line::from(vec![
                    Span::styled(
                        "Repository: ",
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(&worktree.repo),
                ]),
                Line::from(""),
            ];

            // Add session info (match by safe name)
            let safe_name = worktree.name.replace(['-', '.'], "_");
            // We collect recent output to render after the diff section
            let mut pending_preview: Option<String> = None;
            if let Some(session) = self
                .sessions
                .iter()
                .find(|s| s.project == safe_name || s.project == worktree.name)
            {
                // Try to fetch live pane output for selected background session
                let mut display_status: Option<ClaudeStatus> =
                    self.claude_statuses.get(&worktree.name).cloned();
                let mut live_preview: Option<String> = None;
                if !session.is_attached {
                    let now = Instant::now();
                    let should_capture = self
                        .preview_last_capture
                        .get(&worktree.name)
                        .map(|t| now.duration_since(*t) >= Duration::from_millis(200))
                        .unwrap_or(true);
                    if should_capture {
                        let cap_start = Instant::now();
                        if let Ok(output) = self.tmux.capture_pane(&worktree.name, 100) {
                            display_status = Some(self.status_detector.analyze_output(&output));
                            self.preview_cache
                                .insert(worktree.name.clone(), output.clone());
                            self.preview_last_capture.insert(worktree.name.clone(), now);
                            live_preview = Some(output);
                            self.dbg_tmux_capture_ms = Some(cap_start.elapsed().as_millis());
                        }
                    } else {
                        self.dbg_tmux_throttled = true;
                    }
                }

                // Show unified robust status first
                let panel_status = to_panel_status(true, display_status);
                lines.push(Line::from(vec![
                    Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(
                        format!(
                            "{} {}",
                            panel_status.display_icon(),
                            panel_status.display_text()
                        ),
                        Style::default().fg(panel_status.color()),
                    ),
                ]));
                // Show tmux session name and attach state
                let full_session_name = self.tmux.session_name(&worktree.name);
                lines.push(Line::from(vec![
                    Span::styled("Session: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(full_session_name),
                    Span::raw("  "),
                    Span::styled(
                        if session.is_attached {
                            "(Attached)"
                        } else {
                            "(Background)"
                        },
                        Style::default().fg(if session.is_attached {
                            Color::Green
                        } else {
                            Color::Yellow
                        }),
                    ),
                ]));
                // Show session timing info, plus total runtime
                lines.push(Line::from(vec![
                    Span::styled("Started: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(SessionInfo::format_time(session.created_at)),
                    Span::raw("  "),
                    Span::styled("Last: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(SessionInfo::format_time(session.last_activity)),
                    Span::raw("  "),
                    Span::styled("Total: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(SessionInfo::format_duration_since(session.created_at)),
                ]));
                // Show worktree path
                if let Some(info) = self.state.worktrees.get(&worktree.key) {
                    lines.push(Line::from(vec![
                        Span::styled("Path: ", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(info.path.to_string_lossy().to_string()),
                    ]));
                    // Show initial prompt if available
                    if let Some(ref prompt) = info.initial_prompt {
                        lines.push(Line::from(""));
                        lines.push(Line::from(Span::styled(
                            "Initial prompt:",
                            Style::default().add_modifier(Modifier::BOLD),
                        )));
                        lines.push(Line::from("─".repeat(area.width as usize - 2)));
                        for l in prompt.lines() {
                            lines.push(Line::from(l.to_string()));
                        }
                    }
                }
                lines.push(Line::from(""));

                // (initial prompt is shown in task header view)

                // (initial prompt for task is shown in header view)

                // Show preview if available (prefer live capture, fallback to cache)
                if !session.is_attached
                    && let Some(preview) =
                        live_preview.or_else(|| self.preview_cache.get(&worktree.name).cloned())
                {
                    // We'll render Recent output after Diff to match requested order
                    pending_preview = Some(preview);
                }
            } else {
                // No tmux session: show Exited status first
                let panel_status = PanelStatus::Exited;
                lines.push(Line::from(vec![
                    Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(
                        format!(
                            "{} {}",
                            panel_status.display_icon(),
                            panel_status.display_text()
                        ),
                        Style::default().fg(panel_status.color()),
                    ),
                ]));
                // Show would-be tmux session name and path
                let full_session_name = self.tmux.session_name(&worktree.name);
                lines.push(Line::from(vec![
                    Span::styled("Session: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(full_session_name),
                    Span::raw("  "),
                    Span::styled("(Not running)", Style::default().fg(Color::DarkGray)),
                ]));
                if let Some(info) = self.state.worktrees.get(&worktree.key) {
                    lines.push(Line::from(vec![
                        Span::styled("Path: ", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(info.path.to_string_lossy().to_string()),
                    ]));
                    // Show initial prompt if available
                    if let Some(ref prompt) = info.initial_prompt {
                        lines.push(Line::from(""));
                        lines.push(Line::from(Span::styled(
                            "Initial prompt:",
                            Style::default().add_modifier(Modifier::BOLD),
                        )));
                        lines.push(Line::from("─".repeat(area.width as usize - 2)));
                        for l in prompt.lines() {
                            lines.push(Line::from(l.to_string()));
                        }
                    }
                }
                lines.push(Line::from(""));
            }

            // Diff section comes immediately after Status
            // Always show git diff for this worktree (colored), regardless of session state
            if let Some(info) = self.state.worktrees.get(&worktree.key) {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Diff:",
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from("─".repeat(area.width as usize - 2)));
                let diff_text = if let Some(cached) = self.diff_cache.get(&worktree.key) {
                    cached.clone()
                } else {
                    match get_diff_for_path(&info.path) {
                        Ok(diff) => {
                            self.diff_cache.insert(worktree.key.clone(), diff.clone());
                            diff
                        }
                        Err(_) => String::new(),
                    }
                };

                if diff_text.is_empty() {
                    lines.push(Line::from(Span::styled(
                        "(no changes)",
                        Style::default().fg(Color::DarkGray),
                    )));
                } else {
                    let mut styled = Self::style_diff_lines(&diff_text, 200);
                    self.dbg_diff_lines = styled.len();
                    lines.append(&mut styled);
                }
            }

            // Git command logs (debug view)
            if self.debug_mode {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Git (recent):",
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from("─".repeat(area.width as usize - 2)));
                if let Some(info) = self.state.worktrees.get(&worktree.key) {
                    for entry in recent_git_logs_for_path(&info.path, 6) {
                        lines.push(Line::from(entry));
                    }
                }
            }

            // Recent output section comes after Diff
            if let Some(preview) = pending_preview {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Recent output:",
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from("─".repeat(area.width as usize - 2)));

                for line in preview.lines() {
                    self.dbg_recent_lines += 1;
                    lines.push(Line::from(line.to_string()));
                }
            }

            // record total lines before moving into Paragraph
            self.dbg_total_lines = lines.len();

            let preview = Paragraph::new(lines)
                .block(Block::default().borders(Borders::ALL).title(" Details "))
                .wrap(Wrap { trim: true })
                .scroll((self.preview_scroll, 0));

            f.render_widget(preview, area);
        } else {
            // Header selected: show aggregated diffs for this task
            if let Some(task) = self.header_task_map.get(&self.selected) {
                let mut lines: Vec<Line> = Vec::new();
                lines.push(Line::from(vec![Span::styled(
                    format!("Task: {} — Combined diffs", task),
                    Style::default().add_modifier(Modifier::BOLD),
                )]));
                lines.push(Line::from(""));
                // Show the task's initial prompt if available (from any member)
                if let Some(prompt) = {
                    let mut found: Option<String> = None;
                    for m in self.worktrees.iter().filter(|w| &w.task_id == task) {
                        if let Some(info) = self.state.worktrees.get(&m.key)
                            && let Some(ref p) = info.initial_prompt
                        {
                            found = Some(p.clone());
                            break;
                        }
                    }
                    found
                } {
                    lines.push(Line::from(Span::styled(
                        "Initial prompt:",
                        Style::default().add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::from("─".repeat(area.width as usize - 2)));
                    for l in prompt.lines() {
                        lines.push(Line::from(l.to_string()));
                    }
                    lines.push(Line::from(""));
                }

                // Collect members for this task
                let members: Vec<&WorktreeDisplay> = self
                    .worktrees
                    .iter()
                    .filter(|w| &w.task_id == task)
                    .collect();

                // Performance: cap total styled diff lines across all members
                let total_cap: usize = 250; // overall budget across task
                let per_member_cap: usize = 80; // per worktree budget
                let mut used: usize = 0;

                for m in members {
                    if used >= total_cap {
                        break;
                    }
                    // Find path from state
                    if let Some(info) = self.state.worktrees.get(&m.key) {
                        let alias = if let Some(rest) = m.name.strip_prefix(&format!("{}-", task)) {
                            rest.to_string()
                        } else {
                            m.name.clone()
                        };

                        lines.push(Line::from(Span::styled(
                            format!("== {} ==", alias),
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        )));

                        // Get diff text from cache or compute once
                        let diff_text = if let Some(cached) = self.diff_cache.get(&m.key) {
                            cached.clone()
                        } else {
                            match get_diff_for_path(&info.path) {
                                Ok(diff) => {
                                    self.diff_cache.insert(m.key.clone(), diff.clone());
                                    diff
                                }
                                Err(_) => String::new(),
                            }
                        };

                        if diff_text.is_empty() {
                            lines.push(Line::from(Span::styled(
                                "(no changes)",
                                Style::default().fg(Color::DarkGray),
                            )));
                        } else {
                            let remaining = total_cap.saturating_sub(used);
                            let take = remaining.min(per_member_cap);
                            let mut styled = Self::style_diff_lines(&diff_text, take);
                            used += styled.len();
                            lines.append(&mut styled);
                        }
                        lines.push(Line::from(""));
                    }
                }

                // Debug: record diff lines used in aggregated view
                self.dbg_diff_lines = used;
                self.dbg_total_lines = lines.len();

                // If truncated, add a hint
                if used >= total_cap {
                    lines.push(Line::from(Span::styled(
                        "(truncated — open a worktree for full diff)",
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::ITALIC),
                    )));
                }

                // Git command logs (debug view)
                if self.debug_mode {
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        "Git (recent):",
                        Style::default().add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::from("─".repeat(area.width as usize - 2)));
                    // Aggregated (header) view keeps global logs for now
                    for entry in recent_git_logs(6) {
                        lines.push(Line::from(entry));
                    }
                }

                let preview = Paragraph::new(lines)
                    .block(Block::default().borders(Borders::ALL).title(" Preview "))
                    .wrap(Wrap { trim: true })
                    .scroll((self.preview_scroll, 0));
                f.render_widget(preview, area);
            }
        }
    }

    fn render_help(&self, f: &mut Frame) {
        let help_text = vec![
            Line::from(""),
            Line::from(Span::styled(
                "agentdev dashboard - Help",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Navigation:",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("↑/k", Style::default().fg(Color::Yellow)),
                Span::raw("    Move up"),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("↓/j", Style::default().fg(Color::Yellow)),
                Span::raw("    Move down"),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Enter", Style::default().fg(Color::Yellow)),
                Span::raw("  Open selected project"),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Shift+J", Style::default().fg(Color::Yellow)),
                Span::raw("  Scroll preview (right) down"),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Shift+K", Style::default().fg(Color::Yellow)),
                Span::raw("  Scroll preview (right) up"),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Actions:",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("n", Style::default().fg(Color::Yellow)),
                Span::raw("      Create new worktree"),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("d", Style::default().fg(Color::Yellow)),
                Span::raw("      Delete task/worktree (based on selection)"),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("m", Style::default().fg(Color::Yellow)),
                Span::raw("      Squash merge selected worktree"),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("f", Style::default().fg(Color::Yellow)),
                Span::raw("      Send follow-up to ALL agents"),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("r", Style::default().fg(Color::Yellow)),
                Span::raw("      Refresh list"),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("q", Style::default().fg(Color::Yellow)),
                Span::raw("      Quit dashboard"),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "In Claude session:",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Ctrl+Q", Style::default().fg(Color::Yellow)),
                Span::raw(" Return to dashboard"),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Press any key to continue...",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )),
        ];

        let help = Paragraph::new(help_text)
            .block(Block::default().borders(Borders::ALL).title(" Help "))
            .alignment(Alignment::Left);

        let area = centered_rect(60, 80, f.area());
        f.render_widget(help, area);
    }

    fn render_create_dialog(&self, f: &mut Frame) {
        // Calculate dialog area (centered, 50% width, 30% height)
        let area = centered_rect(50, 30, f.area());

        // Clear the dialog area
        let clear = ratatui::widgets::Clear;
        f.render_widget(clear, area);

        // Create the dialog content
        let repo_text = self.create_repo.as_deref().unwrap_or("current repository");
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("Creating new worktree in {}", repo_text),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("Enter worktree name:"),
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{}_", self.create_input),
                    Style::default().bg(Color::DarkGray).fg(Color::White),
                ),
            ]),
            Line::from(""),
            Line::from(""),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(Color::Green)),
                Span::raw(" to create  "),
                Span::styled("Esc", Style::default().fg(Color::Red)),
                Span::raw(" to cancel"),
            ]),
        ];

        // If input is empty, show a hint
        if self.create_input.is_empty() {
            lines.insert(
                7,
                Line::from(Span::styled(
                    "  (leave empty for random name)",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                )),
            );
        }

        let dialog = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Create New Worktree ")
                    .border_style(Style::default().fg(Color::Blue)),
            )
            .alignment(Alignment::Center);

        f.render_widget(dialog, area);
    }

    fn render_config_dialog(&self, f: &mut Frame) {
        // Calculate dialog area (centered, 60% width, 40% height)
        let area = centered_rect(60, 40, f.area());

        // Clear the dialog area
        let clear = ratatui::widgets::Clear;
        f.render_widget(clear, area);

        // Create the dialog content
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "Configuration",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("Editor command for opening projects:"),
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{}_", self.config_editor_input),
                    Style::default().bg(Color::DarkGray).fg(Color::White),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Examples: zed, code, vim, nvim, subl, 'code -n'",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )),
            Line::from(""),
            Line::from("This editor will be used when pressing Ctrl+O in tmux sessions."),
            Line::from(""),
            Line::from(""),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(Color::Green)),
                Span::raw(" to save  "),
                Span::styled("Esc", Style::default().fg(Color::Red)),
                Span::raw(" to cancel"),
            ]),
        ];

        let dialog = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Configuration ")
                    .border_style(Style::default().fg(Color::Blue)),
            )
            .alignment(Alignment::Center);

        f.render_widget(dialog, area);
    }

    fn render_confirm_dialog(&self, f: &mut Frame) {
        let area = centered_rect(60, 30, f.area());
        let clear = ratatui::widgets::Clear;
        f.render_widget(clear, area);

        if let Some(ref confirm) = self.confirm_dialog {
            let (title, message, mut extra_lines) = match &confirm.target {
                ConfirmTarget::DeleteWorktree(name) => (
                    " Confirm Deletion ",
                    format!("Delete worktree '{}' ?", name),
                    vec![Line::from(Span::styled(
                        "This action cannot be undone.",
                        Style::default().fg(Color::Yellow),
                    ))],
                ),
                ConfirmTarget::DeleteTask(task) => (
                    " Confirm Deletion ",
                    format!("Delete ALL worktrees for task '{}' ?", task),
                    vec![Line::from(Span::styled(
                        "This action cannot be undone.",
                        Style::default().fg(Color::Yellow),
                    ))],
                ),
                ConfirmTarget::MergeWorktree(name) => (
                    " Confirm Merge ",
                    format!("Squash merge worktree '{}' ?", name),
                    vec![
                        Line::from(Span::styled(
                            "Runs `agentdev worktree merge --strategy squash`.",
                            Style::default()
                                .fg(Color::DarkGray)
                                .add_modifier(Modifier::ITALIC),
                        )),
                        Line::from(Span::styled(
                            "Push manually after review if needed.",
                            Style::default().fg(Color::DarkGray),
                        )),
                    ],
                ),
            };

            let mut lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    message,
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
            ];
            lines.append(&mut extra_lines);
            lines.push(Line::from(""));

            // Buttons row
            let yes_style = if confirm.yes_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Green)
            };
            let no_style = if !confirm.yes_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Red)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Red)
            };
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("[ Yes ]", yes_style),
                Span::raw("    "),
                Span::styled("[ No ]", no_style),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Use ←/→ to choose, Enter to confirm, Esc to cancel",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )));

            let dialog = Paragraph::new(lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(title)
                        .border_style(Style::default().fg(Color::Blue)),
                )
                .alignment(Alignment::Center);

            f.render_widget(dialog, area);
        }
    }

    fn render_follow_dialog(&self, f: &mut Frame) {
        let area = centered_rect(70, 30, f.area());
        let clear = ratatui::widgets::Clear;
        f.render_widget(clear, area);

        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "Broadcast follow-up to ALL agents",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("Enter follow-up message:"),
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{}_", self.follow_input),
                    Style::default().bg(Color::DarkGray).fg(Color::White),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Note: This will type into every running agent session.",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(Color::Green)),
                Span::raw(" to send  "),
                Span::styled("Esc", Style::default().fg(Color::Red)),
                Span::raw(" to cancel"),
            ]),
        ];

        if self.follow_input.is_empty() {
            lines.insert(
                5,
                Line::from(Span::styled(
                    "(leave empty to cancel)",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                )),
            );
        }

        let dialog = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Follow-up ")
                    .border_style(Style::default().fg(Color::Blue)),
            )
            .alignment(Alignment::Center);

        f.render_widget(dialog, area);
    }
}

enum InputResult {
    Exit,
    Attach(String),
    CreateWorktree(Option<String>, Option<String>), // optional name and optional repo context
    DeleteWorktree(String),
    DeleteTask(String),
    MergeWorktree(String),
    Continue,
}

#[derive(Clone)]
enum ConfirmTarget {
    DeleteWorktree(String),
    DeleteTask(String),
    MergeWorktree(String),
}

#[derive(Clone)]
struct ConfirmDialog {
    target: ConfirmTarget,
    yes_selected: bool,
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

pub fn handle_dashboard() -> Result<()> {
    // Check if tmux is available
    if !TmuxManager::is_available() {
        anyhow::bail!(
            "tmux is not installed. Please install tmux to use the dashboard feature.\n\
             On macOS: brew install tmux\n\
             On Ubuntu/Debian: apt-get install tmux"
        );
    }

    let mut dashboard = Dashboard::new()?;
    dashboard.run()?;

    Ok(())
}

/// Slowly type text into a tmux session to reduce input drop issues.
fn slow_type(tmux: &TmuxManager, project: &str, text: &str) -> Result<()> {
    let chunk_size: usize = std::env::var("AGENTDEV_SLOW_TYPE_CHUNK")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(80);
    let delay_ms: u64 = std::env::var("AGENTDEV_SLOW_TYPE_DELAY_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(40);
    let mut i = 0;
    let chars: Vec<char> = text.chars().collect();
    while i < chars.len() {
        let end = std::cmp::min(i + chunk_size, chars.len());
        let chunk: String = chars[i..end].iter().collect();
        tmux.send_text(project, &chunk)?;
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        i = end;
    }
    Ok(())
}
