use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

mod codex;

pub use codex::CodexSessionProvider;

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
        }
    }

    pub fn set_working_dir(&mut self, path: &str) {
        self.working_dir = Some(PathBuf::from(path));
    }
}

pub trait SessionProvider {
    fn name(&self) -> &'static str;
    fn list_sessions(&self) -> Result<Vec<SessionRecord>>;
}

pub fn default_providers() -> Vec<Box<dyn SessionProvider + Send + Sync>> {
    vec![Box::new(CodexSessionProvider::new())]
}

pub fn canonicalize(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}
