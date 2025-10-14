use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

// Simple in-memory ring buffer for recent git command logs (for dashboard debug view)
// Keep this lightweight and dependency-free.
#[derive(Clone, Debug)]
pub struct GitLogEntry {
    pub args: Vec<String>,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct AheadBehind {
    pub ahead: u32,
    pub behind: u32,
}

const GIT_LOG_CAPACITY: usize = 100;
// Global chronological buffer (all git calls)
static GIT_LOGS_GLOBAL: OnceLock<Mutex<VecDeque<GitLogEntry>>> = OnceLock::new();
// Per-worktree buffers, keyed by repo toplevel path
static GIT_LOGS_BY_KEY: OnceLock<Mutex<HashMap<String, VecDeque<GitLogEntry>>>> = OnceLock::new();

fn git_logs_global() -> &'static Mutex<VecDeque<GitLogEntry>> {
    GIT_LOGS_GLOBAL.get_or_init(|| Mutex::new(VecDeque::with_capacity(GIT_LOG_CAPACITY)))
}

fn git_logs_by_key() -> &'static Mutex<HashMap<String, VecDeque<GitLogEntry>>> {
    GIT_LOGS_BY_KEY.get_or_init(|| Mutex::new(HashMap::new()))
}

// Best-effort derive a worktree key for a git invocation
// Priority:
// 1) explicit -C <path> in args
// 2) current directory's repo toplevel (git rev-parse --show-toplevel)
fn detect_worktree_key(args: &[&str]) -> Option<String> {
    // Look for -C <path>
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-C"
            && let Some(p) = args.get(i + 1)
        {
            return Some(p.to_string());
        }
        i += 1;
    }
    // Fallback: query repo toplevel quietly (do not record to logs)
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

fn push_git_log(args: &[&str], exit_code: Option<i32>, stdout: &[u8], stderr: &[u8]) {
    // Truncate outputs to avoid excessive memory usage in the ring buffer
    // Use byte-level truncation before UTF-8 decoding to avoid slicing a
    // `String` at a non-char boundary (which would panic).
    const MAX_FIELD_LEN: usize = 2048; // roughly 2KB per field

    fn lossy_truncate_bytes(input: &[u8], max_len: usize) -> String {
        if input.len() <= max_len {
            return String::from_utf8_lossy(input).to_string();
        }
        let mut s = String::from_utf8_lossy(&input[..max_len]).to_string();
        s.push_str("\n... [truncated]");
        s
    }

    let out = lossy_truncate_bytes(stdout, MAX_FIELD_LEN);
    let err = lossy_truncate_bytes(stderr, MAX_FIELD_LEN);

    let entry = GitLogEntry {
        args: args.iter().map(|s| s.to_string()).collect(),
        exit_code,
        stdout: out,
        stderr: err,
    };

    // Push to global buffer (chronological across worktrees)
    {
        let mut buf = git_logs_global().lock().expect("git logs mutex poisoned");
        if buf.len() >= GIT_LOG_CAPACITY {
            buf.pop_front();
        }
        buf.push_back(entry.clone());
    }

    // Push to per-worktree buffer if we can detect a key
    if let Some(key) = detect_worktree_key(args) {
        let mut map = git_logs_by_key()
            .lock()
            .expect("git logs by key mutex poisoned");
        let buf = map
            .entry(key)
            .or_insert_with(|| VecDeque::with_capacity(GIT_LOG_CAPACITY));
        if buf.len() >= GIT_LOG_CAPACITY {
            buf.pop_front();
        }
        buf.push_back(entry);
    }
}

/// Format a slice of entries for display.
fn format_git_entries(entries: &[GitLogEntry], limit: usize) -> Vec<String> {
    let count = entries.len();
    let start = count.saturating_sub(limit);
    entries
        .iter()
        .skip(start)
        .map(|e| {
            // Reconstruct a simple display string.
            let cmd = {
                let mut parts: Vec<String> = Vec::with_capacity(e.args.len() + 1);
                parts.push("git".to_string());
                for a in &e.args {
                    // Add minimal quoting for whitespace or special parens to aid readability
                    if a.contains(' ') || a.contains('(') || a.contains(')') {
                        parts.push(format!("\"{}\"", a));
                    } else {
                        parts.push(a.clone());
                    }
                }
                parts.join(" ")
            };
            let code = e
                .exit_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "-".to_string());
            let stdout_first = e
                .stdout
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(140)
                .collect::<String>();
            let stderr_first = e
                .stderr
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(140)
                .collect::<String>();
            let out_len = e.stdout.len();
            let err_len = e.stderr.len();
            if !stderr_first.is_empty() {
                format!(
                    "{cmd} => code={code} | stdout {out_len}B: {stdout_first} | stderr {err_len}B: {stderr_first}"
                )
            } else if !stdout_first.is_empty() {
                format!(
                    "{cmd} => code={code} | stdout {out_len}B: {stdout_first}"
                )
            } else {
                format!("{cmd} => code={code} | (no output)")
            }
        })
        .collect()
}

