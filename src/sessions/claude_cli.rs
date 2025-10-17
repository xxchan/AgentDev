use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{
    SessionEvent, SessionProvider, SessionRecord, SessionToolEvent, SessionToolPhase, canonicalize,
};

pub struct ClaudeCliSessionProvider {
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

static CLAUDE_SESSION_CACHE: OnceLock<Mutex<SessionCache>> = OnceLock::new();

/// TODO(provider-models): promote this raw struct into a discriminated enum so higher layers can pattern-match on structured variants.
#[derive(Debug, Deserialize, Serialize)]
struct ClaudeRawEntry {
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(rename = "type", default)]
    entry_type: Option<String>,
    #[serde(default)]
    message: Option<ClaudeRawMessage>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ClaudeRawMessage {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    content: Option<Value>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Debug, Clone)]
enum ClaudeEntryCategory {
    User,
    Assistant,
    System,
    Generic,
}

#[derive(Debug)]
struct ClaudeParsedEntry {
    raw: Value,
    data: ClaudeRawEntry,
}

impl ClaudeParsedEntry {
    fn parse(raw: Value) -> Option<Self> {
        let data = serde_json::from_value::<ClaudeRawEntry>(raw.clone()).ok()?;
        Some(Self { raw, data })
    }

    fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.data
            .timestamp
            .as_deref()
            .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
            .map(|dt| dt.with_timezone(&Utc))
    }

    fn cwd(&self) -> Option<&str> {
        self.data
            .cwd
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    fn category_with_actor(&self) -> (String, Option<String>, ClaudeEntryCategory) {
        let actor = self
            .data
            .message
            .as_ref()
            .and_then(|message| message.role.clone())
            .or_else(|| self.data.entry_type.clone());

        let category = self
            .data
            .entry_type
            .clone()
            .or_else(|| actor.clone())
            .unwrap_or_else(|| "message".to_string());

        let entry_category = match actor.as_deref() {
            Some("user") => ClaudeEntryCategory::User,
            Some("assistant") => ClaudeEntryCategory::Assistant,
            Some("system") => ClaudeEntryCategory::System,
            _ => ClaudeEntryCategory::Generic,
        };

        (category, actor, entry_category)
    }

    fn apply_summary(&self, record: &mut SessionRecord, entry_category: &ClaudeEntryCategory) {
        if matches!(entry_category, ClaudeEntryCategory::System) && record.instructions.is_none() {
            if let Some(text) = self.message_text() {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    record.instructions = Some(trimmed.to_string());
                }
            }
        }
    }

    fn message_text(&self) -> Option<String> {
        extract_message_text(&self.raw)
    }

    fn working_dir_value(&self) -> Option<Value> {
        self.cwd()
            .map(|cwd| Value::String(cwd.to_string()))
            .or_else(|| self.data.extra.get("cwd").cloned())
    }

    fn to_event(&self, include_raw: bool) -> Option<SessionEvent> {
        let (category, actor, entry_category) = self.category_with_actor();

        let text = self
            .message_text()
            .or_else(|| {
                self.data
                    .message
                    .as_ref()
                    .and_then(|message| message.text.clone())
            })
            .unwrap_or_else(|| pretty_json(&self.raw));

        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }

        let summary_text = if matches!(entry_category, ClaudeEntryCategory::User) {
            normalize_user_message(trimmed)
        } else {
            None
        };

        let working_dir_hint = self.cwd().map(|value| value.to_string());

        let mut data_map = serde_json::Map::new();
        if let Some(cwd) = self.working_dir_value() {
            data_map.insert("working_dir".to_string(), cwd);
        }
        if let Some(id) = self.data.id.clone() {
            data_map.insert("id".to_string(), id);
        }
        let data = if data_map.is_empty() {
            None
        } else {
            Some(Value::Object(data_map))
        };

        let mut tool = extract_claude_tool_event(&self.raw, working_dir_hint.as_deref());
        if let Some(tool_event) = tool.as_mut() {
            if tool_event.working_dir.is_none() {
                if let Some(dir) = working_dir_hint.as_ref() {
                    tool_event.working_dir = Some(dir.clone());
                }
            }
        }

        let mut category_value = category;
        let mut label = actor
            .as_ref()
            .map(|value| to_title_case(value))
            .or_else(|| Some(to_title_case(&category_value)));

        if let Some(tool_event) = tool.as_ref() {
            category_value = match tool_event.phase {
                SessionToolPhase::Use => "tool_use".to_string(),
                SessionToolPhase::Result => "tool_result".to_string(),
            };
            label = Some(tool_label(tool_event));
        }

        let raw = if include_raw {
            Some(self.raw.clone())
        } else {
            None
        };

        Some(SessionEvent {
            actor,
            category: category_value,
            label,
            text: Some(trimmed.to_string()),
            summary_text,
            data,
            timestamp: self.timestamp(),
            raw,
            tool,
        })
    }
}

