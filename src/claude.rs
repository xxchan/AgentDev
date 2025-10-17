use chrono::{DateTime, Utc};
use std::path::Path;

use crate::sessions::ClaudeCliSessionProvider;

#[derive(Debug)]
pub struct SessionInfo {
    pub id: String,
    pub last_user_message: String,
    pub last_timestamp: Option<DateTime<Utc>>,
    pub user_messages: Vec<String>,
}

pub fn get_claude_sessions(project_path: &Path) -> Vec<SessionInfo> {
    let provider = ClaudeCliSessionProvider::new();
    let records = match provider.list_sessions_for_path(project_path) {
        Ok(records) => records,
        Err(_) => return Vec::new(),
    };

    let mut sessions: Vec<SessionInfo> = records
        .into_iter()
        .filter_map(|record| {
            if record.user_messages.is_empty() {
                return None;
            }

            let SessionData {
                id,
                last_user_message,
                last_timestamp,
                user_messages,
            } = SessionData::from(record);

            Some(SessionInfo {
                id,
                last_user_message,
                last_timestamp,
                user_messages,
            })
        })
        .collect();

    sessions.sort_by(|a, b| match (&b.last_timestamp, &a.last_timestamp) {
        (Some(b_ts), Some(a_ts)) => b_ts.cmp(a_ts),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    sessions
}

struct SessionData {
    id: String,
    last_user_message: String,
    last_timestamp: Option<DateTime<Utc>>,
    user_messages: Vec<String>,
}

impl From<crate::sessions::SessionRecord> for SessionData {
    fn from(mut record: crate::sessions::SessionRecord) -> Self {
        let last_user_message = record
            .last_user_message
            .take()
            .or_else(|| record.user_messages.last().cloned())
            .unwrap_or_default();
        Self {
            id: record.id,
            last_user_message,
            last_timestamp: record.last_timestamp,
            user_messages: record.user_messages,
        }
    }
}