/// Get the most recent git command logs (global buffer) formatted for display.
pub fn recent_git_logs(limit: usize) -> Vec<String> {
    let buf = git_logs_global().lock().expect("git logs mutex poisoned");
    let tmp: Vec<GitLogEntry> = buf.iter().cloned().collect();
    format_git_entries(&tmp, limit)
}

/// Get the most recent git command logs for a specific worktree path.
pub fn recent_git_logs_for_path(path: &Path, limit: usize) -> Vec<String> {
    let key = path.to_string_lossy().to_string();
    let map = git_logs_by_key()
        .lock()
        .expect("git logs by key mutex poisoned");
    if let Some(buf) = map.get(&key) {
        let tmp: Vec<GitLogEntry> = buf.iter().cloned().collect();
        return format_git_entries(&tmp, limit);
    }
    Vec::new()
}

fn format_git_command(args: &[&str]) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(args.len() + 1);
    parts.push("git".to_string());
    for arg in args {
        if arg
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '(' | ')' | '$'))
        {
            parts.push(format!("\"{}\"", arg.replace('"', "\\\"")));
        } else {
            parts.push((*arg).to_string());
        }
    }
    parts.join(" ")
}

fn truncate_output(text: &str) -> String {
    const MAX_LEN: usize = 512;
    if text.len() <= MAX_LEN {
        text.trim().to_string()
    } else {
        let mut truncated = text[..MAX_LEN].trim_end().to_string();
        truncated.push_str("… [truncated]");
        truncated
    }
}

pub fn execute_git(args: &[&str]) -> Result<String> {
    let display_cmd = format_git_command(args);

    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|err| anyhow::anyhow!("Failed to spawn git command: {display_cmd} ({err})"))?;

    // Record in debug log buffer
    push_git_log(args, output.status.code(), &output.stdout, &output.stderr);

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }

    let status = output
        .status
        .code()
        .map(|c| c.to_string())
        .unwrap_or_else(|| "signal".to_string());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut details = String::new();
    if !stderr.trim().is_empty() {
        details.push_str(&format!("stderr: {}", truncate_output(&stderr)));
    }
    if !stdout.trim().is_empty() {
        if !details.is_empty() {
            details.push_str(" | ");
        }
        details.push_str(&format!("stdout: {}", truncate_output(&stdout)));
    }

    if details.is_empty() {
        anyhow::bail!("git command failed (exit {status}): {display_cmd}");
    } else {
        anyhow::bail!("git command failed (exit {status}): {display_cmd} -> {details}");
    }
}

pub fn ahead_behind(local_ref: &str, upstream_ref: &str) -> Result<AheadBehind> {
    let spec = format!("{upstream_ref}...{local_ref}");
    let output = execute_git(&["rev-list", "--left-right", "--count", &spec])?;
    let mut parts = output.split_whitespace();
    let behind_str = parts
        .next()
        .context("Missing behind count from git rev-list output")?;
    let ahead_str = parts
        .next()
        .context("Missing ahead count from git rev-list output")?;
    let behind = behind_str
        .parse::<u32>()
        .with_context(|| format!("Failed to parse behind count from `{behind_str}`"))?;
    let ahead = ahead_str
        .parse::<u32>()
        .with_context(|| format!("Failed to parse ahead count from `{ahead_str}`"))?;
    Ok(AheadBehind { ahead, behind })
}

/// Execute a git command and capture stdout even when exit code is 1.
///
/// Rationale: `git diff` family returns exit code 1 when differences are found,
/// which is not an error for our use cases. Other commands should still use
/// `execute_git` to get strict error handling.
fn execute_git_allow_code_1(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .context("Failed to execute git command")?;

    // Record in debug log buffer
    push_git_log(args, output.status.code(), &output.stdout, &output.stderr);

    let code = output.status.code().unwrap_or(-1);
    if output.status.success() || code == 1 {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Git command failed: {}", stderr);
    }
}

