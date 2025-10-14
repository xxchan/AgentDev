use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Utc};
use colored::Colorize;
use serde::Serialize;

use crate::sessions::{SessionRecord, canonicalize, default_providers};
use crate::state::{WorktreeInfo, XlaudeState};

#[derive(Debug, Serialize)]
struct JsonSession {
    provider: String,
    session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    worktree_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    worktree_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    worktree_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    working_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    originator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_user_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_user_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_timestamp: Option<String>,
    file_path: String,
}

#[derive(Debug)]
struct SessionWithWorktree {
    record: SessionRecord,
    worktree_key: Option<String>,
    worktree_name: Option<String>,
    worktree_path: Option<PathBuf>,
}

pub fn handle_sessions_list(worktree: Option<String>, all: bool, json: bool) -> Result<()> {
    let state = XlaudeState::load()?;

    let worktree_entries = build_worktree_index(&state);
    let worktree_filter = worktree.as_deref();

    let mut sessions: Vec<SessionWithWorktree> = Vec::new();
    for provider in default_providers() {
        match provider.list_sessions() {
            Ok(records) => {
                for record in records {
                    let matched = record
                        .working_dir
                        .as_ref()
                        .and_then(|path| match_worktree(path, &worktree_entries));

                    let (worktree_key, worktree_name, worktree_path) = match matched {
                        Some((key, info)) => (
                            Some(key.clone()),
                            Some(info.name.clone()),
                            Some(info.path.clone()),
                        ),
                        None => (None, None, None),
                    };

                    sessions.push(SessionWithWorktree {
                        record,
                        worktree_key,
                        worktree_name,
                        worktree_path,
                    });
                }
            }
            Err(err) => {
                eprintln!("{} {}: {}", "[warn]".yellow(), provider.name(), err);
            }
        }
    }

    if !all {
        sessions.retain(|session| session.worktree_key.is_some());
    }

    if let Some(filter) = worktree_filter {
        sessions.retain(|session| {
            session
                .worktree_key
                .as_deref()
                .is_some_and(|key| key == filter)
                || session
                    .worktree_name
                    .as_deref()
                    .is_some_and(|name| name == filter)
        });
    }

    sessions.sort_by(|a, b| b.record.last_timestamp.cmp(&a.record.last_timestamp));

    if json {
        let payload = serde_json::to_string_pretty(&build_json_output(&sessions))?;
        println!("{payload}");
    } else {
        print_human_readable(&sessions);
    }

    Ok(())
}

fn build_worktree_index(state: &XlaudeState) -> Vec<(String, WorktreeInfo, Option<PathBuf>)> {
    state
        .worktrees
        .iter()
        .map(|(key, info)| {
            let canonical = canonicalize(&info.path);
            (key.clone(), info.clone(), canonical)
        })
        .collect()
}

fn match_worktree<'a>(
    path: &Path,
    worktrees: &'a [(String, WorktreeInfo, Option<PathBuf>)],
) -> Option<(&'a String, &'a WorktreeInfo)> {
    let candidate = canonicalize(path).unwrap_or_else(|| path.to_path_buf());

    worktrees.iter().find_map(|(key, info, canonical)| {
        if let Some(canonical_path) = canonical {
            if candidate.starts_with(canonical_path) {
                return Some((key, info));
            }
        } else if candidate == info.path {
            return Some((key, info));
        }
        None
    })
}

fn build_json_output(sessions: &[SessionWithWorktree]) -> HashMap<&'static str, Vec<JsonSession>> {
    let mut map: HashMap<&'static str, Vec<JsonSession>> = HashMap::new();

    for session in sessions {
        let entry = map.entry("sessions").or_default();
        entry.push(JsonSession {
            provider: session.record.provider.clone(),
            session_id: session.record.id.clone(),
            worktree_key: session.worktree_key.clone(),
            worktree_name: session.worktree_name.clone(),
            worktree_path: session
                .worktree_path
                .as_ref()
                .map(|p| p.display().to_string()),
            working_dir: session
                .record
                .working_dir
                .as_ref()
                .map(|p| p.display().to_string()),
            originator: session.record.originator.clone(),
            instructions: session.record.instructions.clone(),
            first_user_message: session.record.first_user_message.clone(),
            last_user_message: session.record.last_user_message.clone(),
            last_timestamp: session.record.last_timestamp.map(|ts| ts.to_rfc3339()),
            file_path: session.record.file_path.display().to_string(),
        });
    }

    map
}

fn print_human_readable(sessions: &[SessionWithWorktree]) {
    if sessions.is_empty() {
        println!("{} No sessions found", "📭".yellow());
        return;
    }

    println!("{} Sessions:", "🗂".cyan());
    println!();

    for session in sessions {
        let provider = session.record.provider.as_str();
        let title = session
            .record
            .first_user_message
            .as_deref()
            .unwrap_or("(no user messages)");
        let last_ts = session
            .record
            .last_timestamp
            .map(|ts| format!("{} ({})", ts.to_rfc3339(), format_relative(ts)))
            .unwrap_or_else(|| "unknown".to_string());

        let worktree_label = session
            .worktree_name
            .as_deref()
            .or(session.worktree_key.as_deref())
            .unwrap_or("unmapped");

        println!(
            "  {} {} {}",
            "•".green(),
            provider.bold(),
            worktree_label.cyan()
        );
        println!("    {} {}", "Session:".bright_black(), session.record.id);
        println!("    {} {}", "Last activity:".bright_black(), last_ts);
        if let Some(path) = session
            .record
            .working_dir
            .as_ref()
            .map(|p| p.display().to_string())
        {
            println!("    {} {}", "Dir:".bright_black(), path);
        }
        println!("    {} {}", "Summary:".bright_black(), truncate(title, 100));
        println!();
    }
}

fn format_relative(ts: DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = now.signed_duration_since(ts);

    if diff.num_minutes() < 1 {
        "just now".to_string()
    } else if diff.num_minutes() < 60 {
        format!("{}m ago", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}h ago", diff.num_hours())
    } else if diff.num_days() < 30 {
        format!("{}d ago", diff.num_days())
    } else {
        format!("{}mo ago", diff.num_days() / 30)
    }
}

fn truncate(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let mut acc = String::with_capacity(limit + 3);
    for ch in text.chars() {
        if acc.len() + ch.len_utf8() > limit {
            break;
        }
        acc.push(ch);
    }
    acc.push_str("...");
    acc
}
