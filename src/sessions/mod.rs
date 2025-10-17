use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

mod claude_cli;
mod codex;
mod kimi;

pub use claude_cli::ClaudeCliSessionProvider;
pub use codex::CodexSessionProvider;
pub use kimi::KimiSessionProvider;

#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub provider: String,
    pub id: String,
    pub working_dir: Option<PathBuf>,
    pub originator: Option<String>,
    pub instructions: Option<String>,
    pub first_user_message: Option<String>,
    pub last_user_message: Option<String>,
    pub last_timestamp: Option<DateTime<Utc>>,
    pub file_path: PathBuf,
    pub user_messages: Vec<String>,
}

impl SessionRecord {
    pub fn new(provider: &str, file_path: PathBuf) -> Self {
        let default_id = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();

        Self {
            provider: provider.to_string(),
            id: default_id,
            working_dir: None,
            originator: None,
            instructions: None,
            first_user_message: None,
            last_user_message: None,
            last_timestamp: None,
            file_path,
            user_messages: Vec::new(),
        }
    }

    pub fn set_working_dir(&mut self, path: &str) {
        self.working_dir = Some(PathBuf::from(path));
    }

    pub fn ingest_event(&mut self, event: &SessionEvent) {
        if let Some(timestamp) = event.timestamp {
            if self
                .last_timestamp
                .map_or(true, |current| timestamp > current)
            {
                self.last_timestamp = Some(timestamp);
            }
        }

        if let Some(actor) = event.actor.as_deref() {
            if actor.eq_ignore_ascii_case("user") {
                if let Some(text) = event.summary_text.as_ref().or_else(|| event.text.as_ref()) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        let summary_text = trimmed.to_string();
                        if self.first_user_message.is_none() {
                            self.first_user_message = Some(summary_text.clone());
                        }
                        if self
                            .user_messages
                            .last()
                            .map_or(true, |previous| previous != &summary_text)
                        {
                            self.user_messages.push(summary_text.clone());
                        }
                        self.last_user_message = Some(summary_text);
                    }
                }
                if self.working_dir.is_none() {
                    if let Some(dir) = event
                        .data
                        .as_ref()
                        .and_then(|value| value.get("working_dir"))
                        .and_then(|entry| entry.as_str())
                    {
                        self.set_working_dir(dir);
                    }
                }
            }
        }

        if self.working_dir.is_none() {
            if let Some(dir) = event
                .tool
                .as_ref()
                .and_then(|tool| tool.working_dir.as_deref())
            {
                self.set_working_dir(dir);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    /// Provider-reported actor/role (e.g. "user", "assistant"). Forwarded as-is when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    /// AgentDev-normalized category. Tool events are normalized to `tool_use`/`tool_result`.
    pub category: String,
    /// Display label constructed by AgentDev for consistent UI headers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Provider-sourced message body. May contain tool summaries or textual replies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Short user message summary computed by AgentDev for grouping/search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_text: Option<String>,
    /// Provider-specific structured payload forwarded verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    /// Provider timestamp as reported in the raw log.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,
    /// Provider-native payload kept for debugging/trace purposes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<Value>,
    /// Normalized tool metadata extracted by AgentDev (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<SessionToolEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionToolPhase {
    Use,
    Result,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionToolEvent {
    /// Phase of the tool interaction (normalized by AgentDev).
    pub phase: SessionToolPhase,
    /// Tool name reported by the provider, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Identifier that links tool use/result pairs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    /// Structured tool input payload, if provided.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
    /// Structured tool output payload, if provided.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    /// Working directory associated with the tool call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    /// Provider-specific or unrecognized fields preserved for debugging.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub extras: Map<String, Value>,
}

/// TODO(provider-models): consider upgrading raw entry structs into provider-specific
/// enums (e.g. `CodexEvent`) before converting to `SessionEvent` so we can enforce
/// variant coverage at compile time and expose richer metadata downstream.

pub trait SessionProvider {
    fn name(&self) -> &'static str;
    fn list_sessions(&self) -> Result<Vec<SessionRecord>>;
    fn load_session_events(&self, record: &SessionRecord) -> Result<Vec<SessionEvent>>;
}

pub fn default_providers() -> Vec<Box<dyn SessionProvider + Send + Sync>> {
    vec![
        Box::new(ClaudeCliSessionProvider::new()),
        Box::new(CodexSessionProvider::new()),
        Box::new(KimiSessionProvider::new()),
    ]
}

pub fn canonicalize(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}
