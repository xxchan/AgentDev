use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use walkdir::WalkDir;

use super::{SessionEvent, SessionProvider, SessionRecord, SessionToolEvent, SessionToolPhase};

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

/// TODO(provider-models): consider promoting this raw struct into a typed enum so callers can match variants explicitly.
#[derive(Debug, Deserialize, Serialize)]
struct CodexRawEntry {
    #[serde(rename = "type", default)]
    entry_type: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    originator: Option<String>,
    #[serde(default)]
    instructions: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    payload: Option<CodexRawPayload>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

/// TODO(provider-models): same as above—eventually migrate into strongly typed payload variants.
#[derive(Debug, Deserialize, Serialize)]
struct CodexRawPayload {
    #[serde(rename = "type", default)]
    payload_type: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    content: Option<Value>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    originator: Option<String>,
    #[serde(default)]
    instructions: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Debug, Clone)]
enum CodexEntryCategory {
    SessionMeta,
    ResponseItem,
    UserMessage,
    AssistantMessage,
    Event(String),
}

#[derive(Debug)]
struct CodexParsedEntry {
    raw: Value,
    data: CodexRawEntry,
}

impl CodexParsedEntry {
    fn parse(raw: Value) -> Option<Self> {
        let data = serde_json::from_value::<CodexRawEntry>(raw.clone()).ok()?;
        Some(Self { raw, data })
    }

    fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.data
            .timestamp
            .as_deref()
            .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
            .map(|dt| dt.with_timezone(&Utc))
    }

    fn raw_payload(&self) -> Option<&Value> {
        self.raw
            .as_object()
            .and_then(|object| object.get("payload"))
    }

    fn payload_value(&self) -> Option<Value> {
        self.data
            .payload
            .as_ref()
            .and_then(|payload| serde_json::to_value(payload).ok())
    }

    fn category_with_actor(&self) -> (String, Option<String>, CodexEntryCategory) {
        let mut category = self
            .data
            .entry_type
            .as_ref()
            .or(self.data.kind.as_ref())
            .cloned()
            .unwrap_or_else(|| "message".to_string());

        let mut actor = self
            .data
            .payload
            .as_ref()
            .and_then(|payload| payload.role.clone())
            .or_else(|| self.data.role.clone());

        if actor.is_none() && category == "session_meta" {
            actor = Some("system".to_string());
        }

        if category == "event_msg" {
            if let Some(payload) = self.data.payload.as_ref() {
                if let Some(inner) = payload.payload_type.as_ref() {
                    category = inner.clone();
                    if actor.is_none() {
                        actor = match inner.as_str() {
                            "user_message" => Some("user".to_string()),
                            "assistant_message" => Some("assistant".to_string()),
                            _ => None,
                        };
                    }
                }
            }
        }

        let entry_category = match category.as_str() {
            "session_meta" => CodexEntryCategory::SessionMeta,
            "response_item" => CodexEntryCategory::ResponseItem,
            "user_message" => CodexEntryCategory::UserMessage,
            "assistant_message" => CodexEntryCategory::AssistantMessage,
            other => CodexEntryCategory::Event(other.to_string()),
        };

        (category, actor, entry_category)
    }

    fn apply_summary(&self, record: &mut SessionRecord, entry_category: &CodexEntryCategory) {
        match entry_category {
            CodexEntryCategory::SessionMeta => {
                if let Some(payload) = self.data.payload.as_ref() {
                    if let Some(id) = payload.id.as_ref().or(self.data.id.as_ref()) {
                        record.id = id.clone();
                    }
                    if record.originator.is_none() {
                        if let Some(originator) = payload
                            .originator
                            .as_ref()
                            .or(self.data.originator.as_ref())
                        {
                            record.originator = Some(originator.clone());
                        }
                    }
                    if record.instructions.is_none() {
                        if let Some(instructions) = payload
                            .instructions
                            .as_ref()
                            .or(self.data.instructions.as_ref())
                        {
                            let trimmed = instructions.trim();
                            if !trimmed.is_empty() {
                                record.instructions = Some(trimmed.to_string());
                            }
                        }
                    }
                    if record.working_dir.is_none() {
                        if let Some(cwd) = payload
                            .cwd
                            .as_deref()
                            .or_else(|| payload.extra.get("working_dir").and_then(|v| v.as_str()))
                        {
                            record.set_working_dir(cwd);
                        }
                    }
                } else if let Some(id) = self.data.id.as_ref() {
                    record.id = id.clone();
                }
            }
            _ => {
                if record.working_dir.is_none() {
                    if let Some(dir) = self.working_dir_hint() {
                        record.set_working_dir(&dir);
                    }
                }
            }
        }

        if let Some(id) = self.data.id.as_ref() {
            if id != &record.id {
                record.id = id.clone();
            }
        }

        if record.originator.is_none() {
            if let Some(originator) = self.data.originator.as_ref() {
                record.originator = Some(originator.clone());
            }
        }

        if record.instructions.is_none() {
            if let Some(instructions) = self.data.instructions.as_ref() {
                let trimmed = instructions.trim();
                if !trimmed.is_empty() {
                    record.instructions = Some(trimmed.to_string());
                }
            }
        }
    }

    fn working_dir_hint(&self) -> Option<String> {
        if let Some(payload) = self.data.payload.as_ref() {
            if let Some(cwd) = payload.cwd.as_ref() {
                return Some(cwd.clone());
            }
            if let Some(dir) = payload.extra.get("working_dir").and_then(|v| v.as_str()) {
                return Some(dir.to_string());
            }
        }
        if let Some(dir) = self.data.extra.get("cwd").and_then(|value| value.as_str()) {
            return Some(dir.to_string());
        }
        extract_working_dir_from_message(&self.raw)
    }

    fn is_special_user_message(&self) -> bool {
        self.raw_payload()
            .and_then(codex_format_response_item)
            .map(|text| {
                let trimmed = text.trim_start();
                trimmed
                    .strip_prefix('<')
                    .and_then(|rest| rest.split('>').next())
                    .is_some_and(|tag| {
                        matches!(
                            tag,
                            "user_instructions" | "environment_context" | "user_action"
                        )
                    })
            })
            .unwrap_or(false)
    }

    fn to_event(&self, include_raw: bool) -> Option<SessionEvent> {
        let (_category, mut actor, entry_category) = self.category_with_actor();

        if matches!(entry_category, CodexEntryCategory::ResponseItem)
            && actor
                .as_deref()
                .is_some_and(|role| role.eq_ignore_ascii_case("user"))
        {
            if self
                .data
                .payload
                .as_ref()
                .and_then(|payload| payload.payload_type.as_deref())
                .is_some_and(|payload_type| payload_type.eq_ignore_ascii_case("message"))
                && !self.is_special_user_message()
            {
                // Codex often emits both a response_item(message, user) and a
                // follow-up event_msg(user_message) for the same user turn.
                // Skip the redundant response_item so user messages only appear
                // once in transcripts, except when the message is one of the
                // tagged AGENTS sections that Codex does not mirror as an
                // event_msg payload.
                return None;
            }
        }

        let payload_value = self.payload_value();

        let text = match entry_category {
            CodexEntryCategory::SessionMeta => self
                .raw_payload()
                .and_then(codex_format_session_meta)
                .unwrap_or_else(|| pretty_json(&self.raw)),
            CodexEntryCategory::ResponseItem => self
                .raw_payload()
                .and_then(codex_format_response_item)
                .unwrap_or_else(|| pretty_json(&self.raw)),
            CodexEntryCategory::UserMessage | CodexEntryCategory::AssistantMessage => self
                .raw_payload()
                .and_then(codex_format_event_message)
                .unwrap_or_else(|| {
                    self.data
                        .message
                        .clone()
                        .unwrap_or_else(|| pretty_json(&self.raw))
                }),
            CodexEntryCategory::Event(ref kind) => {
                if kind == "event_msg" {
                    self.raw_payload()
                        .and_then(codex_format_event_message)
                        .unwrap_or_else(|| pretty_json(&self.raw))
                } else {
                    self.raw_payload()
                        .and_then(codex_format_generic_payload)
                        .or_else(|| {
                            self.data
                                .message
                                .clone()
                                .map(|value| value.trim().to_string())
                        })
                        .unwrap_or_else(|| pretty_json(&self.raw))
                }
            }
        };

        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }

        let working_dir = self.working_dir_hint();
        let tool = payload_value.as_ref().and_then(extract_codex_tool_event);

        let data = attach_working_dir(payload_value, working_dir.clone());

        let mut category_value = match entry_category {
            CodexEntryCategory::Event(ref kind) => kind.clone(),
            other => match other {
                CodexEntryCategory::SessionMeta => "session_meta".to_string(),
                CodexEntryCategory::ResponseItem => "response_item".to_string(),
                CodexEntryCategory::UserMessage => "user_message".to_string(),
                CodexEntryCategory::AssistantMessage => "assistant_message".to_string(),
                CodexEntryCategory::Event(_) => unreachable!(),
            },
        };

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
            if actor.is_none() {
                actor = Some(match tool_event.phase {
                    SessionToolPhase::Use => "assistant".to_string(),
                    SessionToolPhase::Result => "tool".to_string(),
                });
            }
        }

        let timestamp = self.timestamp();

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
            summary_text: None,
            data,
            timestamp,
            raw,
            tool,
        })
    }
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
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let Ok(raw) = serde_json::from_str::<Value>(trimmed) else {
                continue;
            };

            let Some(entry) = CodexParsedEntry::parse(raw) else {
                continue;
            };
            let (_, _, entry_category) = entry.category_with_actor();
            entry.apply_summary(&mut record, &entry_category);
            if let Some(event) = entry.to_event(false) {
                record.ingest_event(&event);
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

            let Some(entry) = CodexParsedEntry::parse(raw) else {
                continue;
            };

            if let Some(event) = entry.to_event(true) {
                events.push(event);
            }
        }

        Ok(events)
    }
}