pub fn get_repo_name() -> Result<String> {
    // First, try to get the repository name from the remote URL
    // This gives us the true repository name regardless of local directory name
    if let Ok(remote_url) = execute_git(&["remote", "get-url", "origin"]) {
        // Extract repo name from URL
        // Supports:
        // - https://github.com/user/repo.git
        // - git@github.com:user/repo.git
        // - https://gitlab.com/user/repo
        // - /path/to/local/repo.git
        let repo_name = if let Some(name) = extract_repo_name_from_url(&remote_url) {
            name
        } else {
            // Fallback to directory name if URL parsing fails
            get_repo_name_from_directory()?
        };
        return Ok(repo_name);
    }

    // If no remote, use the directory name of the main repository
    get_repo_name_from_directory()
}

pub fn extract_repo_name_from_url(url: &str) -> Option<String> {
    let url = url.trim();

    // Remove .git suffix if present
    let url = url.strip_suffix(".git").unwrap_or(url);

    // Handle SSH URLs (git@github.com:user/repo)
    if url.starts_with("git@") {
        return url
            .split(':')
            .nth(1)
            .and_then(|path| path.split('/').next_back())
            .map(|s| s.to_string());
    }

    // Handle HTTP(S) URLs and file paths
    url.split('/')
        .next_back()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn get_repo_name_from_directory() -> Result<String> {
    // For worktrees, we need to get the main repository path
    // Try to get the common git directory first (which points to main repo for worktrees)
    let git_common_dir = execute_git(&["rev-parse", "--git-common-dir"])?;
    let git_dir = execute_git(&["rev-parse", "--git-dir"])?;

    let repo_path = if git_common_dir != git_dir {
        // We're in a worktree - git-common-dir points to main repo's .git
        let path = Path::new(&git_common_dir);
        if path.file_name().is_some_and(|n| n == ".git") {
            // Get the parent directory which is the main repo
            path.parent()
                .and_then(|p| p.to_str())
                .map(|s| s.to_string())
                .context("Failed to get main repository path")?
        } else {
            // git-common-dir doesn't end with .git, use it directly
            git_common_dir
        }
    } else {
        // Not in a worktree, use toplevel
        execute_git(&["rev-parse", "--show-toplevel"])?
    };

    let path = Path::new(&repo_path);
    path.file_name()
        .and_then(|n| n.to_str())
        .map(std::string::ToString::to_string)
        .context("Failed to get repository name")
}

pub fn get_current_branch() -> Result<String> {
    execute_git(&["symbolic-ref", "--short", "HEAD"])
}

pub fn get_default_branch() -> Result<String> {
    // Try to get the default branch from remote HEAD
    if let Ok(output) = execute_git(&["remote", "show", "origin"]) {
        for line in output.lines() {
            if let Some(branch) = line.strip_prefix("  HEAD branch: ") {
                return Ok(branch.trim().to_string());
            }
        }
    }

    // Fallback: try to get HEAD from symbolic-ref
    if let Ok(output) = execute_git(&["symbolic-ref", "refs/remotes/origin/HEAD"])
        && let Some(branch) = output.strip_prefix("refs/remotes/origin/")
    {
        return Ok(branch.to_string());
    }

    // Final fallback: return "main" as the most common default
    Ok("main".to_string())
}

pub fn is_base_branch() -> Result<bool> {
    let current = get_current_branch()?;

    // Get the actual default branch from remote
    let default_branch = get_default_branch().unwrap_or_else(|_| "main".to_string());

    // Check if current branch is the default branch
    if current == default_branch {
        return Ok(true);
    }

    // Also allow common base branches for flexibility
    let common_base_branches = ["main", "master", "develop"];
    Ok(common_base_branches.contains(&current.as_str()))
}

#[allow(dead_code)]
pub fn branch_exists(branch_name: &str) -> Result<bool> {
    // Check if branch exists locally
    if execute_git(&[
        "show-ref",
        "--verify",
        "--quiet",
        &format!("refs/heads/{}", branch_name),
    ])
    .is_ok()
    {
        return Ok(true);
    }

    // Check if branch exists on remote
    if execute_git(&[
        "show-ref",
        "--verify",
        "--quiet",
        &format!("refs/remotes/origin/{}", branch_name),
    ])
    .is_ok()
    {
        return Ok(true);
    }

    Ok(false)
}

pub fn is_working_tree_clean() -> Result<bool> {
    let status = execute_git(&["status", "--porcelain"])?;
    Ok(status.is_empty())
}

pub fn has_unpushed_commits() -> bool {
    execute_git(&["log", "@{u}.."]).is_ok_and(|output| !output.is_empty())
}

pub fn is_in_worktree() -> Result<bool> {
    // Check if we're in a worktree by looking for .git file (not directory)
    let git_path = Path::new(".git");
    if git_path.exists() && git_path.is_file() {
        return Ok(true);
    }

    // Alternative: check git worktree list
    match execute_git(&["rev-parse", "--git-common-dir"]) {
        Ok(common_dir) => {
            let current_git_dir = execute_git(&["rev-parse", "--git-dir"])?;
            if common_dir != current_git_dir {
                return Ok(true);
            }
            // Fallback: if inside a git work tree, treat as worktree context
            // Note: main repo will also return true here, but callers typically
            // combine with `!is_base_branch()` to exclude base branches.
            if let Ok(val) = execute_git(&["rev-parse", "--is-inside-work-tree"]) {
                return Ok(val.trim() == "true");
            }
            Ok(false)
        }
        Err(_) => Ok(false),
    }
}

pub fn list_worktrees() -> Result<Vec<PathBuf>> {
    let output = execute_git(&["worktree", "list", "--porcelain"])?;
    let mut worktrees = Vec::new();

    for line in output.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            worktrees.push(PathBuf::from(path));
        }
    }

    Ok(worktrees)
}

