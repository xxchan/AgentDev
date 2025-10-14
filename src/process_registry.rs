use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::{XlaudeState, get_config_dir};

const REGISTRY_FILENAME: &str = "processes.json";
pub const MAX_PROCESSES_PER_WORKTREE: usize = 25;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessRecord {
    pub id: String,
    pub worktree_key: String,
    pub worktree_name: String,
    pub repo_name: String,
    pub command: Vec<String>,
    pub status: ProcessStatus,
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl ProcessRecord {
    pub fn new(
        worktree_key: String,
        worktree_name: String,
        repo_name: String,
        command: Vec<String>,
        cwd: Option<PathBuf>,
        status: ProcessStatus,
    ) -> Self {
        let id = Uuid::new_v4().to_string();
        let started_at = Utc::now();
        Self {
            id,
            worktree_key,
            worktree_name,
            repo_name,
            command,
            status,
            started_at,
            finished_at: None,
            exit_code: None,
            cwd,
            description: None,
            error: None,
            stdout: None,
            stderr: None,
            updated_at: started_at,
        }
    }

    pub fn mark_running(&mut self) {
        self.status = ProcessStatus::Running;
        self.started_at = Utc::now();
        self.updated_at = self.started_at;
        self.stdout = None;
        self.stderr = None;
    }

    pub fn mark_finished(
        &mut self,
        status: ProcessStatus,
        exit_code: Option<i32>,
        error: Option<String>,
        stdout: Option<String>,
        stderr: Option<String>,
    ) {
        self.status = status;
        self.exit_code = exit_code;
        self.finished_at = Some(Utc::now());
        self.error = error;
        self.stdout = stdout;
        self.stderr = stderr;
        self.updated_at = Utc::now();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessRegistry {
    pub processes: HashMap<String, ProcessRecord>,
}

impl ProcessRegistry {
    pub fn load() -> Result<Self> {
        let _guard = registry_lock()
            .lock()
            .expect("Process registry lock poisoned");
        Self::load_unlocked()
    }

    pub fn insert(&mut self, record: ProcessRecord) {
        self.processes.insert(record.id.clone(), record);
    }

    pub fn update<F>(&mut self, id: &str, mut updater: F) -> Result<()>
    where
        F: FnMut(&mut ProcessRecord),
    {
        let record = self
            .processes
            .get_mut(id)
            .with_context(|| format!("Process record {id} not found"))?;
        updater(record);
        record.updated_at = Utc::now();
        Ok(())
    }

    pub fn processes_for_worktree(&self, worktree_key: &str) -> Vec<&ProcessRecord> {
        let mut filtered: Vec<&ProcessRecord> = self
            .processes
            .values()
            .filter(|record| record.worktree_key == worktree_key)
            .collect();
        filtered.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        filtered
    }

    pub fn prune_missing_worktrees(&mut self) -> Result<()> {
        let state = XlaudeState::load()?;
        self.processes
            .retain(|_, record| state.worktrees.contains_key(&record.worktree_key));
        Ok(())
    }

    pub fn retain_recent(&mut self, max_per_worktree: usize) {
        if max_per_worktree == 0 {
            return;
        }
        let mut grouped: HashMap<String, Vec<String>> = HashMap::new();
        for record in self.processes.values() {
            let entry = grouped.entry(record.worktree_key.clone()).or_default();
            entry.push(record.id.clone());
        }

        for ids in grouped.values_mut() {
            ids.sort_by(|a, b| {
                let a_rec = &self.processes[a];
                let b_rec = &self.processes[b];
                b_rec.started_at.cmp(&a_rec.started_at)
            });
        }

        for (_worktree, ids) in grouped {
            if ids.len() <= max_per_worktree {
                continue;
            }
            for id in ids.iter().skip(max_per_worktree) {
                self.processes.remove(id);
            }
        }
    }

    pub fn mutate<F>(mutator: F) -> Result<()>
    where
        F: FnOnce(&mut ProcessRegistry) -> Result<()>,
    {
        let _guard = registry_lock()
            .lock()
            .expect("Process registry lock poisoned");
        let mut registry = Self::load_unlocked()?;
        mutator(&mut registry)?;
        registry.save_unlocked()?;
        Ok(())
    }
}

fn registry_path() -> Result<PathBuf> {
    let dir = get_config_dir()?;
    Ok(dir.join(REGISTRY_FILENAME))
}

pub fn canonicalize_cwd(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn registry_lock() -> &'static Mutex<()> {
    static REGISTRY_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    REGISTRY_LOCK.get_or_init(|| Mutex::new(()))
}

impl ProcessRegistry {
    fn load_unlocked() -> Result<Self> {
        let path = registry_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read process registry at {}", path.display()))?;
        let registry = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse process registry at {}", path.display()))?;
        Ok(registry)
    }

    fn save_unlocked(&self) -> Result<()> {
        let path = registry_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context("Failed to create config directory for registry")?;
        }
        let content =
            serde_json::to_string_pretty(self).context("Failed to serialize process registry")?;
        fs::write(&path, content)
            .with_context(|| format!("Failed to write process registry to {}", path.display()))?;
        Ok(())
    }
}