fn extract_working_dir_from_message(value: &Value) -> Option<String> {
    if let Some(content) = value.get("content") {
        if let Some(dir) = extract_working_dir_from_content(content) {
            return Some(dir);
        }
    }
    if let Some(message) = value.get("message") {
        if let Some(content) = message.get("content") {
            if let Some(dir) = extract_working_dir_from_content(content) {
                return Some(dir);
            }
        }
    }
    None
}

fn extract_working_dir_from_content(content: &Value) -> Option<String> {
    match content {
        Value::String(text) => extract_cwd_from_text(text),
        Value::Array(items) => {
            for item in items {
                if let Some(text) = item.get("text").and_then(|entry| entry.as_str()) {
                    if let Some(dir) = extract_cwd_from_text(text) {
                        return Some(dir);
                    }
                }
                if let Some(nested) = item.get("content") {
                    if let Some(dir) = extract_working_dir_from_content(nested) {
                        return Some(dir);
                    }
                }
            }
            None
        }
        Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(|entry| entry.as_str()) {
                if let Some(dir) = extract_cwd_from_text(text) {
                    return Some(dir);
                }
            }
            if let Some(nested) = map.get("content") {
                return extract_working_dir_from_content(nested);
            }
            None
        }
        _ => None,
    }
}

fn extract_cwd_from_text(text: &str) -> Option<String> {
    let mut slice = text;
    loop {
        let Some(start) = slice.find("<cwd>") else {
            return None;
        };
        let remainder = &slice[start + 5..];
        let Some(end) = remainder.find("</cwd>") else {
            return None;
        };
        let value = remainder[..end].trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
        let next_index = start + 5 + end + 6;
        if next_index >= slice.len() {
            return None;
        }
        slice = &slice[next_index..];
    }
}

fn attach_working_dir(data: Option<Value>, working_dir: Option<String>) -> Option<Value> {
    let Some(dir) = working_dir else {
        return data;
    };

    match data {
        Some(Value::Object(mut map)) => {
            map.entry("working_dir".to_string())
                .or_insert_with(|| Value::String(dir.clone()));
            Some(Value::Object(map))
        }
        Some(other) => {
            let mut map = serde_json::Map::new();
            map.insert("payload".to_string(), other);
            map.insert("working_dir".to_string(), Value::String(dir));
            Some(Value::Object(map))
        }
        None => {
            let mut map = serde_json::Map::new();
            map.insert("working_dir".to_string(), Value::String(dir));
            Some(Value::Object(map))
        }
    }
}