pub fn update_submodules(worktree_path: &Path) -> Result<()> {
    // Check if submodules exist
    let gitmodules = worktree_path.join(".gitmodules");
    if !gitmodules.exists() {
        return Ok(());
    }

    // Initialize and update submodules using git -C
    execute_git(&[
        "-C",
        worktree_path.to_str().unwrap(),
        "submodule",
        "update",
        "--init",
        "--recursive",
    ])
    .context("Failed to update submodules")?;

    Ok(())
}

fn detect_default_branch_for_repo(repo: &str) -> Option<String> {
    if let Ok(output) = execute_git(&["-C", repo, "remote", "show", "origin"]) {
        for line in output.lines() {
            if let Some(branch) = line.strip_prefix("  HEAD branch: ") {
                return Some(branch.trim().to_string());
            }
        }
    }

    if let Ok(output) = execute_git(&["-C", repo, "symbolic-ref", "refs/remotes/origin/HEAD"]) {
        if let Some(branch) = output.strip_prefix("refs/remotes/origin/") {
            return Some(branch.to_string());
        }
    }

    None
}

#[derive(Debug, Clone)]
pub struct WorktreeGitStatus {
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

#[derive(Debug, Clone)]
pub struct HeadCommitInfo {
    pub commit_id: String,
    pub summary: String,
    pub timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct CommitDiffInfo {
    pub reference: String,
    pub diff: String,
}

#[derive(Debug, Clone)]
pub struct GitFileDiff {
    pub path: String,
    pub display_path: String,
    pub status: String,
    pub diff: String,
}

#[derive(Debug, Clone, Default)]
pub struct WorktreeDiffBreakdown {
    pub commit: Option<CommitDiffInfo>,
    pub staged: Vec<GitFileDiff>,
    pub unstaged: Vec<GitFileDiff>,
    pub untracked: Vec<GitFileDiff>,
}

/// Summarize the git status for a worktree.
pub fn summarize_worktree_status(path: &Path, fallback_branch: &str) -> Result<WorktreeGitStatus> {
    let repo = path
        .to_str()
        .context("worktree path contains invalid UTF-8")?;

    let raw = execute_git(&["-C", repo, "status", "--porcelain=2", "--branch"])?;

    let mut status = WorktreeGitStatus {
        branch: String::new(),
        upstream: None,
        ahead: 0,
        behind: 0,
        staged: 0,
        unstaged: 0,
        untracked: 0,
        conflicts: 0,
        is_clean: true,
    };

    for line in raw.lines() {
        if let Some(head) = line.strip_prefix("# branch.head ") {
            status.branch = head.trim().to_string();
            continue;
        }
        if let Some(upstream) = line.strip_prefix("# branch.upstream ") {
            status.upstream = Some(upstream.trim().to_string());
            continue;
        }
        if let Some(ab) = line.strip_prefix("# branch.ab ") {
            for token in ab.split_whitespace() {
                if let Some(val) = token.strip_prefix('+') {
                    if let Ok(parsed) = val.parse::<u32>() {
                        status.ahead = parsed;
                    }
                } else if let Some(val) = token.strip_prefix('-') {
                    if let Ok(parsed) = val.parse::<u32>() {
                        status.behind = parsed;
                    }
                }
            }
            continue;
        }
        if line.starts_with("? ") {
            status.untracked += 1;
            continue;
        }
        if line.starts_with("! ") {
            continue;
        }
        if line.starts_with("u ") {
            status.conflicts += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix("1 ") {
            note_status_tokens(&mut status, rest);
            continue;
        }
        if let Some(rest) = line.strip_prefix("2 ") {
            note_status_tokens(&mut status, rest);
            continue;
        }
    }

    if status.branch.is_empty() {
        status.branch = fallback_branch.to_string();
    }
    status.is_clean = status.staged == 0
        && status.unstaged == 0
        && status.untracked == 0
        && status.conflicts == 0;

    Ok(status)
}

fn note_status_tokens(status: &mut WorktreeGitStatus, rest: &str) {
    if let Some(token) = rest.split_whitespace().next() {
        let mut chars = token.chars();
        let index = chars.next().unwrap_or('.');
        let worktree = chars.next().unwrap_or('.');

        let conflict = index == 'U' || worktree == 'U';
        if conflict {
            status.conflicts += 1;
            return;
        }
        if index != '.' {
            status.staged += 1;
        }
        if worktree != '.' {
            status.unstaged += 1;
        }
    }
}

/// Get information about the HEAD commit in a worktree, if any.
pub fn head_commit_info(path: &Path) -> Result<Option<HeadCommitInfo>> {
    let repo = path
        .to_str()
        .context("worktree path contains invalid UTF-8")?;

    let args = ["-C", repo, "log", "-1", "--pretty=format:%H%x00%ct%x00%s"];

    let raw = match execute_git(&args) {
        Ok(output) => output,
        Err(err) => {
            let message = err.to_string();
            if message.contains("does not have any commits yet")
                || message.contains("unknown revision or path not in the working tree")
                || message.contains("Needed a single revision")
            {
                return Ok(None);
            }
            return Err(err);
        }
    };

    if raw.is_empty() {
        return Ok(None);
    }

    let mut parts = raw.split('\0');
    let commit_id = parts.next().unwrap_or_default().trim().to_string();
    let timestamp = parts
        .next()
        .and_then(|ts| ts.parse::<i64>().ok())
        .and_then(|ts| Utc.timestamp_opt(ts, 0).single());
    let summary = parts.next().unwrap_or_default().trim().to_string();

    if commit_id.is_empty() && summary.is_empty() {
        return Ok(None);
    }

    Ok(Some(HeadCommitInfo {
        commit_id,
        summary,
        timestamp,
    }))
}

struct GitNameStatusRecord {
    status: String,
    diff_path: String,
    display_path: String,
}

fn compute_commit_diff_for_repo(repo: &str) -> Option<CommitDiffInfo> {
    let mut branch_candidates: Vec<String> = Vec::new();
    if let Some(detected) = detect_default_branch_for_repo(repo) {
        branch_candidates.push(detected);
    }
    for fallback in ["main", "master"] {
        if !branch_candidates.iter().any(|b| b == fallback) {
            branch_candidates.push(fallback.to_string());
        }
    }

    for branch in branch_candidates {
        let remote_ref = format!("origin/{branch}");
        let remote_range = format!("{remote_ref}...HEAD");
        if let Some(diff) = diff_against(repo, remote_range.as_str()) {
            return Some(CommitDiffInfo {
                reference: remote_ref,
                diff,
            });
        }

        let local_range = format!("{branch}...HEAD");
        if let Some(diff) = diff_against(repo, local_range.as_str()) {
            return Some(CommitDiffInfo {
                reference: branch,
                diff,
            });
        }
    }

    None
}

fn diff_against(repo: &str, range: &str) -> Option<String> {
    let args = [
        "-C",
        repo,
        "-c",
        "core.quotepath=false",
        "--no-pager",
        "diff",
        "--no-ext-diff",
        range,
    ];
    execute_git_allow_code_1(&args)
        .ok()
        .filter(|s| !s.trim().is_empty())
}

fn parse_name_status_output(output: &str) -> Vec<GitNameStatusRecord> {
    let mut records = Vec::new();
    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let status = parts
            .next()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let Some(status) = status else {
            continue;
        };

        if status.starts_with('R') || status.starts_with('C') {
            let from = parts.next().unwrap_or("").trim();
            let to = parts.next().unwrap_or("").trim();
            if from.is_empty() || to.is_empty() {
                continue;
            }
            records.push(GitNameStatusRecord {
                status,
                diff_path: to.to_string(),
                display_path: format!("{from} -> {to}"),
            });
        } else if let Some(path) = parts.next() {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                continue;
            }
            records.push(GitNameStatusRecord {
                status,
                diff_path: trimmed.to_string(),
                display_path: trimmed.to_string(),
            });
        }
    }
    records
}

fn diff_for_file(repo: &str, path: &str, staged: bool) -> Result<String> {
    let mut args: Vec<String> = vec![
        "-C".to_string(),
        repo.to_string(),
        "-c".to_string(),
        "core.quotepath=false".to_string(),
        "--no-pager".to_string(),
        "diff".to_string(),
        "--no-ext-diff".to_string(),
    ];
    if staged {
        args.push("--cached".to_string());
    }
    args.push("--".to_string());
    args.push(path.to_string());

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    execute_git_allow_code_1(&arg_refs).map(|s| s.trim().to_string())
}

fn diff_for_untracked_file(repo: &str, path: &str) -> Result<String> {
    let dev_null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let args: Vec<String> = vec![
        "-C".to_string(),
        repo.to_string(),
        "-c".to_string(),
        "core.quotepath=false".to_string(),
        "--no-pager".to_string(),
        "diff".to_string(),
        "--no-ext-diff".to_string(),
        "--no-index".to_string(),
        "--".to_string(),
        dev_null.to_string(),
        path.to_string(),
    ];

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    execute_git_allow_code_1(&arg_refs).map(|s| s.trim().to_string())
}

pub fn collect_worktree_diff_breakdown(path: &Path) -> Result<WorktreeDiffBreakdown> {
    let repo = path
        .to_str()
        .context("worktree path contains non-UTF8 characters")?;

    let mut breakdown = WorktreeDiffBreakdown {
        commit: compute_commit_diff_for_repo(repo),
        staged: Vec::new(),
        unstaged: Vec::new(),
        untracked: Vec::new(),
    };

    let staged_output = execute_git_allow_code_1(&[
        "-C",
        repo,
        "--no-pager",
        "diff",
        "--name-status",
        "--no-ext-diff",
        "--cached",
    ])
    .unwrap_or_default();

    for record in parse_name_status_output(&staged_output) {
        if let Ok(diff) = diff_for_file(repo, &record.diff_path, true) {
            breakdown.staged.push(GitFileDiff {
                path: record.diff_path,
                display_path: record.display_path,
                status: record.status,
                diff,
            });
        }
    }

    let unstaged_output = execute_git_allow_code_1(&[
        "-C",
        repo,
        "--no-pager",
        "diff",
        "--name-status",
        "--no-ext-diff",
    ])
    .unwrap_or_default();

    for record in parse_name_status_output(&unstaged_output) {
        if let Ok(diff) = diff_for_file(repo, &record.diff_path, false) {
            breakdown.unstaged.push(GitFileDiff {
                path: record.diff_path,
                display_path: record.display_path,
                status: record.status,
                diff,
            });
        }
    }

    let untracked_output = execute_git(&["-C", repo, "ls-files", "--others", "--exclude-standard"])
        .unwrap_or_default();

    for path in untracked_output
        .lines()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        if let Ok(diff) = diff_for_untracked_file(repo, path) {
            breakdown.untracked.push(GitFileDiff {
                path: path.to_string(),
                display_path: path.to_string(),
                status: "??".to_string(),
                diff,
            });
        }
    }

