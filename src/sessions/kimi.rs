use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{SessionEvent, SessionProvider, SessionRecord};

pub struct KimiSessionProvider {
    sessions_dir: Option<PathBuf>,
    workdir_index: HashMap<String, PathBuf>,
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

static KIMI_SESSION_CACHE: OnceLock<Mutex<SessionCache>> = OnceLock::new();

/// TODO(provider-models): convert this raw struct into an enum-based event model to enforce variant coverage.
#[derive(Debug, Deserialize, Serialize)]
struct KimiRawEntry {
    #[serde(default)]
    role: Option<String>,
    #[serde(rename = "type", default)]
    entry_type: Option<String>,
    #[serde(default)]
    content: Option<Value>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    function_call: Option<Value>,
    #[serde(default)]
    arguments: Option<Value>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    tool_calls: Option<Value>,
    #[serde(default)]
    token_count: Option<u64>,
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
    #[serde(default)]
    id: Option<i64>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Debug, Clone)]
enum KimiEntryCategory {
    Checkpoint,
    Usage,
    Message,
    Generic,
}

#[derive(Debug)]
struct KimiParsedEntry {
    raw: Value,
    data: KimiRawEntry,
}

impl KimiParsedEntry {
    fn parse(raw: Value) -> Option<Self> {
        let data = serde_json::from_value::<KimiRawEntry>(raw.clone()).ok()?;
        Some(Self { raw, data })
    }

    fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.data
            .timestamp
            .as_deref()
            .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
            .map(|dt| dt.with_timezone(&Utc))
    }

    fn category_with_actor(&self) -> (String, Option<String>, KimiEntryCategory) {
        let mut category = self
            .data
            .entry_type
            .clone()
            .or_else(|| self.data.role.clone())
            .unwrap_or_else(|| "message".to_string());

        let actor = self.data.role.clone();

        let entry_category = match actor.as_deref() {
            Some("_checkpoint") => {
                category = "_checkpoint".to_string();
                KimiEntryCategory::Checkpoint
            }
            Some("_usage") => {
                category = "_usage".to_string();
                KimiEntryCategory::Usage
            }
            _ => match category.as_str() {
                "_checkpoint" => KimiEntryCategory::Checkpoint,
                "_usage" => KimiEntryCategory::Usage,
                "user" | "assistant" | "system" => KimiEntryCategory::Message,
                _ => KimiEntryCategory::Generic,
            },
        };

        (category, actor, entry_category)
    }

    fn apply_summary(&self, _record: &mut SessionRecord, _entry_category: &KimiEntryCategory) {}

    fn to_event(&self, include_raw: bool) -> Option<SessionEvent> {
        let (category, actor, entry_category) = self.category_with_actor();
        let text = kimi_format_entry(&self.raw, actor.as_deref())
            .unwrap_or_else(|| kimi_pretty_json(&self.raw));

        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }

        let label = actor
            .as_ref()
            .map(|value| to_title_case(value))
            .or_else(|| Some(to_title_case(&category)));

        let summary_text = match entry_category {
            KimiEntryCategory::Message => {
                if actor.as_deref() == Some("user") {
                    Some(trimmed.to_string())
                } else {
                    None
                }
            }
            _ => None,
        };

        let timestamp = self.timestamp();
        let raw = if include_raw {
            Some(self.raw.clone())
        } else {
            None
        };

        Some(SessionEvent {
            actor,
            category,
            label,
            text: Some(trimmed.to_string()),
            summary_text,
            data: serde_json::to_value(&self.data).ok(),
            timestamp,
            raw,
        })
    }
}

#[derive(Debug, Default, Deserialize)]
struct KimiConfig {
    #[serde(default)]
    work_dirs: Vec<KimiWorkDir>,
}

#[derive(Debug, Default, Deserialize)]
struct KimiWorkDir {
    path: String,
}

impl KimiSessionProvider {
    pub fn new() -> Self {
        let home = std::env::var("HOME").ok();

        let (sessions_dir, workdir_index) = if let Some(home) = home {
            let base = PathBuf::from(&home)
                .join(".local")
                .join("share")
                .join("kimi");
            let sessions_dir = base.join("sessions");
            let config_path = base.join("kimi.json");
            let workdir_index = Self::load_workdirs(&config_path);
            (Some(sessions_dir), workdir_index)
        } else {
            (None, HashMap::new())
        };

        Self {
            sessions_dir,
            workdir_index,
        }
    }

