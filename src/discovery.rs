use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use crate::git::execute_git;
use crate::state::{WorktreeInfo, XlaudeState};
use crate::utils::sanitize_branch_name;

pub const MAX_RECURSIVE_DEPTH: usize = 6;
pub const MAX_REPOS_DISCOVERED: usize = 64;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DiscoveredWorktree {
    pub repo: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prunable: Option<String>,
    pub bare: bool,
}

#[derive(Debug, Clone)]
pub struct DiscoveryOptions {
    pub recursive: bool,
    pub root: Option<PathBuf>,
}

impl Default for DiscoveryOptions {
    fn default() -> Self {
        Self {
            recursive: false,
            root: None,
        }
    }
}

pub fn discover_worktrees(options: DiscoveryOptions) -> Result<Vec<DiscoveredWorktree>> {
    let root = options
        .root
        .unwrap_or(std::env::current_dir().context("Failed to determine current directory")?);

    let repos = if options.recursive {
        discover_repositories_recursive(&root)?
    } else {
        vec![resolve_repo_root(&root)?]
    };

    if repos.is_empty() {
        return Ok(Vec::new());
    }

    let state = XlaudeState::load()?;
    let managed_paths = build_managed_path_index(&state)?;
    let mut seen_paths: HashSet<String> = HashSet::new();
    let mut discovered = Vec::new();

    for repo in repos {
        let mut entries = discover_repo_worktrees(&repo, &managed_paths, &mut seen_paths)?;
        discovered.append(&mut entries);
    }

    discovered.sort_by(|a, b| {
        a.repo
            .cmp(&b.repo)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.branch.cmp(&b.branch))
    });

    Ok(discovered)
}

/// Persist discovered worktrees into xlaude/agentdev state, assigning
/// reasonable names for each newly tracked worktree.
///
/// Returns the list of worktree infos that were newly added (in the same
/// order as `entries` were provided). Existing managed worktrees are skipped.
pub fn add_discovered_to_state(entries: &[DiscoveredWorktree]) -> Result<Vec<WorktreeInfo>> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }

    let mut state = XlaudeState::load()?;
    let mut added: Vec<WorktreeInfo> = Vec::new();

    let mut existing_paths: HashMap<String, String> = HashMap::new();
    for (key, info) in &state.worktrees {
        let canonical = canonical_string(&info.path)?;
        existing_paths.insert(canonical, key.clone());
    }

    for entry in entries {
        let path = PathBuf::from(&entry.path);
        if !path.exists() {
            continue;
        }

        let canonical = canonical_string(&path)?;
        if existing_paths.contains_key(&canonical) {
            continue;
        }

        let repo_path = PathBuf::from(&entry.repo);
        let repo_name = repo_path
            .file_name()
            .and_then(|v| v.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| repo_path.display().to_string());

        let desired_name = entry
            .branch
            .as_deref()
            .map(sanitize_branch_name)
            .or_else(|| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .map(sanitize_branch_name)
            })
            .unwrap_or_else(|| format!("worktree-{}", state.worktrees.len() + 1));

        let final_name = ensure_unique_name(&state, &repo_name, desired_name);
        let key = XlaudeState::make_key(&repo_name, &final_name);

        let info = WorktreeInfo {
            name: final_name.clone(),
            branch: entry.branch.clone().unwrap_or_else(|| final_name.clone()),
            path: path.clone(),
            repo_name: repo_name.clone(),
            created_at: Utc::now(),
            task_id: None,
            task_name: None,
            initial_prompt: None,
            agent_alias: None,
        };

        state.worktrees.insert(key.clone(), info.clone());
        existing_paths.insert(canonical, key);
        added.push(info);
    }

    if !added.is_empty() {
        state.save()?;
    }

    Ok(added)
}

fn resolve_repo_root(start: &Path) -> Result<PathBuf> {
    let cmd = execute_git(&["-C", path_to_str(start)?, "rev-parse", "--show-toplevel"]);
    match cmd {
        Ok(path) => {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                Err(anyhow!("Not inside a git repository"))
            } else {
                Ok(PathBuf::from(trimmed))
            }
        }
        Err(err) => Err(anyhow!("Failed to locate git repository: {err}")),
    }
}

fn discover_repositories_recursive(start: &Path) -> Result<Vec<PathBuf>> {
    let mut repos = Vec::new();
    let mut seen_keys: HashSet<String> = HashSet::new();
    let mut stack: VecDeque<(PathBuf, usize)> = VecDeque::new();
    stack.push_back((start.to_path_buf(), 0));

    while let Some((dir, depth)) = stack.pop_back() {
        if depth > MAX_RECURSIVE_DEPTH {
            continue;
        }

        let is_repo = is_git_repo_root(&dir);
        if is_repo {
            let key = canonical_string(&dir)?;
            if seen_keys.insert(key.clone()) {
                repos.push(PathBuf::from(&key));
                if repos.len() > MAX_REPOS_DISCOVERED {
                    anyhow::bail!(
                        "Reached discovery limit of {MAX_REPOS_DISCOVERED} repositories. Narrow your search or raise the limit."
                    );
                }
            }

            if depth > 0 {
                // Avoid descending into nested repositories to keep runtime bounded.
                continue;
            }
        }

        if depth == MAX_RECURSIVE_DEPTH {
            continue;
        }

        let read_dir = match fs::read_dir(&dir) {
            Ok(read_dir) => read_dir,
            Err(_) => continue,
        };

        for entry in read_dir.flatten() {
            if !entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                continue;
            }
            let path = entry.path();
            if should_skip_dir(&path) {
                continue;
            }
            stack.push_back((path, depth + 1));
        }
    }

    repos.sort();
    repos.dedup();
    Ok(repos)
}