    Ok(breakdown)
}

/// Get a comprehensive git diff for the given worktree path.
///
/// Behavior:
/// - Shows committed changes relative to the repo's default branch when available
/// - Shows both staged and unstaged changes relative to HEAD
/// - Falls back to unstaged diff when HEAD doesn't exist (e.g., initial commit)
/// - Includes untracked files by generating diffs against /dev/null
pub fn get_diff_for_path(path: &Path) -> Result<String> {
    let breakdown = collect_worktree_diff_breakdown(path)?;

    let mut sections: Vec<String> = Vec::new();

    if let Some(commit) = breakdown.commit {
        if !commit.diff.is_empty() {
            sections.push(format!(
                "### Committed changes (vs {})\n{}",
                commit.reference, commit.diff
            ));
        }
    }

    if !breakdown.unstaged.is_empty() {
        let content = breakdown
            .unstaged
            .iter()
            .map(|entry| entry.diff.as_str())
            .filter(|diff| !diff.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if !content.trim().is_empty() {
            sections.push(format!("### Unstaged changes\n{content}"));
        }
    }

    if !breakdown.staged.is_empty() {
        let content = breakdown
            .staged
            .iter()
            .map(|entry| entry.diff.as_str())
            .filter(|diff| !diff.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if !content.trim().is_empty() {
            sections.push(format!("### Staged changes\n{content}"));
        }
    }

    if !breakdown.untracked.is_empty() {
        for entry in breakdown.untracked {
            if entry.diff.trim().is_empty() {
                continue;
            }
            sections.push(format!(
                "### Untracked file: {}\n{}",
                entry.display_path, entry.diff
            ));
        }
    }

    Ok(sections.join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_repo_name_from_url() {
        // GitHub HTTPS
        assert_eq!(
            extract_repo_name_from_url("https://github.com/user/my-repo.git"),
            Some("my-repo".to_string())
        );

        // GitHub SSH
        assert_eq!(
            extract_repo_name_from_url("git@github.com:user/my-repo.git"),
            Some("my-repo".to_string())
        );

        // GitLab HTTPS without .git
        assert_eq!(
            extract_repo_name_from_url("https://gitlab.com/user/my-repo"),
            Some("my-repo".to_string())
        );

        // Local path
        assert_eq!(
            extract_repo_name_from_url("/path/to/repos/my-repo.git"),
            Some("my-repo".to_string())
        );

        // Complex repo name
        assert_eq!(
            extract_repo_name_from_url("git@github.com:xuanwo/xlaude-enable.git"),
            Some("xlaude-enable".to_string())
        );

        // Edge cases
        assert_eq!(
            extract_repo_name_from_url("https://github.com/user/repo-with-dots.v2.git"),
            Some("repo-with-dots.v2".to_string())
        );
    }

    #[test]
    fn test_get_default_branch() {
        // This test will work based on the actual git repository it's run in
        // We can't make strong assertions about the result since it depends on the repo
        let result = get_default_branch();

        // Should either succeed with a non-empty string or fail gracefully
        match result {
            Ok(branch) => {
                assert!(!branch.is_empty());
                // Common default branches
                assert!(
                    ["main", "master", "develop"].contains(&branch.as_str()) || !branch.is_empty()
                );
            }
            Err(_) => {
                // It's okay to fail if we're not in a git repo or no remote
                // The function should handle this gracefully
            }
        }
    }

    #[test]
    fn test_get_diff_for_path_includes_committed_changes() {
        use std::fs;

        let temp = tempfile::tempdir().expect("create temp dir");
        let repo_path = temp.path();

        let run_git = |args: &[&str]| {
            let status = Command::new("git")
                .args(args)
                .current_dir(repo_path)
                .status()
                .expect("execute git command");
            assert!(status.success(), "git {:?} failed", args);
        };

        run_git(&["init", "--initial-branch=main"]);
        run_git(&["config", "user.email", "test@example.com"]);
        run_git(&["config", "user.name", "Tester"]);

        fs::write(repo_path.join("note.txt"), "line1\n").expect("write file");
        run_git(&["add", "note.txt"]);
        run_git(&["commit", "-m", "initial"]);

        run_git(&["checkout", "-b", "feature"]);
        fs::write(repo_path.join("note.txt"), "line1\nline2\n").expect("update file");
        run_git(&["add", "note.txt"]);
        run_git(&["commit", "-m", "feature work"]);

        let diff = get_diff_for_path(repo_path).expect("collect diff");
        assert!(
            diff.contains("+line2"),
            "diff did not include committed change: {diff}"
        );
    }
}