impl ClaudeCliSessionProvider {
    pub fn new() -> Self {
        let sessions_dir = std::env::var("HOME")
            .ok()
            .map(|home| PathBuf::from(home).join(".claude").join("projects"));
        Self { sessions_dir }
    }

    pub fn list_sessions_for_path(&self, project_path: &Path) -> Result<Vec<SessionRecord>> {
        let canonical_target =
            canonicalize(project_path).unwrap_or_else(|| project_path.to_path_buf());
        let sessions = self.list_sessions()?;
        Ok(sessions
            .into_iter()
            .filter(|record| {
                record
                    .working_dir
                    .as_ref()
                    .map(|dir| dir.starts_with(&canonical_target))
                    .unwrap_or(false)
            })
            .collect())
    }

    fn sessions_root(&self) -> Option<&Path> {
        self.sessions_dir.as_deref()
    }

    fn parse_session_file(&self, path: &Path) -> Option<SessionRecord> {
        let file = File::open(path).ok()?;
        let reader = BufReader::new(file);

        let mut record = SessionRecord::new(self.name(), path.to_path_buf());

        for line in reader.lines().map_while(Result::ok) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let Ok(raw) = serde_json::from_str::<Value>(trimmed) else {
                continue;
            };

            let Some(entry) = ClaudeParsedEntry::parse(raw) else {
                continue;
            };
            if record.working_dir.is_none() {
                if let Some(cwd) = entry.cwd() {
                    let path = Path::new(cwd);
                    if let Some(canonical) = canonicalize(path) {
                        record.working_dir = Some(canonical);
                    } else {
                        record.set_working_dir(cwd);
                    }
                }
            }

            let (_, _, entry_category) = entry.category_with_actor();
            entry.apply_summary(&mut record, &entry_category);

            if let Some(event) = entry.to_event(false) {
                record.ingest_event(&event);
            }
        }

        if record.user_messages.is_empty() {
            return None;
        }

        Some(record)
    }
}

impl SessionProvider for ClaudeCliSessionProvider {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn list_sessions(&self) -> Result<Vec<SessionRecord>> {
        let Some(root) = self.sessions_root() else {
            return Ok(Vec::new());
        };

        if !root.exists() {
            return Ok(Vec::new());
        }

        let cache_lock = CLAUDE_SESSION_CACHE.get_or_init(|| Mutex::new(SessionCache::new()));
        let cache = cache_lock
            .lock()
            .expect("claude cli session cache mutex poisoned");

        let mut seen_paths: HashSet<PathBuf> = HashSet::new();
        let mut refresh_list: Vec<(PathBuf, Option<SystemTime>, u64)> = Vec::new();

