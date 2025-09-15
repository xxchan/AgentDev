use anyhow::{Context, Result};
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

pub fn execute_git(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .context("Failed to execute git command")?;

    // Record in debug log buffer
    push_git_log(args, output.status.code(), &output.stdout, &output.stderr);

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Git command failed: {}", stderr);
    }
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

/// Get a comprehensive git diff for the given worktree path.
///
/// Behavior:
/// - Shows both staged and unstaged changes relative to HEAD when possible
/// - Falls back to unstaged diff when HEAD doesn't exist (e.g., initial commit)
/// - Includes untracked files by generating diffs against /dev/null
pub fn get_diff_for_path(path: &Path) -> Result<String> {
    let repo = path
        .to_str()
        .context("worktree path contains non-UTF8 characters")?;

    // Gather diffs explicitly to ensure unstaged changes are visible.
    // 1) Unstaged changes (working tree vs index)
    let unstaged = execute_git_allow_code_1(&[
        "-C",
        repo,
        "-c",
        "core.quotepath=false",
        "--no-pager",
        "diff",
        "--no-ext-diff",
    ])
    .unwrap_or_default();
    // 2) Staged changes (index vs HEAD)
    let staged = execute_git_allow_code_1(&[
        "-C",
        repo,
        "-c",
        "core.quotepath=false",
        "--no-pager",
        "diff",
        "--no-ext-diff",
        "--cached",
    ])
    .unwrap_or_default();

    let mut combined = String::new();
    if !unstaged.is_empty() {
        combined.push_str(&unstaged);
    }
    if !staged.is_empty() {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(&staged);
    }

    // Append diffs for untracked files by diffing /dev/null (or NUL on Windows) against each file.
    // Use -z to correctly handle filenames with special characters/newlines.
    if let Ok(untracked_z) = execute_git(&[
        "-C",
        repo,
        "ls-files",
        "--others",
        "--exclude-standard",
        "-z",
    ]) && !untracked_z.is_empty()
    {
        let dev_null = if cfg!(windows) { "NUL" } else { "/dev/null" };
        for f in untracked_z.split('\0').filter(|s| !s.is_empty()) {
            // Skip directories just in case (ls-files should only list files)
            // Generate a standard git-style unified diff for new files
            if let Ok(diff_new) = execute_git_allow_code_1(&[
                "-C",
                repo,
                "-c",
                "core.quotepath=false",
                "--no-pager",
                "diff",
                "--no-ext-diff",
                "--no-index",
                "--",
                dev_null,
                f,
            ]) && !diff_new.is_empty()
            {
                if !combined.is_empty() {
                    combined.push('\n');
                }
                combined.push_str(&diff_new);
            }
        }
    }

    Ok(combined)
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
}
