use anyhow::{Result, anyhow};
use axum::{
    Json,
    extract::{Path as AxumPath, Query},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Instant;

use crate::{
    discovery::{DiscoveryOptions, discover_worktrees as discover_unmanaged_worktrees},
    git::{
        CommitsAhead, HeadCommitInfo, WorktreeGitStatus, collect_worktree_diff_breakdown,
        commits_since_merge_base, head_commit_info, summarize_worktree_status,
    },
    process_registry::{
        MAX_PROCESSES_PER_WORKTREE, ProcessRecord, ProcessRegistry,
        ProcessStatus as RegistryProcessStatus, canonicalize_cwd,
    },
    sessions::{
        SessionEvent, SessionProvider, SessionRecord, canonicalize as canonicalize_session_path,
        default_providers,
    },
    state::{WorktreeInfo, XlaudeState},
};
use rayon::prelude::*;

#[derive(Serialize, Deserialize, Clone)]
pub struct WorktreeSessionSummary {
    pub provider: String,
    pub session_id: String,
    pub last_user_message: String,
    pub last_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub user_message_count: usize,
    pub user_messages_preview: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SessionSummaryPayload {
    pub provider: String,
    pub session_id: String,
    pub last_user_message: String,
    pub last_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub user_message_count: usize,
    pub user_messages_preview: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
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
    #[serde(default)]
    pub stdout: Option<String>,
    #[serde(default)]
    pub stderr: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WorktreeProcessListResponse {
    pub processes: Vec<WorktreeProcessSummary>,
}

#[derive(Deserialize)]
pub struct WorktreeDiscoveryQuery {
    #[serde(default)]
    pub recursive: Option<bool>,
    #[serde(default)]
    pub root: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct LaunchWorktreeCommandRequest {
    pub command: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct LaunchWorktreeCommandResponse {
    pub process: WorktreeProcessSummary,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct LaunchWorktreeShellRequest {
    #[serde(default)]
    pub command: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct LaunchWorktreeShellResponse {
    pub status: &'static str,
}

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MergeStrategyOption {
    FfOnly,
    Merge,
    Squash,
}

impl Default for MergeStrategyOption {
    fn default() -> Self {
        MergeStrategyOption::FfOnly
    }
}

impl MergeStrategyOption {
    fn as_cli_flag(&self) -> &'static str {
        match self {
            MergeStrategyOption::FfOnly => "ff-only",
            MergeStrategyOption::Merge => "merge",
            MergeStrategyOption::Squash => "squash",
        }
    }
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct MergeWorktreeRequest {
    #[serde(default)]
    pub strategy: Option<MergeStrategyOption>,
    #[serde(default)]
    pub push: bool,
    #[serde(default)]
    pub cleanup: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct MergeWorktreeResponse {
    pub exit_code: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct DeleteWorktreeRequest {
    #[serde(default)]
    pub force: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct DeleteWorktreeResponse {
    pub exit_code: i32,
    pub removed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
struct CommandFailurePayload {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

impl CommandFailurePayload {
    fn simple<S: Into<String>>(message: S) -> Self {
        Self {
            message: message.into(),
            stdout: None,
            stderr: None,
            exit_code: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WorktreeListResponse {
    pub worktrees: Vec<WorktreeSummary>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SessionListResponse {
    pub sessions: Vec<SessionSummaryPayload>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<ProviderSessionsPayload>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ProviderSessionsPayload {
    pub provider: String,
    pub session_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub session_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionDetailMode {
    UserOnly,
    Conversation,
    Full,
}

impl Default for SessionDetailMode {
    fn default() -> Self {
        SessionDetailMode::Full
    }
}

#[derive(Deserialize, Default)]
pub struct SessionDetailQuery {
    #[serde(default)]
    pub mode: Option<SessionDetailMode>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SessionDetailPayload {
    pub provider: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    pub mode: SessionDetailMode,
    pub events: Vec<SessionEvent>,
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

impl From<RegistryProcessStatus> for WorktreeProcessStatus {
    fn from(value: RegistryProcessStatus) -> Self {
        match value {
            RegistryProcessStatus::Pending => WorktreeProcessStatus::Pending,
            RegistryProcessStatus::Running => WorktreeProcessStatus::Running,
            RegistryProcessStatus::Succeeded => WorktreeProcessStatus::Succeeded,
            RegistryProcessStatus::Failed => WorktreeProcessStatus::Failed,
            RegistryProcessStatus::Unknown => WorktreeProcessStatus::Unknown,
        }
    }
}

const SESSION_PREVIEW_MAX_MESSAGES: usize = 12;
const SESSION_PREVIEW_HEAD_MESSAGES: usize = 3;
const SESSION_PREVIEW_MAX_CHARS: usize = 512;

fn truncate_preview_message(message: &str, max_chars: usize) -> String {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut preview = String::new();
    let mut chars = trimmed.chars();
    for _ in 0..max_chars {
        match chars.next() {
            Some(ch) => preview.push(ch),
            None => return preview,
        }
    }

    if chars.next().is_some() {
        preview.push('…');
    }

    preview
}

fn build_user_message_preview(messages: &[String]) -> (Vec<String>, usize) {
    let total = messages.len();
    if total == 0 {
        return (Vec::new(), 0);
    }

    if total <= SESSION_PREVIEW_MAX_MESSAGES {
        let preview = messages
            .iter()
            .map(|message| truncate_preview_message(message, SESSION_PREVIEW_MAX_CHARS))
            .collect();
        return (preview, total);
    }

    let head_count = SESSION_PREVIEW_HEAD_MESSAGES.min(total);
    let mut preview: Vec<String> = messages
        .iter()
        .take(head_count)
        .map(|message| truncate_preview_message(message, SESSION_PREVIEW_MAX_CHARS))
        .collect();

    let tail_capacity = SESSION_PREVIEW_MAX_MESSAGES.saturating_sub(preview.len());
    if tail_capacity > 0 {
        let mut tail_start = total.saturating_sub(tail_capacity);
        if tail_start < head_count {
            tail_start = head_count;
        }

        preview.extend(
            messages
                .iter()
                .skip(tail_start)
                .map(|message| truncate_preview_message(message, SESSION_PREVIEW_MAX_CHARS)),
        );
    }

    (preview, total)
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

fn load_session_detail(
    provider_name: String,
    session_id: String,
    mode: SessionDetailMode,
) -> Result<Option<SessionDetailPayload>> {
    let requested = provider_name.to_lowercase();
    for provider in default_providers() {
        if !provider.name().eq_ignore_ascii_case(&requested) {
            continue;
        }

        let records = provider.list_sessions()?;
        for record in records {
            if record.id == session_id {
                let events = match mode {
                    SessionDetailMode::Full => provider.load_session_events(&record)?,
                    SessionDetailMode::UserOnly => user_messages_to_events(&record),
                    SessionDetailMode::Conversation => conversation_events(&record, &provider)?,
                };

                let working_dir = record
                    .working_dir
                    .as_ref()
                    .map(|dir| dir.display().to_string());

                return Ok(Some(SessionDetailPayload {
                    provider: provider.name().to_string(),
                    session_id: record.id.clone(),
                    last_timestamp: record.last_timestamp,
                    working_dir,
                    mode,
                    events,
                }));
            }
        }

        return Ok(None);
    }
    Ok(None)
}

fn user_messages_to_events(record: &SessionRecord) -> Vec<SessionEvent> {
    record
        .user_messages
        .iter()
        .enumerate()
        .map(|(_, text)| SessionEvent {
            actor: Some("user".to_string()),
            category: "user".to_string(),
            label: Some("User".to_string()),
            text: Some(text.clone()),
            summary_text: Some(text.clone()),
            data: None,
            timestamp: None,
            raw: None,
            tool: None,
        })
        .collect()
}

fn conversation_events(
    record: &SessionRecord,
    provider: &Box<dyn SessionProvider + Send + Sync>,
) -> Result<Vec<SessionEvent>> {
    let all_events = provider.load_session_events(record)?;
    Ok(all_events
        .into_iter()
        .filter(|event| {
            // Filter out tool-related events
            if event.tool.is_some() {
                return false;
            }
            let category_lower = event.category.to_lowercase();
            if category_lower.contains("tool") {
                return false;
            }

            // Only keep user and assistant messages
            if let Some(actor) = &event.actor {
                let actor_lower = actor.to_lowercase();
                actor_lower == "user" || actor_lower == "assistant"
            } else {
                false
            }
        })
        .collect())
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
            let (user_messages_preview, user_message_count) =
                build_user_message_preview(&session.record.user_messages);
            summaries.push(WorktreeSessionSummary {
                provider: session.record.provider.clone(),
                session_id: session.record.id.clone(),
                last_user_message: session.record.last_user_message.clone().unwrap_or_default(),
                last_timestamp: session.record.last_timestamp,
                user_message_count,
                user_messages_preview,
            });
        }
    }
    summaries
}

/// GET /api/sessions - List all known sessions across providers
pub async fn get_sessions() -> impl IntoResponse {
    match tokio::task::spawn_blocking(collect_all_sessions).await {
        Ok(Ok(response)) => Json(response).into_response(),
        Ok(Err(err)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to collect sessions: {err}"),
        )
            .into_response(),
        Err(join_err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Session collection task failed: {join_err}"),
        )
            .into_response(),
    }
}

/// GET /api/sessions/:provider/:session_id - Fetch transcript details for a session
pub async fn get_session_detail(
    AxumPath((provider, session_id)): AxumPath<(String, String)>,
    Query(query): Query<SessionDetailQuery>,
) -> impl IntoResponse {
    let mode = query.mode.unwrap_or_default();
    match tokio::task::spawn_blocking(move || load_session_detail(provider, session_id, mode)).await
    {
        Ok(Ok(Some(detail))) => Json(detail).into_response(),
        Ok(Ok(None)) => (StatusCode::NOT_FOUND, "Session not found".to_string()).into_response(),
        Ok(Err(err)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load session detail: {err}"),
        )
            .into_response(),
        Err(join_err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Session detail task failed: {join_err}"),
        )
            .into_response(),
    }
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

/// GET /api/worktrees/discovery - List unmanaged git worktrees
pub async fn get_worktree_discovery(
    Query(query): Query<WorktreeDiscoveryQuery>,
) -> impl IntoResponse {
    let recursive = query.recursive.unwrap_or(true);
    let root = if let Some(raw) = query.root {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            let path = PathBuf::from(trimmed);
            if !path.exists() {
                return (
                    StatusCode::BAD_REQUEST,
                    format!("Discovery root does not exist: {}", path.display()),
                )
                    .into_response();
            }
            if !path.is_dir() {
                return (
                    StatusCode::BAD_REQUEST,
                    format!("Discovery root is not a directory: {}", path.display()),
                )
                    .into_response();
            }
            Some(path)
        }
    } else {
        None
    };

    match tokio::task::spawn_blocking(move || {
        discover_unmanaged_worktrees(DiscoveryOptions { recursive, root })
    })
    .await
    {
        Ok(Ok(discovered)) => Json(discovered).into_response(),
        Ok(Err(err)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to discover git worktrees: {err}"),
        )
            .into_response(),
        Err(join_err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Worktree discovery task failed: {join_err}"),
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

pub async fn post_worktree_command(
    AxumPath(worktree_id): AxumPath<String>,
    Json(payload): Json<LaunchWorktreeCommandRequest>,
) -> impl IntoResponse {
    let id_for_error = worktree_id.clone();
    match tokio::task::spawn_blocking(move || launch_worktree_command(worktree_id, payload)).await {
        Ok(Ok(LaunchCommandResult::Success(process))) => (
            StatusCode::CREATED,
            Json(LaunchWorktreeCommandResponse { process }),
        )
            .into_response(),
        Ok(Ok(LaunchCommandResult::NotFound)) => (
            StatusCode::NOT_FOUND,
            format!("Worktree {id_for_error} not found"),
        )
            .into_response(),
        Ok(Ok(LaunchCommandResult::Invalid(message))) => {
            (StatusCode::BAD_REQUEST, message).into_response()
        }
        Ok(Err(err)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to launch command: {err}"),
        )
            .into_response(),
        Err(join_err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Command launch task failed: {join_err}"),
        )
            .into_response(),
    }
}

pub async fn post_worktree_shell(
    AxumPath(worktree_id): AxumPath<String>,
    Json(payload): Json<LaunchWorktreeShellRequest>,
) -> impl IntoResponse {
    let id_for_error = worktree_id.clone();
    match tokio::task::spawn_blocking(move || launch_worktree_shell(worktree_id, payload)).await {
        Ok(Ok(LaunchShellResult::Success)) => (
            StatusCode::ACCEPTED,
            Json(LaunchWorktreeShellResponse { status: "launched" }),
        )
            .into_response(),
        Ok(Ok(LaunchShellResult::NotFound)) => (
            StatusCode::NOT_FOUND,
            format!("Worktree {id_for_error} not found"),
        )
            .into_response(),
        Ok(Ok(LaunchShellResult::Invalid(message))) => (
            StatusCode::BAD_REQUEST,
            Json(CommandFailurePayload::simple(message)),
        )
            .into_response(),
        Ok(Err(err)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to launch shell: {err}"),
        )
            .into_response(),
        Err(join_err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Shell launch task failed: {join_err}"),
        )
            .into_response(),
    }
}

pub async fn post_worktree_merge(
    AxumPath(worktree_id): AxumPath<String>,
    Json(payload): Json<MergeWorktreeRequest>,
) -> impl IntoResponse {
    let id_for_error = worktree_id.clone();
    match tokio::task::spawn_blocking(move || merge_worktree_via_cli(worktree_id, payload)).await {
        Ok(Ok(response)) => Json(response).into_response(),
        Ok(Err(WorktreeActionError::NotFound)) => (
            StatusCode::NOT_FOUND,
            format!("Worktree {id_for_error} not found"),
        )
            .into_response(),
        Ok(Err(WorktreeActionError::CommandFailure(payload))) => {
            (StatusCode::CONFLICT, Json(payload)).into_response()
        }
        Ok(Err(WorktreeActionError::Internal(err))) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to merge worktree: {err}"),
        )
            .into_response(),
        Err(join_err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Merge task failed: {join_err}"),
        )
            .into_response(),
    }
}

pub async fn post_worktree_delete(
    AxumPath(worktree_id): AxumPath<String>,
    Json(payload): Json<DeleteWorktreeRequest>,
) -> impl IntoResponse {
    let id_for_error = worktree_id.clone();
    match tokio::task::spawn_blocking(move || delete_worktree_via_cli(worktree_id, payload)).await {
        Ok(Ok(response)) => Json(response).into_response(),
        Ok(Err(WorktreeActionError::NotFound)) => (
            StatusCode::NOT_FOUND,
            format!("Worktree {id_for_error} not found"),
        )
            .into_response(),
        Ok(Err(WorktreeActionError::CommandFailure(payload))) => {
            (StatusCode::CONFLICT, Json(payload)).into_response()
        }
        Ok(Err(WorktreeActionError::Internal(err))) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to delete worktree: {err}"),
        )
            .into_response(),
        Err(join_err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Delete task failed: {join_err}"),
        )
            .into_response(),
    }
}

enum WorktreeActionError {
    NotFound,
    CommandFailure(CommandFailurePayload),
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for WorktreeActionError {
    fn from(value: anyhow::Error) -> Self {
        WorktreeActionError::Internal(value)
    }
}

fn merge_worktree_via_cli(
    worktree_id: String,
    payload: MergeWorktreeRequest,
) -> Result<MergeWorktreeResponse, WorktreeActionError> {
    let state = XlaudeState::load().map_err(WorktreeActionError::from)?;
    let info = state
        .worktrees
        .get(&worktree_id)
        .cloned()
        .ok_or(WorktreeActionError::NotFound)?;

    let strategy = payload.strategy.unwrap_or_default();

    let mut args = vec![
        "worktree".to_string(),
        "merge".to_string(),
        info.name.clone(),
    ];

    if strategy != MergeStrategyOption::FfOnly {
        args.push("--strategy".to_string());
        args.push(strategy.as_cli_flag().to_string());
    }
    if payload.push {
        args.push("--push".to_string());
    }
    if payload.cleanup {
        args.push("--cleanup".to_string());
    }

    let output = run_agentdev_cli(args, Vec::new()).map_err(WorktreeActionError::from)?;
    if !output.success {
        let failure = build_command_failure("Worktree merge failed", &output);
        return Err(WorktreeActionError::CommandFailure(failure));
    }

    Ok(MergeWorktreeResponse {
        exit_code: output.normalized_exit_code(),
        stdout: output.stdout_trimmed(),
        stderr: output.stderr_trimmed(),
    })
}

fn delete_worktree_via_cli(
    worktree_id: String,
    payload: DeleteWorktreeRequest,
) -> Result<DeleteWorktreeResponse, WorktreeActionError> {
    let state = XlaudeState::load().map_err(WorktreeActionError::from)?;
    let info = state
        .worktrees
        .get(&worktree_id)
        .cloned()
        .ok_or(WorktreeActionError::NotFound)?;

    let args = vec![
        "worktree".to_string(),
        "delete".to_string(),
        info.name.clone(),
    ];

    let mut extra_env: Vec<(String, String)> = Vec::new();
    if payload.force {
        extra_env.push(("XLAUDE_YES".to_string(), "1".to_string()));
    }

    let output = run_agentdev_cli(args, extra_env).map_err(WorktreeActionError::from)?;

    let removed = {
        let refreshed = XlaudeState::load().map_err(WorktreeActionError::from)?;
        !refreshed.worktrees.contains_key(&worktree_id)
    };

    if !output.success || !removed {
        let mut failure = build_command_failure("Worktree deletion failed", &output);
        if !removed {
            let fallback = if payload.force {
                "Worktree deletion did not complete"
            } else {
                "Worktree deletion was cancelled; resolve pending work or retry with force"
            };

            if failure.message.trim().is_empty() || failure.message == "Worktree deletion failed" {
                failure.message = fallback.to_string();
            }
        }
        return Err(WorktreeActionError::CommandFailure(failure));
    }

    Ok(DeleteWorktreeResponse {
        exit_code: output.normalized_exit_code(),
        removed,
        stdout: output.stdout_trimmed(),
        stderr: output.stderr_trimmed(),
    })
}

struct CliCommandOutput {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    success: bool,
}

impl CliCommandOutput {
    fn normalized_exit_code(&self) -> i32 {
        self.exit_code.unwrap_or(if self.success { 0 } else { -1 })
    }

    fn stdout_trimmed(&self) -> Option<String> {
        trimmed_or_none(&self.stdout)
    }

    fn stderr_trimmed(&self) -> Option<String> {
        trimmed_or_none(&self.stderr)
    }
}

fn run_agentdev_cli(
    args: Vec<String>,
    extra_env: Vec<(String, String)>,
) -> Result<CliCommandOutput> {
    let exe = resolve_agentdev_cli_executable()?;
    let mut command = Command::new(&exe);
    command.args(&args);
    command.env("XLAUDE_NON_INTERACTIVE", "1");
    command.env("NO_COLOR", "1");
    command.env("CLICOLOR_FORCE", "0");
    for (key, value) in extra_env {
        command.env(key, value);
    }
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let output = command
        .output()
        .map_err(|err| anyhow!("Failed to run agentdev command {:?}: {err}", args))?;

    Ok(CliCommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code(),
        success: output.status.success(),
    })
}

fn trimmed_or_none(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn build_command_failure(context: &str, output: &CliCommandOutput) -> CommandFailurePayload {
    let stderr = output.stderr_trimmed();
    let stdout = output.stdout_trimmed();
    let message = stderr
        .clone()
        .or_else(|| stdout.clone())
        .unwrap_or_else(|| context.to_string());

    CommandFailurePayload {
        message,
        stdout,
        stderr,
        exit_code: output.exit_code,
    }
}

fn resolve_agentdev_cli_executable() -> Result<PathBuf> {
    if let Some(path) = cli_override_from_env()? {
        return Ok(path);
    }

    let current = std::env::current_exe()?;

    if is_cli_binary(&current) {
        return Ok(current);
    }

    if let Some(path) = find_cli_nearby(&current) {
        return Ok(path);
    }

    if let Some(path) = search_workspace_targets(&current) {
        return Ok(path);
    }

    for candidate in CLI_BINARY_NAMES {
        if let Ok(path) = which::which(candidate) {
            return Ok(path);
        }
    }

    Err(anyhow!(
        "Unable to locate agentdev CLI binary. Set AGENTDEV_CLI_BIN to override."
    ))
}

fn cli_override_from_env() -> Result<Option<PathBuf>> {
    match std::env::var_os("AGENTDEV_CLI_BIN") {
        None => Ok(None),
        Some(value) => {
            let path = PathBuf::from(value);
            if path.is_file() {
                Ok(Some(path))
            } else {
                Err(anyhow!(
                    "AGENTDEV_CLI_BIN points to {:?}, but no executable was found there",
                    path
                ))
            }
        }
    }
}

const CLI_BINARY_NAMES: &[&str] = &["agentdev", "xlaude"];

fn is_cli_binary(path: &Path) -> bool {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| CLI_BINARY_NAMES.iter().any(|name| stem == *name))
        .unwrap_or(false)
}

fn find_cli_nearby(current: &Path) -> Option<PathBuf> {
    let dir = current.parent()?;
    for name in CLI_BINARY_NAMES {
        for candidate in candidate_paths(dir, name) {
            if candidate.is_file() && candidate != current {
                return Some(candidate);
            }
        }
    }
    None
}

fn search_workspace_targets(current: &Path) -> Option<PathBuf> {
    let mut profiles = Vec::new();
    if let Some(profile) = current
        .parent()
        .and_then(|dir| dir.file_name())
        .and_then(|value| value.to_str())
    {
        profiles.push(profile.to_string());
    }
    if !profiles.iter().any(|p| p == "debug") {
        profiles.push("debug".to_string());
    }
    if !profiles.iter().any(|p| p == "release") {
        profiles.push("release".to_string());
    }

    let mut dir_opt = current.parent();
    while let Some(dir) = dir_opt {
        let target_root = dir.join("target");
        for profile in &profiles {
            let profile_dir = target_root.join(profile);
            if let Some(path) = find_cli_in_dir(&profile_dir) {
                return Some(path);
            }
        }
        dir_opt = dir.parent();
    }
    None
}

fn find_cli_in_dir(dir: &Path) -> Option<PathBuf> {
    for name in CLI_BINARY_NAMES {
        for candidate in candidate_paths(dir, name) {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(windows)]
fn candidate_paths(dir: &Path, name: &str) -> Vec<PathBuf> {
    let mut paths = vec![dir.join(name)];
    if !name.to_ascii_lowercase().ends_with(".exe") {
        paths.push(dir.join(format!("{name}.exe")));
    }
    paths
}

#[cfg(not(windows))]
fn candidate_paths(dir: &Path, name: &str) -> Vec<PathBuf> {
    vec![dir.join(name)]
}

fn collect_all_sessions() -> Result<SessionListResponse> {
    let profiler = WorktreeProfiler::new();
    let overall_start = if profiler.enabled() {
        Some(Instant::now())
    } else {
        None
    };

    let state = profiler.measure_result("state.load", || XlaudeState::load())?;

    let worktree_entries: Vec<(String, WorktreeInfo)> = state
        .worktrees
        .iter()
        .map(|(id, info)| (id.clone(), info.clone()))
        .collect();

    struct WorktreeLocator {
        id: String,
        info: WorktreeInfo,
        base_path: PathBuf,
    }

    let locators: Vec<WorktreeLocator> = worktree_entries
        .into_iter()
        .map(|(id, info)| {
            let canonical = canonicalize_session_path(&info.path);
            let base_path = canonical.unwrap_or_else(|| info.path.clone());
            WorktreeLocator {
                id,
                info,
                base_path,
            }
        })
        .collect();

    let external_sessions = collect_external_sessions(&profiler);

    let mut sessions: Vec<SessionSummaryPayload> = external_sessions
        .iter()
        .map(|normalized| {
            let working_dir_path = normalized
                .canonical_dir
                .as_ref()
                .map(|path| path.as_path())
                .or_else(|| {
                    normalized
                        .record
                        .working_dir
                        .as_ref()
                        .map(|path| path.as_path())
                });

            let matched = working_dir_path.and_then(|dir| {
                locators
                    .iter()
                    .find(|locator| dir.starts_with(locator.base_path.as_path()))
            });

            let (worktree_id, worktree_name, repo_name, branch) = matched
                .map(|locator| {
                    (
                        Some(locator.id.clone()),
                        Some(locator.info.name.clone()),
                        Some(locator.info.repo_name.clone()),
                        Some(locator.info.branch.clone()),
                    )
                })
                .unwrap_or((None, None, None, None));

            let working_dir = working_dir_path
                .map(|path| path.display().to_string())
                .or_else(|| {
                    normalized
                        .record
                        .working_dir
                        .as_ref()
                        .map(|path| path.display().to_string())
                });

            let (user_messages_preview, user_message_count) =
                build_user_message_preview(&normalized.record.user_messages);

            SessionSummaryPayload {
                provider: normalized.record.provider.clone(),
                session_id: normalized.record.id.clone(),
                last_user_message: normalized
                    .record
                    .last_user_message
                    .clone()
                    .unwrap_or_default(),
                last_timestamp: normalized.record.last_timestamp,
                user_message_count,
                user_messages_preview,
                worktree_id,
                worktree_name,
                repo_name,
                branch,
                working_dir,
            }
        })
        .collect();

    sessions.sort_by(|a, b| {
        b.last_timestamp
            .cmp(&a.last_timestamp)
            .then_with(|| a.provider.cmp(&b.provider))
            .then_with(|| a.session_id.cmp(&b.session_id))
    });

    let mut provider_map: std::collections::BTreeMap<String, ProviderSessionsPayload> =
        std::collections::BTreeMap::new();
    for session in &sessions {
        let entry = provider_map
            .entry(session.provider.clone())
            .or_insert_with(|| ProviderSessionsPayload {
                provider: session.provider.clone(),
                session_count: 0,
                session_ids: Vec::new(),
                latest_timestamp: None,
            });
        entry.session_count += 1;
        entry.session_ids.push(session.session_id.clone());
        if let Some(candidate) = session.last_timestamp {
            if entry
                .latest_timestamp
                .map_or(true, |current| candidate > current)
            {
                entry.latest_timestamp = Some(candidate);
            }
        }
    }

    let mut providers: Vec<ProviderSessionsPayload> = provider_map.into_values().collect();
    providers.sort_by(|a, b| {
        b.session_count
            .cmp(&a.session_count)
            .then_with(|| a.provider.cmp(&b.provider))
    });

    if let Some(start) = overall_start {
        println!(
            "[profile/sessions] total took {:?} ({} sessions)",
            start.elapsed(),
            sessions.len()
        );
    }

    Ok(SessionListResponse {
        sessions,
        providers,
    })
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

    let registry = ProcessRegistry::load()?;
    let processes = registry
        .processes_for_worktree(worktree_id)
        .into_iter()
        .map(process_record_to_summary)
        .collect();

    Ok(Some(WorktreeProcessListResponse { processes }))
}

enum LaunchCommandResult {
    Success(WorktreeProcessSummary),
    NotFound,
    Invalid(String),
}

enum LaunchShellResult {
    Success,
    NotFound,
    Invalid(String),
}

fn launch_worktree_shell(
    worktree_id: String,
    request: LaunchWorktreeShellRequest,
) -> Result<LaunchShellResult> {
    if let Some(raw) = request.command.as_ref() {
        if raw.trim().is_empty() {
            return Ok(LaunchShellResult::Invalid(
                "Command is required when provided".to_string(),
            ));
        }
    }

    let state = XlaudeState::load()?;
    let Some(info) = state.worktrees.get(&worktree_id) else {
        return Ok(LaunchShellResult::NotFound);
    };

    let command_line = build_terminal_launch_command(info, request.command.as_deref())?;
    let (program, args) = command_line
        .split_first()
        .ok_or_else(|| anyhow!("Terminal command unexpectedly empty"))?;

    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| anyhow!("Failed to launch terminal via '{program}': {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr_trimmed = stderr.trim();
        let stdout_trimmed = stdout.trim();
        let message = if !stderr_trimmed.is_empty() {
            stderr_trimmed.to_string()
        } else if !stdout_trimmed.is_empty() {
            stdout_trimmed.to_string()
        } else {
            format!(
                "Terminal command exited with status {}",
                output.status.code().unwrap_or(-1)
            )
        };
        return Err(anyhow!(message));
    }

    Ok(LaunchShellResult::Success)
}

fn launch_worktree_command(
    worktree_id: String,
    request: LaunchWorktreeCommandRequest,
) -> Result<LaunchCommandResult> {
    let trimmed = request.command.trim();
    if trimmed.is_empty() {
        return Ok(LaunchCommandResult::Invalid(
            "Command is required".to_string(),
        ));
    }

    let command_tokens = match shell_words::split(trimmed) {
        Ok(tokens) if !tokens.is_empty() => tokens,
        Ok(_) => {
            return Ok(LaunchCommandResult::Invalid(
                "Command is required".to_string(),
            ));
        }
        Err(err) => {
            return Ok(LaunchCommandResult::Invalid(format!(
                "Invalid command string: {err}"
            )));
        }
    };

    let state = XlaudeState::load()?;
    let Some(info) = state.worktrees.get(&worktree_id) else {
        return Ok(LaunchCommandResult::NotFound);
    };

    let mut record = ProcessRecord::new(
        worktree_id.clone(),
        info.name.clone(),
        info.repo_name.clone(),
        command_tokens.clone(),
        Some(info.path.clone()),
        RegistryProcessStatus::Pending,
    );

    let default_description = "Launched from AgentDev dashboard".to_string();
    record.description = request
        .description
        .and_then(|value| {
            let trimmed_desc = value.trim();
            if trimmed_desc.is_empty() {
                None
            } else {
                Some(trimmed_desc.to_string())
            }
        })
        .or(Some(default_description));

    let process_id = record.id.clone();
    let record_to_store = record.clone();
    ProcessRegistry::mutate(move |registry| {
        registry.insert(record_to_store);
        registry.retain_recent(MAX_PROCESSES_PER_WORKTREE);
        Ok(())
    })?;

    let worktree_path = info.path.clone();
    spawn_command_runner(
        worktree_id,
        process_id.clone(),
        command_tokens,
        worktree_path,
    );

    Ok(LaunchCommandResult::Success(process_record_to_summary(
        &record,
    )))
}

fn build_terminal_launch_command(
    worktree: &WorktreeInfo,
    command: Option<&str>,
) -> Result<Vec<String>> {
    if let Ok(template) = std::env::var("AGENTDEV_TERMINAL_COMMAND") {
        if !template.trim().is_empty() {
            return build_terminal_command_from_template(worktree, command, &template);
        }
    }
    build_default_terminal_command(worktree, command)
}

fn build_terminal_command_from_template(
    worktree: &WorktreeInfo,
    command: Option<&str>,
    template: &str,
) -> Result<Vec<String>> {
    let cwd = worktree.path.display().to_string();
    let command_value = command.unwrap_or("").to_string();
    let command_or_shell = command
        .map(|value| value.to_string())
        .unwrap_or_else(|| r#"exec "$SHELL" -l"#.to_string());
    let filled = template
        .replace("{cwd}", &cwd)
        .replace("{command_or_shell}", &command_or_shell)
        .replace("{command}", &command_value);
    let tokens = shell_words::split(&filled)
        .map_err(|err| anyhow!("Failed to parse AGENTDEV_TERMINAL_COMMAND: {err}"))?;
    if tokens.is_empty() {
        return Err(anyhow!(
            "AGENTDEV_TERMINAL_COMMAND did not produce a runnable command"
        ));
    }
    Ok(tokens)
}

fn build_default_terminal_command(
    worktree: &WorktreeInfo,
    command: Option<&str>,
) -> Result<Vec<String>> {
    #[cfg(target_os = "macos")]
    {
        build_macos_terminal_command(worktree, command)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let suggestion = "Set AGENTDEV_TERMINAL_COMMAND to customize terminal launching, e.g. `wezterm start --cwd '{cwd}' -- bash -lc '{command}'`.";
        Err(anyhow!(
            "Terminal launch is not implemented for this platform. {suggestion}"
        ))
    }
}

#[cfg(target_os = "macos")]
fn build_macos_terminal_command(
    worktree: &WorktreeInfo,
    command: Option<&str>,
) -> Result<Vec<String>> {
    let path_str = worktree
        .path
        .to_str()
        .ok_or_else(|| anyhow!("Worktree path contains invalid UTF-8"))?;
    let base = format!("cd {} &&", shell_quote(path_str));
    let shell_command = match command {
        Some(cmd) => format!(
            "{base} {cmd}; exec \"$SHELL\" -l",
            base = base,
            cmd = cmd.trim()
        ),
        None => format!("{base} exec \"$SHELL\" -l"),
    };
    let escaped = escape_applescript_string(&shell_command);
    let activate = "tell application \"Terminal\" to activate".to_string();
    let run_command = format!(
        "tell application \"Terminal\" to do script \"{escaped}\"",
        escaped = escaped
    );
    Ok(vec![
        "osascript".to_string(),
        "-e".to_string(),
        activate,
        "-e".to_string(),
        run_command,
    ])
}

#[cfg(target_os = "macos")]
fn escape_applescript_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(target_os = "macos")]
fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    if !value.contains('\'') {
        return format!("'{value}'");
    }
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

fn spawn_command_runner(
    worktree_id: String,
    process_id: String,
    command_tokens: Vec<String>,
    worktree_path: PathBuf,
) {
    thread::spawn(move || {
        if let Err(err) =
            run_command_runner(&worktree_id, &process_id, &command_tokens, &worktree_path)
        {
            eprintln!(
                "Failed to execute command for worktree {}: {err}",
                worktree_id
            );
        }
    });
}

fn run_command_runner(
    _worktree_id: &str,
    process_id: &str,
    command_tokens: &[String],
    worktree_path: &Path,
) -> Result<()> {
    let (program, args) = command_tokens
        .split_first()
        .ok_or_else(|| anyhow!("Command tokens unexpectedly empty"))?;

    ProcessRegistry::mutate(|registry| {
        registry.update(process_id, |record| {
            record.mark_running();
            record.cwd = Some(canonicalize_cwd(worktree_path));
            record.error = None;
        })?;
        registry.retain_recent(MAX_PROCESSES_PER_WORKTREE);
        Ok(())
    })?;

    let status = Command::new(program)
        .args(args)
        .current_dir(worktree_path)
        .output();

    match status {
        Ok(output) => {
            let stdout_text = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr_text = String::from_utf8_lossy(&output.stderr).to_string();

            if !stdout_text.is_empty() {
                print!("{stdout_text}");
            }
            if !stderr_text.is_empty() {
                eprint!("{stderr_text}");
            }

            let stdout_option = if stdout_text.is_empty() {
                None
            } else {
                Some(stdout_text)
            };
            let stderr_option = if stderr_text.is_empty() {
                None
            } else {
                Some(stderr_text)
            };

            let outcome = if output.status.success() {
                RegistryProcessStatus::Succeeded
            } else {
                RegistryProcessStatus::Failed
            };
            ProcessRegistry::mutate(|registry| {
                registry.update(process_id, |record| {
                    record.mark_finished(
                        outcome,
                        output.status.code(),
                        None,
                        stdout_option.clone(),
                        stderr_option.clone(),
                    );
                })?;
                registry.retain_recent(MAX_PROCESSES_PER_WORKTREE);
                Ok(())
            })?;

            if !output.status.success() {
                if let Some(code) = output.status.code() {
                    return Err(anyhow!("Command exited with status {code}"));
                }
                return Err(anyhow!("Command terminated by signal"));
            }
        }
        Err(err) => {
            let error_message = format!("Failed to spawn '{program}': {err}");
            ProcessRegistry::mutate(|registry| {
                registry.update(process_id, |record| {
                    record.mark_finished(
                        RegistryProcessStatus::Failed,
                        None,
                        Some(error_message.clone()),
                        None,
                        None,
                    );
                })?;
                registry.retain_recent(MAX_PROCESSES_PER_WORKTREE);
                Ok(())
            })?;
            return Err(anyhow!(error_message));
        }
    }

    Ok(())
}

fn process_record_to_summary(record: &ProcessRecord) -> WorktreeProcessSummary {
    WorktreeProcessSummary {
        id: record.id.clone(),
        command: record.command.clone(),
        status: WorktreeProcessStatus::from(record.status),
        started_at: Some(record.started_at),
        finished_at: record.finished_at,
        exit_code: record.exit_code,
        cwd: record.cwd.as_ref().map(|path| path.display().to_string()),
        description: record.description.clone().or_else(|| record.error.clone()),
        stdout: record.stdout.clone(),
        stderr: record.stderr.clone(),
    }
}

impl From<crate::git::GitFileDiff> for WorktreeFileDiffPayload {
    fn from(value: crate::git::GitFileDiff) -> Self {
        Self {
            path: value.path,
            display_path: value.display_path,
            status: value.status,
            diff: value.diff,
        }
    }
}

impl From<crate::git::CommitDiffInfo> for WorktreeCommitDiffPayload {
    fn from(value: crate::git::CommitDiffInfo) -> Self {
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