fn discover_repo_worktrees(
    repo_root: &Path,
    managed_paths: &HashSet<String>,
    seen_paths: &mut HashSet<String>,
) -> Result<Vec<DiscoveredWorktree>> {
    let repo_str = path_to_str(repo_root)?;
    let output = execute_git(&["-C", repo_str, "worktree", "list", "--porcelain"])?;
    let repo_key = canonical_string(repo_root)?;

    let entries = parse_git_worktree_porcelain(&output)?;
    let mut result = Vec::new();

    for entry in entries {
        let canonical = canonical_string(&entry.path)?;
        if canonical == repo_key {
            continue;
        }
        if managed_paths.contains(&canonical) {
            continue;
        }
        if !seen_paths.insert(canonical.clone()) {
            continue;
        }

        result.push(DiscoveredWorktree {
            repo: repo_str.to_string(),
            path: entry.path.to_string_lossy().to_string(),
            branch: entry.branch,
            head: entry.head,
            locked: entry.locked,
            prunable: entry.prunable,
            bare: entry.bare,
        });
    }

    Ok(result)
}

fn build_managed_path_index(state: &XlaudeState) -> Result<HashSet<String>> {
    let mut managed = HashSet::new();
    for info in state.worktrees.values() {
        managed.insert(canonical_string(&info.path)?);
    }
    Ok(managed)
}

fn parse_git_worktree_porcelain(output: &str) -> Result<Vec<GitWorktreeRecord>> {
    let mut entries = Vec::new();
    let mut current = ParsedWorktreeEntry::default();

    for line in output.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            if current.path.is_some() {
                entries.push(current.finish()?);
            }
            current = ParsedWorktreeEntry::default();
            continue;
        }

        if let Some(path) = trimmed.strip_prefix("worktree ") {
            current.path = Some(PathBuf::from(path));
        } else if let Some(head) = trimmed.strip_prefix("HEAD ") {
            current.head = Some(head.to_string());
        } else if let Some(branch) = trimmed.strip_prefix("branch ") {
            current.branch = Some(shorten_ref(branch));
        } else if let Some(detached) = trimmed.strip_prefix("detached ") {
            current.head = Some(detached.to_string());
        } else if trimmed == "bare" {
            current.bare = true;
        } else if let Some(rest) = trimmed.strip_prefix("locked ") {
            current.locked = Some(rest.to_string());
        } else if trimmed == "locked" {
            current.locked = Some(String::from("(no reason provided)"));
        } else if let Some(rest) = trimmed.strip_prefix("prunable ") {
            current.prunable = Some(rest.to_string());
        }
    }

    if current.path.is_some() {
        entries.push(current.finish()?);
    }

    Ok(entries)
}

#[derive(Default)]
struct ParsedWorktreeEntry {
    path: Option<PathBuf>,
    branch: Option<String>,
    head: Option<String>,
    bare: bool,
    locked: Option<String>,
    prunable: Option<String>,
}

impl ParsedWorktreeEntry {
    fn finish(self) -> Result<GitWorktreeRecord> {
        let path = self
            .path
            .ok_or_else(|| anyhow!("Missing worktree path in git output"))?;
        Ok(GitWorktreeRecord {
            path,
            branch: self.branch,
            head: self.head,
            bare: self.bare,
            locked: self.locked,
            prunable: self.prunable,
        })
    }
}

struct GitWorktreeRecord {
    path: PathBuf,
    branch: Option<String>,
    head: Option<String>,
    bare: bool,
    locked: Option<String>,
    prunable: Option<String>,
}

fn shorten_ref(raw: &str) -> String {
    raw.strip_prefix("refs/heads/")
        .or_else(|| raw.strip_prefix("refs/remotes/"))
        .map(|value| value.to_string())
        .unwrap_or_else(|| raw.to_string())
}

fn canonical_string(path: &Path) -> Result<String> {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let display = canonical.to_string_lossy();
    #[cfg(windows)]
    {
        Ok(display.to_lowercase())
    }
    #[cfg(not(windows))]
    {
        Ok(display.into_owned())
    }
}

fn path_to_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow!("Path contains invalid UTF-8: {}", path.display()))
}

fn should_skip_dir(path: &Path) -> bool {
    const SKIP_NAMES: [&str; 7] = [
        ".git",
        "node_modules",
        "target",
        ".next",
        "out",
        "dist",
        "build",
    ];

    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    SKIP_NAMES.contains(&name)
}

fn is_git_repo_root(path: &Path) -> bool {
    if should_skip_dir(path) {
        return false;
    }

    if path.join(".git").exists() {
        return true;
    }

    if path.file_name().is_some_and(|name| name == ".git") {
        return false;
    }

    path.join("HEAD").exists() && path.join("config").exists()
}

fn ensure_unique_name(state: &XlaudeState, repo_name: &str, desired: String) -> String {
    let mut name = desired;
    let mut counter = 2;

    loop {
        let key = XlaudeState::make_key(repo_name, &name);
        if !state.worktrees.contains_key(&key) {
            return name;
        }
        name = format!("{name}-{counter}");
        counter += 1;
    }
}