    fn load_workdirs(config_path: &Path) -> HashMap<String, PathBuf> {
        let mut map = HashMap::new();

        let Ok(file) = File::open(config_path) else {
            return map;
        };
        let reader = BufReader::new(file);

        let Ok(config) = serde_json::from_reader::<_, KimiConfig>(reader) else {
            return map;
        };

        for entry in config.work_dirs {
            let hash = format!("{:x}", md5::compute(entry.path.as_bytes()));
            map.insert(hash, PathBuf::from(entry.path));
        }

        map
    }

    fn resolve_working_dir<'a>(&'a self, session_file: &Path) -> Option<&'a Path> {
        session_file
            .parent()
            .and_then(|dir| dir.file_name())
            .and_then(|name| name.to_str())
            .and_then(|hash| self.workdir_index.get(hash))
            .map(PathBuf::as_path)
    }

    fn parse_session_file(&self, path: &Path, working_dir: Option<&Path>) -> Option<SessionRecord> {
        let file = File::open(path).ok()?;
        let reader = BufReader::new(file);

        let mut record = SessionRecord::new(self.name(), path.to_path_buf());
        if let Some(dir) = working_dir {
            record.working_dir = Some(dir.to_path_buf());
        }

        for line in reader.lines().map_while(Result::ok) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let Ok(raw) = serde_json::from_str::<Value>(trimmed) else {
                continue;
            };

            let Some(entry) = KimiParsedEntry::parse(raw) else {
                continue;
            };
            let (_, _, entry_category) = entry.category_with_actor();
            entry.apply_summary(&mut record, &entry_category);

            if let Some(event) = entry.to_event(false) {
                record.ingest_event(&event);
            }
        }

        if record.user_messages.is_empty() {
            return None;
        }

        if record.last_timestamp.is_none() {
            if let Ok(metadata) = fs::metadata(path) {
                if let Ok(modified) = metadata.modified() {
                    record.last_timestamp = Some(DateTime::<Utc>::from(modified));
                }
            }
        }

        Some(record)
    }
}

