use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use crate::claude::get_claude_sessions;
use crate::git::execute_git;
use crate::state::XlaudeState;

#[derive(Debug, Serialize, Deserialize)]
struct JsonSessionInfo {
    last_user_message: String,
    last_timestamp: Option<DateTime<Utc>>,
    time_ago: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonGitStatus {
    branch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    upstream: Option<String>,
    ahead: u32,
    behind: u32,
    staged: usize,
    unstaged: usize,
    untracked: usize,
    conflicts: usize,
    is_clean: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonCommitInfo {
    commit_id: String,
    summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonWorktreeInfo {
    name: String,
    branch: String,
    path: String,
    repo_name: String,
    created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    task_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    initial_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_alias: Option<String>,
    last_activity_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    git_status: Option<JsonGitStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    head_commit: Option<JsonCommitInfo>,
    sessions: Vec<JsonSessionInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonOutput {
    worktrees: Vec<JsonWorktreeInfo>,
}

pub fn handle_list(json: bool) -> Result<()> {
    let state = XlaudeState::load()?;

    if state.worktrees.is_empty() {
        if json {
            let output = JsonOutput { worktrees: vec![] };
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("{} No active worktrees", "📭".yellow());
        }
        return Ok(());
    }

    if json {
        // JSON output
        let mut worktrees = Vec::new();

        for info in state.worktrees.values() {
            let sessions = get_claude_sessions(&info.path);
            let json_sessions: Vec<JsonSessionInfo> = sessions
                .into_iter()
                .map(|session| {
                    let time_ago = session.last_timestamp.as_ref().map_or_else(
                        || "unknown".to_string(),
                        |ts| {
                            let now = Utc::now();
                            let diff = now.signed_duration_since(*ts);

                            if diff.num_minutes() < 60 {
                                format!("{}m ago", diff.num_minutes())
                            } else if diff.num_hours() < 24 {
                                format!("{}h ago", diff.num_hours())
                            } else {
                                format!("{}d ago", diff.num_days())
                            }
                        },
                    );

                    JsonSessionInfo {
                        last_user_message: session.last_user_message,
                        last_timestamp: session.last_timestamp,
                        time_ago,
                    }
                })
                .collect();

            let git_status = summarize_git_status(&info.path, &info.branch)
                .map_err(|err| {
                    eprintln!(
                        "⚠️  Failed to inspect git status for {}: {err}",
                        info.path.display()
                    );
                    err
                })
                .ok();

            let head_commit = collect_head_commit(&info.path)
                .map_err(|err| {
                    eprintln!(
                        "⚠️  Failed to read last commit for {}: {err}",
                        info.path.display()
                    );
                    err
                })
                .ok()
                .flatten();

            let mut last_activity = info.created_at;
            if let Some(ref commit) = head_commit {
                if let Some(ts) = commit.timestamp {
                    if ts > last_activity {
                        last_activity = ts;
                    }
                }
            }
            for session in &json_sessions {
                if let Some(ts) = session.last_timestamp {
                    if ts > last_activity {
                        last_activity = ts;
                    }
                }
            }

            worktrees.push(JsonWorktreeInfo {
                name: info.name.clone(),
                branch: info.branch.clone(),
                path: info.path.display().to_string(),
                repo_name: info.repo_name.clone(),
                created_at: info.created_at,
                task_id: info.task_id.clone(),
                task_name: info.task_name.clone(),
                initial_prompt: info.initial_prompt.clone(),
                agent_alias: info.agent_alias.clone(),
                last_activity_at: last_activity,
                git_status,
                head_commit,
                sessions: json_sessions,
            });
        }

        // Sort worktrees by repo name and then by name
        worktrees.sort_by(|a, b| {
            a.repo_name
                .cmp(&b.repo_name)
                .then_with(|| a.name.cmp(&b.name))
        });

        let output = JsonOutput { worktrees };
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        // Original colored output
        println!("{} Active worktrees:", "📋".cyan());
        println!();

        // Group worktrees by repository
        let mut grouped: BTreeMap<String, Vec<_>> = BTreeMap::new();
        for info in state.worktrees.values() {
            grouped
                .entry(info.repo_name.clone())
                .or_default()
                .push(info);
        }

        // Display grouped by repository
        for (repo_name, mut worktrees) in grouped {
            println!("  {} {}", "📦".blue(), repo_name.bold());

            // Sort worktrees within each repo by name
            worktrees.sort_by_key(|w| &w.name);

            for info in worktrees {
                println!("    {} {}", "•".green(), info.name.cyan());
                println!("      {} {}", "Path:".bright_black(), info.path.display());
                println!(
                    "      {} {}",
                    "Created:".bright_black(),
                    info.created_at.format("%Y-%m-%d %H:%M:%S")
                );

                // Get Claude sessions for this worktree
                let sessions = get_claude_sessions(&info.path);
                if !sessions.is_empty() {
                    println!(
                        "      {} {} session(s):",
                        "Claude:".bright_black(),
                        sessions.len()
                    );
                    for session in sessions.iter().take(3) {
                        // Format time
                        let time_str = session.last_timestamp.as_ref().map_or_else(
                            || "unknown".to_string(),
                            |ts| {
                                let now = Utc::now();
                                let diff = now.signed_duration_since(*ts);

                                if diff.num_minutes() < 60 {
                                    format!("{}m ago", diff.num_minutes())
                                } else if diff.num_hours() < 24 {
                                    format!("{}h ago", diff.num_hours())
                                } else {
                                    format!("{}d ago", diff.num_days())
                                }
                            },
                        );

                        // Truncate message if too long
                        let message = if session.last_user_message.len() > 60 {
                            let mut truncated = String::new();
                            for ch in session.last_user_message.chars() {
                                if truncated.len() + ch.len_utf8() > 57 {
                                    break;
                                }
                                truncated.push(ch);
                            }
                            format!("{truncated}...")
                        } else {
                            session.last_user_message.clone()
                        };

                        println!(
                            "        {} {} {}",
                            "-".bright_black(),
                            time_str.bright_black(),
                            message.bright_black()
                        );
                    }
                    if sessions.len() > 3 {
                        println!(
                            "        {} ... and {} more",
                            "-".bright_black(),
                            sessions.len() - 3
                        );
                    }
                }
            }
            println!();
        }
    }

    Ok(())
}

fn summarize_git_status(worktree_path: &Path, fallback_branch: &str) -> Result<JsonGitStatus> {
    let repo = worktree_path
        .to_str()
        .context("worktree path contains invalid UTF-8")?;

    let raw = execute_git(&["-C", repo, "status", "--porcelain=2", "--branch"])?;

    let mut status = JsonGitStatus {
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

fn note_status_tokens(status: &mut JsonGitStatus, rest: &str) {
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

fn collect_head_commit(worktree_path: &Path) -> Result<Option<JsonCommitInfo>> {
    let repo = worktree_path
        .to_str()
        .context("worktree path contains invalid UTF-8")?;

    let args = ["-C", repo, "log", "-1", "--pretty=format:%H\x00%ct\x00%s"];

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

    Ok(Some(JsonCommitInfo {
        commit_id,
        summary,
        timestamp,
    }))
}