        if let Ok(projects) = fs::read_dir(root) {
            for project_entry in projects.flatten() {
                let project_path = project_entry.path();
                if !project_path.is_dir() {
                    continue;
                }

                if let Ok(files) = fs::read_dir(&project_path) {
                    for file_entry in files.flatten() {
                        let file_path = file_entry.path();
                        if !file_path.is_file()
                            || file_path
                                .extension()
                                .and_then(|ext| ext.to_str())
                                .map(|ext| !ext.eq_ignore_ascii_case("jsonl"))
                                .unwrap_or(true)
                        {
                            continue;
                        }

                        let path_buf = file_path;
                        seen_paths.insert(path_buf.clone());

                        let metadata = file_entry.metadata().ok();
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
            .expect("claude cli session cache mutex poisoned");

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

    fn load_session_events(&self, record: &SessionRecord) -> Result<Vec<SessionEvent>> {
        let file = File::open(&record.file_path)?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();

        for line in reader.lines() {
            let trimmed = match line {
                Ok(value) => value.trim().to_string(),
                Err(_) => continue,
            };
            if trimmed.is_empty() {
                continue;
            }

            let Ok(raw) = serde_json::from_str::<Value>(&trimmed) else {
                continue;
            };

            let Some(entry) = ClaudeParsedEntry::parse(raw) else {
                continue;
            };

            if let Some(event) = entry.to_event(true) {
                events.push(event);
            }
        }

        Ok(events)
    }
}

fn extract_message_text(value: &Value) -> Option<String> {
    let message = value.get("message")?;
    if let Some(content) = message.get("content") {
        if let Some(text) = format_message_content(content) {
            return Some(text);
        }
    }
    message
        .get("content")
        .and_then(format_message_content)
        .or_else(|| {
            message
                .get("text")
                .and_then(|entry| entry.as_str())
                .map(|entry| entry.trim().to_string())
        })
}

fn format_message_content(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Array(items) => {
            let mut parts: Vec<String> = Vec::new();
            for item in items {
                if let Some(item_type) = item.get("type").and_then(|entry| entry.as_str()) {
                    match item_type {
                        "text" => {
                            if let Some(text) = item.get("text").and_then(|entry| entry.as_str()) {
                                let trimmed = text.trim();
                                if !trimmed.is_empty() {
                                    parts.push(trimmed.to_string());
                                }
                            }
                        }
                        "thinking" => {
                            // Skip hidden chain-of-thought content
                        }
                        "tool_use" => {
                            let name = item
                                .get("name")
                                .and_then(|entry| entry.as_str())
                                .unwrap_or("tool");
                            let mut formatted = format!("Tool call: {name}");
                            if let Some(input) = item.get("input") {
                                let serialized = pretty_json(input);
                                if !serialized.trim().is_empty() {
                                    formatted.push_str("\n");
                                    formatted.push_str(&serialized);
                                }
                            }
                            parts.push(formatted);
                        }
                        "tool_result" => {
                            if let Some(text) = item.get("text").and_then(|entry| entry.as_str()) {
                                let trimmed = text.trim();
                                if !trimmed.is_empty() {
                                    parts.push(trimmed.to_string());
                                }
                            } else if let Some(content) = item.get("content") {
                                parts.push(pretty_json(content));
                            }
                        }
                        _ => {
                            if let Some(text) = item.get("text").and_then(|entry| entry.as_str()) {
                                let trimmed = text.trim();
                                if !trimmed.is_empty() {
                                    parts.push(trimmed.to_string());
                                }
                            } else if let Some(content) = item.get("content") {
                                if let Some(nested) = format_message_content(content) {
                                    parts.push(nested);
                                }
                            } else {
                                parts.push(pretty_json(item));
                            }
                        }
                    }
                } else if let Some(text) = item.get("text").and_then(|entry| entry.as_str()) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        parts.push(trimmed.to_string());
                    }
                }
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n\n"))
            }
        }
        Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(|entry| entry.as_str()) {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            } else if let Some(content) = map.get("content") {
                format_message_content(content)
            } else {
                Some(pretty_json(&Value::Object(map.clone())))
            }
        }
        _ => None,
    }
}

fn extract_claude_tool_event(raw: &Value, working_dir: Option<&str>) -> Option<SessionToolEvent> {
    let message = raw.get("message")?.as_object()?;
    let content = message.get("content")?;
    let items = content.as_array()?;

    let mut candidate: Option<(&Map<String, Value>, SessionToolPhase)> = None;
    for item in items {
        let obj = item.as_object()?;
        let item_type = obj.get("type").and_then(|value| value.as_str())?;
        match item_type {
            "tool_use" => {
                candidate = Some((obj, SessionToolPhase::Use));
                break;
            }
            "tool_result" => {
                if candidate.is_none() {
                    candidate = Some((obj, SessionToolPhase::Result));
                }
            }
            _ => {}
        }
    }

    let (obj, phase) = candidate?;

    let mut extras = Map::new();
    for (key, value) in obj.iter() {
        if matches!(
            key.as_str(),
            "type"
                | "id"
                | "tool_use_id"
                | "name"
                | "input"
                | "output"
                | "result"
                | "content"
                | "text"
        ) {
            continue;
        }
        extras.insert(key.clone(), value.clone());
    }

    let mut working_dir_value = working_dir.map(|value| value.to_string());
    if working_dir_value.is_none() {
        if let Some(dir) = obj
            .get("working_dir")
            .or_else(|| obj.get("cwd"))
            .and_then(|value| value.as_str())
        {
            working_dir_value = Some(dir.to_string());
        }
    }

    let input = obj.get("input").cloned();
    let output = match phase {
        SessionToolPhase::Use => None,
        SessionToolPhase::Result => obj
            .get("output")
            .or_else(|| obj.get("result"))
            .or_else(|| obj.get("content"))
            .or_else(|| obj.get("text"))
            .cloned(),
    };

    let identifier = obj
        .get("id")
        .or_else(|| obj.get("tool_use_id"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());

    Some(SessionToolEvent {
        phase,
        name: obj
            .get("name")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string()),
        identifier,
        input,
        output,
        working_dir: working_dir_value,
        extras,
    })
}

fn tool_label(tool: &SessionToolEvent) -> String {
    let phase = match tool.phase {
        SessionToolPhase::Use => "Tool Use",
        SessionToolPhase::Result => "Tool Result",
    };

    if let Some(name) = tool.name.as_ref().filter(|value| !value.is_empty()) {
        format!("{phase} · {name}")
    } else {
        phase.to_string()
    }
}

fn normalize_user_message(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("<local-command")
        || trimmed.starts_with("<command-")
        || trimmed.starts_with("Caveat:")
        || trimmed.contains("[Request interrupted")
    {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn to_title_case(value: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for segment in value.split(|c: char| c == '_' || c == '-' || c == ' ') {
        if segment.is_empty() {
            continue;
        }
        let mut chars = segment.chars();
        if let Some(first) = chars.next() {
            let mut part = String::new();
            part.extend(first.to_uppercase());
            part.extend(chars.flat_map(|ch| ch.to_lowercase()));
            parts.push(part);
        }
    }

    if parts.is_empty() {
        value.to_string()
    } else {
        parts.join(" ")
    }
}

#[cfg(test)]
mod tests;
