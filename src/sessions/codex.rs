use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use walkdir::WalkDir;

use super::{SessionProvider, SessionRecord};

pub struct CodexSessionProvider {
    sessions_dir: Option<PathBuf>,
}

#[derive(Clone)]
struct CachedSession {
    modified: Option<SystemTime>,
    len: u64,
    record: SessionRecord,
}

#[derive(Default)]
struct SessionCache {
    entries: HashMap<PathBuf, CachedSession>,
}

impl SessionCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

static CODEX_SESSION_CACHE: OnceLock<Mutex<SessionCache>> = OnceLock::new();

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
                                record.last_user_message = Some(text.clone());
                                if record
                                    .user_messages
                                    .last()
                                    .map_or(true, |previous| previous != &text)
                                {
                                    record.user_messages.push(text);
                                }
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
                                if record
                                    .user_messages
                                    .last()
                                    .map_or(true, |previous| previous != msg)
                                {
                                    record.user_messages.push(msg.to_string());
                                }
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

        let cache_lock = CODEX_SESSION_CACHE.get_or_init(|| Mutex::new(SessionCache::new()));
        let cache = cache_lock
            .lock()
            .expect("codex session cache mutex poisoned");

        let mut seen_paths: HashSet<PathBuf> = HashSet::new();
        let mut refresh_list: Vec<(PathBuf, Option<SystemTime>, u64)> = Vec::new();

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
                let path_buf = path.to_path_buf();
                seen_paths.insert(path_buf.clone());
                let metadata = entry.metadata().ok();
                let modified = metadata.as_ref().and_then(|meta| meta.modified().ok());
                let len = metadata.map(|meta| meta.len()).unwrap_or(0);

                let needs_refresh = match cache.entries.get(&path_buf) {
                    Some(existing) => existing.modified != modified || existing.len != len,
                    None => true,
                };

                if needs_refresh {
                    refresh_list.push((path_buf.clone(), modified, len));
                }
            }
        }

        let stale_paths: Vec<PathBuf> = cache
            .entries
            .keys()
            .filter(|path| !seen_paths.contains(*path))
            .cloned()
            .collect();

        drop(cache);

        let mut refreshed: Vec<(PathBuf, Option<SystemTime>, u64, Option<SessionRecord>)> =
            Vec::with_capacity(refresh_list.len());
        for (path_buf, modified, len) in refresh_list {
            let record = self.parse_session_file(&path_buf);
            refreshed.push((path_buf, modified, len, record));
        }

        let mut cache = cache_lock
            .lock()
            .expect("codex session cache mutex poisoned");

        for stale in stale_paths {
            cache.entries.remove(&stale);
        }

        for (path_buf, modified, len, record_opt) in refreshed {
            match record_opt {
                Some(record) => {
                    cache.entries.insert(
                        path_buf,
                        CachedSession {
                            modified,
                            len,
                            record,
                        },
                    );
                }
                None => {
                    cache.entries.remove(&path_buf);
                }
            }
        }

        let mut sessions: Vec<SessionRecord> = cache
            .entries
            .values()
            .map(|entry| entry.record.clone())
            .collect();

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
