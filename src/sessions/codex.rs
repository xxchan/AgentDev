use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use walkdir::WalkDir;

use super::{SessionProvider, SessionRecord};

pub struct CodexSessionProvider {
    sessions_dir: Option<PathBuf>,
}

impl CodexSessionProvider {
    pub fn new() -> Self {
        let sessions_dir = std::env::var("HOME")
            .ok()
            .map(|home| PathBuf::from(home).join(".codex").join("sessions"));
        Self { sessions_dir }
    }

    fn parse_session_file(&self, path: &Path) -> Option<SessionRecord> {
        let file = File::open(path).ok()?;
        let reader = BufReader::new(file);

        let mut record = SessionRecord::new(self.name(), path.to_path_buf());

        for line in reader.lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }

            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };

            if let Some(ts_str) = value.get("timestamp").and_then(|v| v.as_str()) {
                if let Ok(parsed) = DateTime::parse_from_rfc3339(ts_str) {
                    record.last_timestamp = Some(parsed.with_timezone(&Utc));
                }
            }

            match value.get("type").and_then(|v| v.as_str()) {
                Some("session_meta") => {
                    if let Some(payload) = value.get("payload") {
                        if let Some(id) = payload.get("id").and_then(|v| v.as_str()) {
                            record.id = id.to_string();
                        }
                        if let Some(cwd) = payload.get("cwd").and_then(|v| v.as_str()) {
                            record.set_working_dir(cwd);
                        }
                        if let Some(originator) = payload.get("originator").and_then(|v| v.as_str())
                        {
                            record.originator = Some(originator.to_string());
                        }
                        if let Some(instr) = payload.get("instructions").and_then(|v| v.as_str()) {
                            record.instructions = Some(instr.to_string());
                        }
                    }
                }
                Some("response_item") => {
                    if let Some(payload) = value.get("payload") {
                        let role_is_user = payload
                            .get("role")
                            .and_then(|v| v.as_str())
                            .map(|role| role.eq_ignore_ascii_case("user"))
                            .unwrap_or(false);
                        if role_is_user {
                            if let Some(text) = extract_message_text(payload) {
                                if record.first_user_message.is_none() {
                                    record.first_user_message = Some(text.clone());
                                }
                                record.last_user_message = Some(text);
                            }
                        }
                    }
                }
                Some("event_msg") => {
                    if let Some(payload) = value.get("payload") {
                        if payload
                            .get("type")
                            .and_then(|v| v.as_str())
                            .map(|kind| kind.eq_ignore_ascii_case("user_message"))
                            .unwrap_or(false)
                        {
                            if let Some(msg) = payload.get("message").and_then(|v| v.as_str()) {
                                if record.first_user_message.is_none() {
                                    record.first_user_message = Some(msg.to_string());
                                }
                                record.last_user_message = Some(msg.to_string());
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Some(record)
    }
}

impl SessionProvider for CodexSessionProvider {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn list_sessions(&self) -> Result<Vec<SessionRecord>> {
        let Some(dir) = &self.sessions_dir else {
            return Ok(Vec::new());
        };

        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();

        for entry in WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"))
            {
                if let Some(record) = self.parse_session_file(path) {
                    sessions.push(record);
                }
            }
        }

        sessions.sort_by(|a, b| b.last_timestamp.cmp(&a.last_timestamp));
        Ok(sessions)
    }
}

fn extract_message_text(payload: &Value) -> Option<String> {
    if let Some(content) = payload.get("content") {
        if let Some(items) = content.as_array() {
            let mut pieces = Vec::new();
            for item in items {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    if !text.trim().is_empty() {
                        pieces.push(text.trim());
                    }
                }
            }
            if !pieces.is_empty() {
                return Some(pieces.join(" "));
            }
        }
    }
    None
}