fn extract_codex_tool_event(payload: &Value) -> Option<SessionToolEvent> {
    let object = payload.as_object()?;
    let entry_type = object
        .get("type")
        .or_else(|| object.get("payload_type"))
        .and_then(|value| value.as_str())?;
    let phase = match entry_type {
        "tool_use" | "function_call" => SessionToolPhase::Use,
        "tool_result" | "function_call_output" => SessionToolPhase::Result,
        _ => return None,
    };

    let mut extras = Map::new();
    for (key, value) in object.iter() {
        if matches!(
            key.as_str(),
            "type"
                | "payload_type"
                | "role"
                | "message"
                | "content"
                | "id"
                | "tool_use_id"
                | "call_id"
                | "originator"
                | "instructions"
                | "cwd"
                | "working_dir"
                | "name"
                | "input"
                | "arguments"
                | "output"
                | "result"
        ) {
            continue;
        }
        extras.insert(key.clone(), value.clone());
    }

    // Extract working_dir only from the message itself, not from session hint
    let mut working_dir_value = object
        .get("working_dir")
        .or_else(|| object.get("cwd"))
        .and_then(|value| value.as_str())
        .map(|dir| dir.to_string());

    let input = object
        .get("input")
        .cloned()
        .or_else(|| object.get("arguments").and_then(parse_jsonish_string));
    let output = match phase {
        SessionToolPhase::Use => None,
        SessionToolPhase::Result => object
            .get("output")
            .or_else(|| object.get("result"))
            .or_else(|| object.get("content"))
            .and_then(parse_jsonish_string),
    };

    let identifier = object
        .get("tool_use_id")
        .or_else(|| object.get("call_id"))
        .or_else(|| object.get("id"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());

    if working_dir_value.is_none() {
        if let Some(Value::Object(map)) = input.as_ref() {
            if let Some(Value::String(dir)) = map
                .get("workdir")
                .or_else(|| map.get("cwd"))
                .or_else(|| map.get("working_dir"))
            {
                working_dir_value = Some(dir.clone());
            }
        }
    }

    Some(SessionToolEvent {
        phase,
        name: object
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

fn parse_jsonish_string(value: &Value) -> Option<Value> {
    match value {
        Value::String(raw) => {
            if let Ok(parsed) = serde_json::from_str::<Value>(raw) {
                Some(parsed)
            } else {
                Some(Value::String(raw.clone()))
            }
        }
        other => Some(other.clone()),
    }
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

fn codex_format_session_meta(payload: &Value) -> Option<String> {
    let payload = payload.as_object()?;
    let mut parts: Vec<String> = Vec::new();

    if let Some(id) = payload.get("id").and_then(|entry| entry.as_str()) {
        parts.push(format!("Session ID: {id}"));
    }
    if let Some(originator) = payload.get("originator").and_then(|entry| entry.as_str()) {
        parts.push(format!("Originator: {originator}"));
    }
    if let Some(version) = payload.get("cli_version").and_then(|entry| entry.as_str()) {
        parts.push(format!("CLI version: {version}"));
    }
    if let Some(cwd) = payload.get("cwd").and_then(|entry| entry.as_str()) {
        parts.push(format!("Working directory: {cwd}"));
    }
    if let Some(instructions) = payload
        .get("instructions")
        .and_then(|entry| entry.as_str())
        .map(str::trim)
    {
        if !instructions.is_empty() {
            parts.push(format!("Instructions:\n{instructions}"));
        }
    }

    if parts.is_empty() {
        Some(pretty_json(&Value::Object(payload.clone())))
    } else {
        Some(parts.join("\n\n"))
    }
}

fn codex_format_response_item(payload: &Value) -> Option<String> {
    let content = payload.get("content")?;
    match content {
        Value::Array(items) => {
            let mut parts: Vec<String> = Vec::new();
            for item in items {
                if let Some(text) = codex_format_content_item(item) {
                    parts.push(text);
                }
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n\n"))
            }
        }
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        other => Some(pretty_json(other)),
    }
}

fn codex_format_event_message(payload: &Value) -> Option<String> {
    if let Some(text) = payload.get("message").and_then(|entry| entry.as_str()) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    Some(pretty_json(payload))
}

fn codex_format_generic_payload(payload: &Value) -> Option<String> {
    if payload.is_null() {
        return None;
    }
    if let Some(text) = payload.as_str() {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    } else {
        Some(pretty_json(payload))
    }
}

fn codex_format_content_item(item: &Value) -> Option<String> {
    let item_type = item
        .get("type")
        .and_then(|entry| entry.as_str())
        .unwrap_or_default();

    match item_type {
        "input_text" | "output_text" | "text" | "code" => item
            .get("text")
            .and_then(|entry| entry.as_str())
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty()),
        "tool_use" => {
            let name = item
                .get("name")
                .and_then(|entry| entry.as_str())
                .unwrap_or("tool");
            let mut parts = vec![format!("Tool call: {name}")];
            if let Some(input) = item.get("input") {
                let serialized = pretty_json(input);
                if !serialized.is_empty() {
                    parts.push(serialized);
                }
            }
            Some(parts.join("\n"))
        }
        "tool_result" => {
            let mut parts: Vec<String> = Vec::new();
            if let Some(id) = item
                .get("tool_use_id")
                .and_then(|entry| entry.as_str())
                .filter(|entry| !entry.is_empty())
            {
                parts.push(format!("Tool result ({id})"));
            } else {
                parts.push("Tool result".to_string());
            }

            if let Some(content) = item.get("content") {
                match content {
                    Value::Array(entries) => {
                        let mut details: Vec<String> = Vec::new();
                        for entry in entries {
                            if let Some(text) = entry.get("text").and_then(|value| value.as_str()) {
                                let trimmed = text.trim();
                                if !trimmed.is_empty() {
                                    details.push(trimmed.to_string());
                                }
                                continue;
                            }
                            details.push(pretty_json(entry));
                        }
                        if !details.is_empty() {
                            parts.push(details.join("\n"));
                        }
                    }
                    Value::String(text) => {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            parts.push(trimmed.to_string());
                        }
                    }
                    other => parts.push(pretty_json(other)),
                }
            } else if let Some(text) = item.get("text").and_then(|entry| entry.as_str()) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
            }

            Some(parts.join("\n"))
        }
        _ => {
            if let Some(text) = item.get("text").and_then(|entry| entry.as_str()) {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    Some(pretty_json(item))
                } else {
                    Some(trimmed.to_string())
                }
            } else {
                Some(pretty_json(item))
            }
        }
    }
}

fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests;
