use anyhow::{anyhow, Context, Result};
use axum::{
    extract::{Path as AxumPath, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use uuid::Uuid;

use crate::AppState;

use agentdev::{
    claude::get_claude_sessions,
    config::{load_agent_config, split_cmdline},
    git::{
        collect_worktree_diff_breakdown, commits_since_merge_base, execute_git, get_diff_for_path,
        get_repo_name, head_commit_info, summarize_worktree_status, update_submodules,
        CommitsAhead, HeadCommitInfo, WorktreeGitStatus,
    },
    sessions::{canonicalize as canonicalize_session_path, default_providers, SessionRecord},
    state::{WorktreeInfo, XlaudeState},
    tmux::TmuxManager,
    utils::generate_random_name,
};
use rayon::prelude::*;

#[derive(Serialize, Deserialize, Clone)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub agents: Vec<Agent>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub alias: String,
    pub status: AgentStatus,
    pub worktree_path: Option<String>,
    pub tmux_session: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum AgentStatus {
    Initializing,
    Ready,
    Running,
    Error(String),
}

#[derive(Deserialize)]
pub struct CreateTaskRequest {
    pub prompt: String,
    pub agents: Option<Vec<String>>, // Agent aliases to use, default to all configured agents
    pub name: Option<String>,        // Task name, default to random BIP39 words
}

#[derive(Serialize)]
pub struct CreateTaskResponse {
    pub task: Task,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WorktreeSessionSummary {
    pub provider: String,
    pub session_id: String,
    pub last_user_message: String,
    pub last_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub user_messages: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WorktreeGitStatusPayload {
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
    pub conflicts: usize,
    pub is_clean: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WorktreeCommitPayload {
    pub commit_id: String,
    pub summary: String,
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WorktreeCommitsAheadPayload {
    pub base_branch: String,
    pub merge_base: Option<String>,
    pub commits: Vec<WorktreeCommitPayload>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WorktreeSummary {
    pub id: String,
    pub name: String,
    pub branch: String,
    pub repo_name: String,
    pub path: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_activity_at: chrono::DateTime<chrono::Utc>,
    pub task_id: Option<String>,
    pub task_name: Option<String>,
    pub initial_prompt: Option<String>,
    pub agent_alias: Option<String>,
    pub git_status: Option<WorktreeGitStatusPayload>,
    pub head_commit: Option<WorktreeCommitPayload>,
    pub commits_ahead: Option<WorktreeCommitsAheadPayload>,
    pub sessions: Vec<WorktreeSessionSummary>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeProcessStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WorktreeProcessSummary {
    pub id: String,
    pub command: Vec<String>,
    pub status: WorktreeProcessStatus,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub exit_code: Option<i32>,
    pub cwd: Option<String>,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WorktreeProcessListResponse {
    pub processes: Vec<WorktreeProcessSummary>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WorktreeListResponse {
    pub worktrees: Vec<WorktreeSummary>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WorktreeCommitDiffPayload {
    pub reference: String,
    pub diff: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WorktreeFileDiffPayload {
    pub path: String,
    pub display_path: String,
    pub status: String,
    pub diff: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WorktreeGitDetailsPayload {
    pub commit_diff: Option<WorktreeCommitDiffPayload>,
    pub staged: Vec<WorktreeFileDiffPayload>,
    pub unstaged: Vec<WorktreeFileDiffPayload>,
    pub untracked: Vec<WorktreeFileDiffPayload>,
}

impl From<WorktreeGitStatus> for WorktreeGitStatusPayload {
    fn from(value: WorktreeGitStatus) -> Self {
        Self {
            branch: value.branch,
            upstream: value.upstream,
            ahead: value.ahead,
            behind: value.behind,
            staged: value.staged,
            unstaged: value.unstaged,
            untracked: value.untracked,
            conflicts: value.conflicts,
            is_clean: value.is_clean,
        }
    }
}

impl From<HeadCommitInfo> for WorktreeCommitPayload {
    fn from(value: HeadCommitInfo) -> Self {
        Self {
            commit_id: value.commit_id,
            summary: value.summary,
            timestamp: value.timestamp,
        }
    }
}

impl From<CommitsAhead> for WorktreeCommitsAheadPayload {
    fn from(value: CommitsAhead) -> Self {
        Self {
            base_branch: value.base_branch,
            merge_base: value.merge_base,
            commits: value
                .commits
                .into_iter()
                .map(WorktreeCommitPayload::from)
                .collect(),
        }
    }
}

static WORKTREE_WARNINGS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn warn_once(operation: &str, path: &Path, render: impl FnOnce() -> String) {
    let key = format!("{operation}|{}", path.display());
    let set = WORKTREE_WARNINGS.get_or_init(|| Mutex::new(HashSet::new()));
    let mut guard = set.lock().expect("worktree warnings mutex poisoned");
    if guard.insert(key) {
        let message = render();
        drop(guard);
        eprintln!("{message}");
    }
}

fn git_metadata_present(path: &Path) -> bool {
    let git_entry = path.join(".git");
    if git_entry.is_dir() {
        return true;
    }
    if git_entry.is_file() {
        if let Ok(contents) = fs::read_to_string(&git_entry) {
            for line in contents.lines() {
                if let Some(rest) = line.strip_prefix("gitdir:") {
                    let trimmed = rest.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let candidate = Path::new(trimmed);
                    let resolved = if candidate.is_absolute() {
                        candidate.to_path_buf()
                    } else {
                        path.join(candidate)
                    };
                    if resolved.exists() {
                        return true;
                    }
                }
            }
        }
    }
    false
}

struct NormalizedSession {
    record: SessionRecord,
    canonical_dir: Option<PathBuf>,
}

#[derive(Clone)]
struct WorktreeProfiler {
    enabled: bool,
}

impl WorktreeProfiler {
    fn new() -> Self {
        let enabled = std::env::var("AGENTDEV_PROFILE_WORKTREES")
            .map(|value| value != "0")
            .unwrap_or(false);
        Self { enabled }
    }

    fn enabled(&self) -> bool {
        self.enabled
    }

    fn measure<T, F>(&self, label: &str, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        if !self.enabled {
            return f();
        }
        let start = Instant::now();
        let value = f();
        println!("[profile/worktrees] {label} took {:?}", start.elapsed());
        value
    }

    fn measure_result<T, E, F>(&self, label: &str, f: F) -> std::result::Result<T, E>
    where
        F: FnOnce() -> std::result::Result<T, E>,
    {
        if !self.enabled {
            return f();
        }
        let start = Instant::now();
        let result = f();
        println!("[profile/worktrees] {label} took {:?}", start.elapsed());
        result
    }

    fn measure_worktree<T, F>(&self, worktree_id: &str, label: &str, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        if !self.enabled {
            return f();
        }
        let start = Instant::now();
        let value = f();
        println!(
            "[profile/worktrees] {worktree_id}::{label} took {:?}",
            start.elapsed()
        );
        value
    }

    fn measure_worktree_result<T, E, F>(
        &self,
        worktree_id: &str,
        label: &str,
        f: F,
    ) -> std::result::Result<T, E>
    where
        F: FnOnce() -> std::result::Result<T, E>,
    {
        if !self.enabled {
            return f();
        }
        let start = Instant::now();
        let result = f();
        println!(
            "[profile/worktrees] {worktree_id}::{label} took {:?}",
            start.elapsed()
        );
        result
    }
}

fn collect_external_sessions(profiler: &WorktreeProfiler) -> Vec<NormalizedSession> {
    profiler.measure("sessions.total", || {
        let mut collected = Vec::new();

        for provider in default_providers() {
            let provider_name = provider.name();
            let records_result = if profiler.enabled() {
                let start = Instant::now();
                let outcome = provider.list_sessions();
                println!(
                    "[profile/worktrees] sessions::{provider_name} took {:?}",
                    start.elapsed()
                );
                outcome
            } else {
                provider.list_sessions()
            };

            match records_result {
                Ok(records) => {
                    for record in records {
                        let canonical_dir = record
                            .working_dir
                            .as_ref()
                            .and_then(|dir| canonicalize_session_path(dir));
                        collected.push(NormalizedSession {
                            canonical_dir,
                            record,
                        });
                    }
                }
                Err(err) => {
                    eprintln!("⚠️  Failed to list sessions from {}: {err}", provider_name);
                }
            }
        }

        collected
    })
}

fn match_sessions_for_worktree(
    info: &WorktreeInfo,
    external_sessions: &[NormalizedSession],
) -> Vec<WorktreeSessionSummary> {
    if external_sessions.is_empty() {
        return Vec::new();
    }

    let canonical_worktree = canonicalize_session_path(&info.path);
    let base_path = canonical_worktree
        .as_ref()
        .map(PathBuf::as_path)
        .unwrap_or_else(|| info.path.as_path());

    let mut summaries = Vec::new();
    for session in external_sessions {
        let working_dir = session
            .canonical_dir
            .as_ref()
            .map(PathBuf::as_path)
            .or_else(|| session.record.working_dir.as_ref().map(PathBuf::as_path));

        let Some(working_dir) = working_dir else {
            continue;
        };

        if working_dir.starts_with(base_path) {
            summaries.push(WorktreeSessionSummary {
                provider: session.record.provider.clone(),
                session_id: session.record.id.clone(),
                last_user_message: session.record.last_user_message.clone().unwrap_or_default(),
                last_timestamp: session.record.last_timestamp,
                user_messages: session.record.user_messages.clone(),
            });
        }
    }
    summaries
}

/// GET /api/worktrees - Get enriched worktree metadata
pub async fn get_worktrees() -> impl IntoResponse {
    match tokio::task::spawn_blocking(collect_worktree_summaries).await {
        Ok(Ok(response)) => Json(response).into_response(),
        Ok(Err(err)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to collect worktrees: {err}"),
        )
            .into_response(),
        Err(join_err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Worktree collection task failed: {join_err}"),
        )
            .into_response(),
    }
}

/// GET /api/worktrees/:id - Get metadata for a specific worktree
pub async fn get_worktree(AxumPath(worktree_id): AxumPath<String>) -> impl IntoResponse {
    let id_for_error = worktree_id.clone();
    match tokio::task::spawn_blocking(move || collect_worktree_summary(worktree_id)).await {
        Ok(Ok(Some(summary))) => Json(summary).into_response(),
        Ok(Ok(None)) => (
            StatusCode::NOT_FOUND,
            format!("Worktree {id_for_error} not found"),
        )
            .into_response(),
        Ok(Err(err)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load worktree: {err}"),
        )
            .into_response(),
        Err(join_err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Worktree lookup task failed: {join_err}"),
        )
            .into_response(),
    }
}

/// GET /api/worktrees/:id/processes - List active and recent processes for a worktree
pub async fn get_worktree_processes(AxumPath(worktree_id): AxumPath<String>) -> impl IntoResponse {
    let id_for_error = worktree_id.clone();
    match tokio::task::spawn_blocking(move || collect_worktree_processes(&worktree_id)).await {
        Ok(Ok(Some(response))) => Json(response).into_response(),
        Ok(Ok(None)) => (
            StatusCode::NOT_FOUND,
            format!("Worktree {id_for_error} not found"),
        )
            .into_response(),
        Ok(Err(err)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load worktree processes: {err}"),
        )
            .into_response(),
        Err(join_err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Worktree processes task failed: {join_err}"),
        )
            .into_response(),
    }
}

fn collect_worktree_summaries() -> Result<WorktreeListResponse> {
    let profiler = WorktreeProfiler::new();
    let overall_start = if profiler.enabled() {
        Some(Instant::now())
    } else {
        None
    };

    let state = profiler.measure_result("state.load", || XlaudeState::load())?;

    let session_profiler = profiler.clone();
    let session_handle = std::thread::spawn(move || collect_external_sessions(&session_profiler));

    let worktree_entries: Vec<(String, WorktreeInfo)> = state
        .worktrees
        .iter()
        .map(|(id, info)| (id.clone(), info.clone()))
        .collect();

    let external_sessions = session_handle.join().unwrap_or_else(|_| Vec::new());
    let external_sessions = Arc::new(external_sessions);

    let mut summaries: Vec<WorktreeSummary> = worktree_entries
        .into_par_iter()
        .map(|(id, info)| {
            let profiler = profiler.clone();
            let sessions = external_sessions.clone();
            profiler.measure_worktree(&id, "summarize", || {
                summarize_single_worktree(&id, &info, sessions.as_ref().as_slice(), &profiler)
            })
        })
        .collect();

    summaries.sort_by(|a, b| b.last_activity_at.cmp(&a.last_activity_at));

    if let Some(start) = overall_start {
        println!(
            "[profile/worktrees] total took {:?} ({} worktrees)",
            start.elapsed(),
            summaries.len()
        );
    }

    Ok(WorktreeListResponse {
        worktrees: summaries,
    })
}

fn collect_worktree_processes(worktree_id: &str) -> Result<Option<WorktreeProcessListResponse>> {
    let state = XlaudeState::load()?;
    let Some(info) = state.worktrees.get(worktree_id) else {
        return Ok(None);
    };

    if !info.path.exists() {
        warn_once("missing_path", &info.path, || {
            format!(
                "⚠️  Worktree path missing, skipping process inspection: {}",
                info.path.display()
            )
        });
    }

    Ok(Some(WorktreeProcessListResponse {
        processes: Vec::new(),
    }))
}

impl From<agentdev::git::GitFileDiff> for WorktreeFileDiffPayload {
    fn from(value: agentdev::git::GitFileDiff) -> Self {
        Self {
            path: value.path,
            display_path: value.display_path,
            status: value.status,
            diff: value.diff,
        }
    }
}

impl From<agentdev::git::CommitDiffInfo> for WorktreeCommitDiffPayload {
    fn from(value: agentdev::git::CommitDiffInfo) -> Self {
        Self {
            reference: value.reference,
            diff: value.diff,
        }
    }
}

fn collect_worktree_summary(id: String) -> Result<Option<WorktreeSummary>> {
    let profiler = WorktreeProfiler::new();
    let overall_start = if profiler.enabled() {
        Some(Instant::now())
    } else {
        None
    };

    let state = profiler.measure_result("state.load", || XlaudeState::load())?;
    let external_sessions = collect_external_sessions(&profiler);
    let summary = state.worktrees.get(&id).map(|info| {
        profiler.measure_worktree(&id, "summarize", || {
            summarize_single_worktree(&id, info, &external_sessions, &profiler)
        })
    });

    if let Some(start) = overall_start {
        println!(
            "[profile/worktrees] total_single::{id} took {:?}",
            start.elapsed()
        );
    }

    Ok(summary)
}

fn summarize_single_worktree(
    id: &str,
    info: &WorktreeInfo,
    external_sessions: &[NormalizedSession],
    profiler: &WorktreeProfiler,
) -> WorktreeSummary {
    let path_exists = info.path.exists();
    let git_ready = path_exists && git_metadata_present(&info.path);

    if path_exists && !git_ready {
        warn_once("git_metadata", &info.path, || {
            format!(
                "⚠️  Worktree missing git metadata, skipping inspection: {}",
                info.path.display()
            )
        });
    }

    let git_status = if git_ready {
        match profiler.measure_worktree_result(id, "git_status", || {
            summarize_worktree_status(&info.path, &info.branch)
        }) {
            Ok(status) => Some(WorktreeGitStatusPayload::from(status)),
            Err(err) => {
                warn_once("git_status", &info.path, || {
                    format!(
                        "⚠️  Failed to inspect git status for {}: {err}",
                        info.path.display()
                    )
                });
                None
            }
        }
    } else if !path_exists {
        warn_once("missing_path", &info.path, || {
            format!(
                "⚠️  Worktree path missing, skipping git status: {}",
                info.path.display()
            )
        });
        None
    } else {
        None
    };

    let head_commit = if git_ready {
        match profiler.measure_worktree_result(id, "head_commit", || head_commit_info(&info.path)) {
            Ok(result) => result.map(WorktreeCommitPayload::from),
            Err(err) => {
                warn_once("head_commit", &info.path, || {
                    format!(
                        "⚠️  Failed to read last commit for {}: {err}",
                        info.path.display()
                    )
                });
                None
            }
        }
    } else {
        None
    };

    let commits_ahead = if git_ready {
        match profiler.measure_worktree_result(id, "commits_since_merge_base", || {
            commits_since_merge_base(&info.path)
        }) {
            Ok(result) => result.map(WorktreeCommitsAheadPayload::from),
            Err(err) => {
                warn_once("git_commits_ahead", &info.path, || {
                    format!(
                        "⚠️  Failed to read commits relative to base for {}: {err}",
                        info.path.display()
                    )
                });
                None
            }
        }
    } else {
        None
    };

    let mut sessions: Vec<WorktreeSessionSummary> = Vec::new();
    if path_exists {
        sessions.extend(profiler.measure_worktree(id, "sessions.external", || {
            match_sessions_for_worktree(info, external_sessions)
        }));
    }
    if path_exists {
        let claude_sessions =
            profiler.measure_worktree(id, "sessions.claude", || get_claude_sessions(&info.path));
        sessions.extend(
            claude_sessions
                .into_iter()
                .map(|session| WorktreeSessionSummary {
                    provider: "claude".to_string(),
                    session_id: session.id,
                    last_user_message: session.last_user_message,
                    last_timestamp: session.last_timestamp,
                    user_messages: session.user_messages,
                }),
        );
    }

    sessions.sort_by(|a, b| b.last_timestamp.cmp(&a.last_timestamp));

    let mut last_activity = info.created_at;
    if let Some(ref commit) = head_commit {
        if let Some(ts) = commit.timestamp {
            if ts > last_activity {
                last_activity = ts;
            }
        }
    }
    for session in &sessions {
        if let Some(ts) = session.last_timestamp {
            if ts > last_activity {
                last_activity = ts;
            }
        }
    }

    WorktreeSummary {
        id: id.to_string(),
        name: info.name.clone(),
        branch: info.branch.clone(),
        repo_name: info.repo_name.clone(),
        path: info.path.display().to_string(),
        created_at: info.created_at,
        last_activity_at: last_activity,
        task_id: info.task_id.clone(),
        task_name: info.task_name.clone(),
        initial_prompt: info.initial_prompt.clone(),
        agent_alias: info.agent_alias.clone(),
        git_status,
        head_commit,
        commits_ahead,
        sessions,
    }
}

/// GET /api/worktrees/:id/git - Detailed git diff breakdown for a worktree
pub async fn get_worktree_git_details(
    AxumPath(worktree_id): AxumPath<String>,
) -> impl IntoResponse {
    let id_for_error = worktree_id.clone();
    match tokio::task::spawn_blocking(move || collect_worktree_git_details(worktree_id)).await {
        Ok(Ok(Some(details))) => Json(details).into_response(),
        Ok(Ok(None)) => (
            StatusCode::NOT_FOUND,
            format!("Worktree {id_for_error} not found"),
        )
            .into_response(),
        Ok(Err(err)) => {
            let message = err.to_string();
            let status = if message.contains("Worktree path missing")
                || message.contains("Worktree missing git metadata")
            {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                format!("Failed to load git details for {id_for_error}: {message}"),
            )
                .into_response()
        }
        Err(join_err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Git detail task failed: {join_err}"),
        )
            .into_response(),
    }
}

fn collect_worktree_git_details(id: String) -> Result<Option<WorktreeGitDetailsPayload>> {
    let state = XlaudeState::load()?;
    let Some(info) = state.worktrees.get(&id) else {
        return Ok(None);
    };

    if !info.path.exists() {
        anyhow::bail!("Worktree path missing: {}", info.path.display());
    }
    if !git_metadata_present(&info.path) {
        anyhow::bail!("Worktree missing git metadata: {}", info.path.display());
    }

    let breakdown = collect_worktree_diff_breakdown(&info.path)?;

    let payload = WorktreeGitDetailsPayload {
        commit_diff: breakdown.commit.map(WorktreeCommitDiffPayload::from),
        staged: breakdown
            .staged
            .into_iter()
            .map(WorktreeFileDiffPayload::from)
            .collect(),
        unstaged: breakdown
            .unstaged
            .into_iter()
            .map(WorktreeFileDiffPayload::from)
            .collect(),
        untracked: breakdown
            .untracked
            .into_iter()
            .map(WorktreeFileDiffPayload::from)
            .collect(),
    };

    Ok(Some(payload))
}

/// GET /api/tasks - Get all tasks
pub async fn get_tasks(State(state): State<AppState>) -> impl IntoResponse {
    let mut tasks: Vec<Task> = {
        let tasks_guard = state.tasks.read().await;
        tasks_guard.values().cloned().collect()
    };

    for task in &mut tasks {
        task.agents.sort_by(|a, b| a.alias.cmp(&b.alias));
    }
    tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Json(tasks)
}

/// POST /api/tasks - Create a new task
pub async fn create_task(
    State(state): State<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> impl IntoResponse {
    println!("Creating task with prompt: {}", req.prompt);

    match create_task_impl(req).await {
        Ok(task) => {
            {
                let mut tasks_map = state.tasks.write().await;
                tasks_map.insert(task.id.clone(), task.clone());
            }
            Json(CreateTaskResponse { task }).into_response()
        }
        Err(e) => {
            eprintln!("Failed to create task: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create task: {}", e),
            )
                .into_response()
        }
    }
}

async fn create_task_impl(req: CreateTaskRequest) -> Result<Task> {
    let task_id = Uuid::new_v4().to_string();
    let task_name = req.name.unwrap_or_else(|| {
        generate_random_name().unwrap_or_else(|_| format!("task-{}", &task_id[..8]))
    });

    // Load agent configuration
    let agent_config = load_agent_config()?;
    let requested_agents = req
        .agents
        .unwrap_or_else(|| agent_config.agents.keys().cloned().collect::<Vec<_>>());

    if requested_agents.is_empty() {
        anyhow::bail!("No agents configured or requested");
    }

    // Get current repo info
    let repo_name = get_repo_name()?;
    let current_dir = std::env::current_dir()?;

    let mut agents = Vec::new();
    let prompt_text = req.prompt.clone();
    let prompt_ref = if prompt_text.trim().is_empty() {
        None
    } else {
        Some(prompt_text.as_str())
    };

    for agent_alias in requested_agents {
        let tmux_manager = TmuxManager::new();

        if let Some(agent_command) = agent_config.agents.get(&agent_alias) {
            let agent_id = Uuid::new_v4().to_string();

            // Create worktree for this agent
            let worktree_name = format!("{}-{}", task_name, agent_alias);
            let agent_display_name = worktree_name.clone();
            let worktree_path = current_dir
                .parent()
                .unwrap_or(&current_dir)
                .join(format!("{}-{}", repo_name, worktree_name));

            match create_worktree_for_agent(
                &task_id,
                &task_name,
                &agent_alias,
                &worktree_name,
                &worktree_path,
                prompt_ref,
            )
            .await
            {
                Ok(_) => {
                    // Create tmux session with agent command
                    let session_key = worktree_name.clone();
                    let (program, args) = split_cmdline(agent_command)?;

                    if let Err(e) = tmux_manager.create_session_with_command(
                        &session_key,
                        &worktree_path,
                        &program,
                        &args,
                    ) {
                        eprintln!(
                            "Warning: Failed to create tmux session for {}: {}",
                            agent_alias, e
                        );
                    }

                    let tmux_session_name = tmux_manager.session_name(&session_key);
                    agents.push(Agent {
                        id: agent_id.clone(),
                        name: agent_display_name.clone(),
                        alias: agent_alias.clone(),
                        status: AgentStatus::Ready,
                        worktree_path: Some(worktree_path.to_string_lossy().to_string()),
                        tmux_session: Some(tmux_session_name.clone()),
                    });

                    // Send initial prompt to the agent
                    let prompt_clone = prompt_text.clone();
                    let agent_name_clone = agent_display_name.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        if let Err(e) = tmux_manager.send_text(&session_key, &prompt_clone) {
                            eprintln!("Failed to send prompt to {}: {}", agent_name_clone, e);
                        }
                        if let Err(e) = tmux_manager.send_enter(&session_key) {
                            eprintln!("Failed to send enter to {}: {}", agent_name_clone, e);
                        }
                    });
                }
                Err(e) => {
                    agents.push(Agent {
                        id: agent_id,
                        name: agent_display_name,
                        alias: agent_alias,
                        status: AgentStatus::Error(format!("Failed to create worktree: {}", e)),
                        worktree_path: None,
                        tmux_session: None,
                    });
                }
            }
        } else {
            let agent_id = Uuid::new_v4().to_string();
            agents.push(Agent {
                id: agent_id,
                name: agent_alias.clone(),
                alias: agent_alias,
                status: AgentStatus::Error("Agent not found in configuration".to_string()),
                worktree_path: None,
                tmux_session: None,
            });
        }
    }

    Ok(Task {
        id: task_id,
        name: task_name,
        prompt: req.prompt,
        created_at: chrono::Utc::now(),
        agents,
    })
}

async fn create_worktree_for_agent(
    task_id: &str,
    task_name: &str,
    agent_alias: &str,
    worktree_name: &str,
    worktree_path: &Path,
    initial_prompt: Option<&str>,
) -> Result<()> {
    let path_str = worktree_path
        .to_str()
        .context("worktree path contains non-UTF8 characters")?
        .to_string();

    execute_git(&["worktree", "add", "-b", worktree_name, &path_str, "HEAD"])?;
    update_submodules(worktree_path)?;

    // Persist metadata
    let mut state = XlaudeState::load()?;
    let repo_name = get_repo_name()?;
    let key = XlaudeState::make_key(&repo_name, worktree_name);

    let worktree_info = agentdev::state::WorktreeInfo {
        name: worktree_name.to_string(),
        branch: worktree_name.to_string(),
        path: worktree_path.to_path_buf(),
        repo_name,
        created_at: chrono::Utc::now(),
        task_id: Some(task_id.to_string()),
        task_name: Some(task_name.to_string()),
        initial_prompt: initial_prompt
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty()),
        agent_alias: Some(agent_alias.to_string()),
    };

    state.worktrees.insert(key, worktree_info);
    state.save()?;

    Ok(())
}

/// DELETE /api/tasks/:task_id - Delete a task
pub async fn delete_task(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> impl IntoResponse {
    println!("Deleting task: {}", task_id);

    let task = {
        let mut tasks_map = state.tasks.write().await;
        tasks_map.remove(&task_id)
    };

    match task {
        Some(task) => match cleanup_task(task).await {
            Ok(_) => StatusCode::OK.into_response(),
            Err(e) => {
                eprintln!("Failed to delete task: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to delete task: {}", e),
                )
                    .into_response()
            }
        },
        None => (
            StatusCode::NOT_FOUND,
            format!("Task not found: {}", task_id),
        )
            .into_response(),
    }
}

async fn cleanup_task(task: Task) -> Result<()> {
    let tmux_manager = TmuxManager::new();
    let mut state = XlaudeState::load()?;
    let mut state_dirty = false;

    for agent in &task.agents {
        if let Some(session) = agent.tmux_session.as_deref() {
            let session_name = session.strip_prefix("agentdev_").unwrap_or(session);
            if let Err(e) = tmux_manager.kill_session(session_name) {
                eprintln!("Failed to kill tmux session {}: {}", session_name, e);
            }
        } else if tmux_manager.session_exists(&agent.name) {
            if let Err(e) = tmux_manager.kill_session(&agent.name) {
                eprintln!("Failed to kill inferred tmux session {}: {}", agent.name, e);
            }
        }

        if let Some(worktree_path) = agent.worktree_path.as_ref() {
            let path = PathBuf::from(worktree_path);
            let path_str = path.to_string_lossy().to_string();

            if let Err(e) = execute_git(&["worktree", "remove", "--force", &path_str]) {
                eprintln!("Failed to remove worktree {}: {}", path.display(), e);
            }

            let keys_by_path: Vec<String> = state
                .worktrees
                .iter()
                .filter(|(_, info)| info.path == path)
                .map(|(key, _)| key.clone())
                .collect();

            if !keys_by_path.is_empty() {
                state_dirty = true;
            }

            for key in keys_by_path {
                state.worktrees.remove(&key);
            }
        }
    }

    let keys_by_task: Vec<String> = state
        .worktrees
        .iter()
        .filter(|(_, info)| info.task_id.as_deref() == Some(&task.id))
        .map(|(key, _)| key.clone())
        .collect();

    if !keys_by_task.is_empty() {
        state_dirty = true;
    }

    for key in keys_by_task {
        state.worktrees.remove(&key);
    }

    if state_dirty {
        state.save()?;
    }

    Ok(())
}

/// GET /api/tasks/:task_id/agents/:agent_id/diff - Get agent diff
pub async fn get_agent_diff(
    State(state): State<AppState>,
    AxumPath((task_id, agent_id)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    println!("Getting diff for task {} agent {}", task_id, agent_id);

    match get_agent_diff_impl(&state, &task_id, &agent_id).await {
        Ok(diff) => diff.into_response(),
        Err(e) => {
            eprintln!("Failed to get diff: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get diff: {}", e),
            )
                .into_response()
        }
    }
}

async fn get_agent_diff_impl(state: &AppState, task_id: &str, agent_id: &str) -> Result<String> {
    let worktree_path = {
        let tasks_map = state.tasks.read().await;
        let task = tasks_map
            .get(task_id)
            .ok_or_else(|| anyhow!("Task not found: {}", task_id))?;
        let agent = task
            .agents
            .iter()
            .find(|a| a.id == agent_id)
            .ok_or_else(|| anyhow!("Agent not found: {}", agent_id))?;
        agent
            .worktree_path
            .clone()
            .ok_or_else(|| anyhow!("Agent has no worktree path"))
    }?;

    let path = PathBuf::from(&worktree_path);
    if !path.exists() {
        anyhow::bail!("Worktree path does not exist: {}", path.display());
    }

    get_diff_for_path(&path)
}

/// Rehydrate tasks from persisted agentdev state on startup.
pub fn load_tasks_from_state() -> Result<HashMap<String, Task>> {
    let mut tasks: HashMap<String, Task> = HashMap::new();
    let repo_filter = get_repo_name().ok();
    let tmux_manager = TmuxManager::new();
    let known_aliases = load_agent_config()
        .ok()
        .map(|cfg| cfg.agents.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    let state = XlaudeState::load()?;

    for info in state.worktrees.values() {
        if let Some(ref repo_name) = repo_filter {
            if &info.repo_name != repo_name {
                continue;
            }
        }

        let task_id = info.task_id.clone().unwrap_or_else(|| info.name.clone());
        let task_name = info
            .task_name
            .clone()
            .or_else(|| info.task_id.clone())
            .unwrap_or_else(|| task_id.clone());
        let prompt = sanitize_prompt(info.initial_prompt.clone());

        let alias = info
            .agent_alias
            .clone()
            .unwrap_or_else(|| infer_agent_alias(&info.name, &known_aliases));

        let session_exists = tmux_manager.session_exists(&info.name);
        let tmux_session = if session_exists {
            Some(tmux_manager.session_name(&info.name))
        } else {
            None
        };

        let worktree_path_string = info.path.to_string_lossy().to_string();
        let path_exists = info.path.exists();

        let status = if !path_exists {
            AgentStatus::Error(format!("Worktree missing at {}", worktree_path_string))
        } else if session_exists {
            AgentStatus::Running
        } else {
            AgentStatus::Ready
        };

        let agent = Agent {
            id: format!("{}:{}", task_id, info.name),
            name: info.name.clone(),
            alias: alias.clone(),
            status,
            worktree_path: Some(worktree_path_string),
            tmux_session,
        };

        let entry = tasks.entry(task_id.clone()).or_insert_with(|| Task {
            id: task_id.clone(),
            name: task_name.clone(),
            prompt: prompt.clone(),
            created_at: info.created_at,
            agents: Vec::new(),
        });

        if entry.prompt.is_empty() && !prompt.is_empty() {
            entry.prompt = prompt.clone();
        }
        if entry.name.is_empty() && !task_name.is_empty() {
            entry.name = task_name.clone();
        }
        if info.created_at < entry.created_at {
            entry.created_at = info.created_at;
        }

        if !entry
            .agents
            .iter()
            .any(|existing| existing.name == agent.name)
        {
            entry.agents.push(agent);
        }
    }

    Ok(tasks)
}

fn infer_agent_alias(worktree_name: &str, known_aliases: &[String]) -> String {
    for alias in known_aliases {
        if worktree_name == alias {
            return alias.clone();
        }
        if let Some(prefix) = worktree_name.strip_suffix(alias) {
            if prefix.ends_with('-') {
                return alias.clone();
            }
        }
    }

    worktree_name
        .rsplit_once('-')
        .map(|(_, suffix)| suffix.to_string())
        .filter(|suffix| !suffix.is_empty())
        .unwrap_or_else(|| worktree_name.to_string())
}

fn sanitize_prompt(prompt: Option<String>) -> String {
    prompt
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .unwrap_or_default()
}