impl SessionProvider for KimiSessionProvider {
    fn name(&self) -> &'static str {
        "kimi"
    }

    fn list_sessions(&self) -> Result<Vec<SessionRecord>> {
        let Some(root) = &self.sessions_dir else {
            return Ok(Vec::new());
        };

        if !root.exists() {
            return Ok(Vec::new());
        }

        let cache_lock = KIMI_SESSION_CACHE.get_or_init(|| Mutex::new(SessionCache::new()));
        let cache = cache_lock
            .lock()
            .expect("kimi session cache mutex poisoned");

        let mut seen_paths: HashSet<PathBuf> = HashSet::new();
        let mut refresh_list: Vec<(PathBuf, Option<SystemTime>, u64)> = Vec::new();

        for entry in fs::read_dir(root)? {
            let entry = match entry {
                Ok(value) => value,
                Err(_) => continue,
            };

            let dir_path = entry.path();
            if !dir_path.is_dir() {
                continue;
            }

            for file_entry in fs::read_dir(&dir_path)? {
                let file_entry = match file_entry {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                let file_path = file_entry.path();
                if !file_path.is_file() {
                    continue;
                }
                if file_path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"))
                {
                    let metadata = file_entry.metadata().ok();
                    let modified = metadata.as_ref().and_then(|meta| meta.modified().ok());
                    let len = metadata.map(|meta| meta.len()).unwrap_or(0);
                    let path_buf = file_path;

                    seen_paths.insert(path_buf.clone());

                    let needs_refresh = match cache.entries.get(&path_buf) {
                        Some(existing) => existing.modified != modified || existing.len != len,
                        None => true,
                    };

                    if needs_refresh {
                        refresh_list.push((path_buf, modified, len));
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
            let working_dir = self.resolve_working_dir(&path_buf);
            let record = self.parse_session_file(&path_buf, working_dir);
            refreshed.push((path_buf, modified, len, record));
        }

        let mut cache = cache_lock
            .lock()
            .expect("kimi session cache mutex poisoned");

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

            let raw: Value = match serde_json::from_str(&trimmed) {
                Ok(parsed) => parsed,
                Err(_) => continue,
            };

            let Some(entry) = KimiParsedEntry::parse(raw) else {
                continue;
            };

            if let Some(event) = entry.to_event(true) {
                events.push(event);
            }
        }

        Ok(events)
    }
}

fn kimi_format_entry(value: &Value, role: Option<&str>) -> Option<String> {
    match role {
        Some("_checkpoint") => {
            let id = value
                .get("id")
                .and_then(|entry| entry.as_i64())
                .map(|entry| entry.to_string())
                .unwrap_or_else(|| "?".to_string());
            Some(format!("Checkpoint {id}"))
        }
        Some("_usage") => kimi_format_usage(value),
        _ => {
            if let Some(content) = value.get("content") {
                if let Some(formatted) = kimi_format_content(content) {
                    return Some(formatted);
                }
            }
            if let Some(message) = value
                .get("message")
                .and_then(|entry| entry.as_str())
                .map(str::trim)
            {
                if !message.is_empty() {
                    return Some(message.to_string());
                }
            }
            if let Some(function_call) = value.get("function_call") {
                return Some(format!(
                    "Function call:\n{}",
                    kimi_pretty_json(function_call)
                ));
            }
            if let Some(arguments) = value.get("arguments") {
                if let Some(name) = value.get("name").and_then(|entry| entry.as_str()) {
                    return Some(format!(
                        "Function {name} arguments:\n{}",
                        kimi_pretty_json(arguments)
                    ));
                }
            }
            if let Some(tool_calls) = value.get("tool_calls") {
                return Some(format!("Tool calls:\n{}", kimi_pretty_json(tool_calls)));
            }
            None
        }
    }
}

fn kimi_format_usage(value: &Value) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(total) = value.get("token_count").and_then(|entry| entry.as_u64()) {
        parts.push(format!("Total tokens: {total}"));
    }
    if let Some(input) = value.get("input_tokens").and_then(|entry| entry.as_u64()) {
        parts.push(format!("Input tokens: {input}"));
    }
    if let Some(output) = value.get("output_tokens").and_then(|entry| entry.as_u64()) {
        parts.push(format!("Output tokens: {output}"));
    }
    if parts.is_empty() {
        Some(kimi_pretty_json(value))
    } else {
        Some(parts.join("\n"))
    }
}

fn kimi_format_content(content: &Value) -> Option<String> {
    match content {
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
                match item {
                    Value::String(text) => {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            parts.push(trimmed.to_string());
                        }
                    }
                    Value::Object(map) => {
                        if let Some(text) = map
                            .get("text")
                            .and_then(|entry| entry.as_str())
                            .map(str::trim)
                        {
                            if !text.is_empty() {
                                parts.push(text.to_string());
                                continue;
                            }
                        }
                        if let Some(url) = map
                            .get("url")
                            .and_then(|entry| entry.as_str())
                            .map(str::trim)
                        {
                            if !url.is_empty() {
                                parts.push(format!("Attachment: {url}"));
                                continue;
                            }
                        }
                        if let Some(kind) = map.get("type").and_then(|entry| entry.as_str()) {
                            parts.push(format!("{kind}:\n{}", kimi_pretty_json(item)));
                        } else {
                            parts.push(kimi_pretty_json(item));
                        }
                    }
                    other => parts.push(kimi_pretty_json(other)),
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
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
            Some(kimi_pretty_json(content))
        }
        _ => Some(kimi_pretty_json(content)),
    }
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

fn kimi_pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;
    use serde_json::json;

    fn snapshot(event: &SessionEvent) -> String {
        serde_json::to_string_pretty(event).expect("serialize event")
    }

    #[test]
    fn tool_use_event_snapshot() {
        let raw = json!({
            "timestamp": "2025-01-02T03:04:05Z",
            "role": "assistant",
            "tool_calls": [
                {
                    "id": "call-1",
                    "type": "function",
                    "function": {
                        "name": "git_diff",
                        "arguments": "{\"paths\": [\"src/lib.rs\"], \"commit\": \"HEAD\"}"
                    }
                }
            ]
        });

        let entry = KimiParsedEntry::parse(raw).expect("parse kimi entry");
        let event = entry.to_event(false).expect("convert kimi tool event");

        expect![[r#"
            {
              "actor": "assistant",
              "category": "assistant",
              "label": "Assistant",
              "text": "Tool calls:\n[\n  {\n    \"function\": {\n      \"arguments\": \"{\\\"paths\\\": [\\\"src/lib.rs\\\"], \\\"commit\\\": \\\"HEAD\\\"}\",\n      \"name\": \"git_diff\"\n    },\n    \"id\": \"call-1\",\n    \"type\": \"function\"\n  }\n]",
              "data": {
                "arguments": null,
                "content": null,
                "function_call": null,
                "id": null,
                "input_tokens": null,
                "message": null,
                "name": null,
                "output_tokens": null,
                "role": "assistant",
                "timestamp": "2025-01-02T03:04:05Z",
                "token_count": null,
                "tool_calls": [
                  {
                    "function": {
                      "arguments": "{\"paths\": [\"src/lib.rs\"], \"commit\": \"HEAD\"}",
                      "name": "git_diff"
                    },
                    "id": "call-1",
                    "type": "function"
                  }
                ],
                "type": null
              },
              "timestamp": "2025-01-02T03:04:05Z"
            }"#]]
        .assert_eq(&snapshot(&event));
    }
}
