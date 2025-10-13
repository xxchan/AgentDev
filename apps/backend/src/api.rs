use anyhow::Result;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

use crate::AppState;

// Re-export main agentdev types and functions
use agentdev::{
    config::{load_agent_config, split_cmdline},
    git::{get_diff_for_path, get_repo_name, execute_git},
    state::{XlaudeState, WorktreeInfo},
    tmux::TmuxManager,
    utils::generate_random_name,
};

#[derive(Serialize, Deserialize, Clone)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub agents: Vec<Agent>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub alias: String,
    pub status: AgentStatus,
    pub worktree_path: Option<String>,
    pub tmux_session: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum AgentStatus {
    Initializing,
    Ready,
    Running,
    Error(String),
}

// Internal task state management
static TASK_STATE: std::sync::OnceLock<std::sync::Mutex<HashMap<String, Task>>> = std::sync::OnceLock::new();

pub fn get_task_state() -> &'static std::sync::Mutex<HashMap<String, Task>> {
    TASK_STATE.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

#[derive(Deserialize)]
pub struct CreateTaskRequest {
    pub prompt: String,
    pub agents: Option<Vec<String>>, // Agent aliases to use, default to all configured agents
    pub name: Option<String>, // Task name, default to random BIP39 words
}

#[derive(Serialize)]
pub struct CreateTaskResponse {
    pub task: Task,
}

/// GET /api/tasks - Get all tasks
pub async fn get_tasks(State(_state): State<AppState>) -> impl IntoResponse {
    let task_state = get_task_state();
    let tasks_map = task_state.lock().unwrap();
    let tasks: Vec<Task> = tasks_map.values().cloned().collect();
    Json(tasks)
}

/// POST /api/tasks - Create a new task
pub async fn create_task(
    State(_state): State<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> impl IntoResponse {
    println!("Creating task with prompt: {}", req.prompt);
    
    match create_task_impl(req).await {
        Ok(task) => {
            // Store task in memory
            let task_state = get_task_state();
            let mut tasks_map = task_state.lock().unwrap();
            tasks_map.insert(task.id.clone(), task.clone());
            
            Json(CreateTaskResponse { task }).into_response()
        }
        Err(e) => {
            eprintln!("Failed to create task: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create task: {}", e)).into_response()
        }
    }
}

async fn create_task_impl(req: CreateTaskRequest) -> Result<Task> {
    let task_id = Uuid::new_v4().to_string();
    let task_name = req.name.unwrap_or_else(|| {
        generate_random_name().unwrap_or_else(|_| format!("task-{}", &task_id[..8]))
    });

    // Load agent configuration
    let agent_config = load_agent_config()?;
    let requested_agents = req.agents.unwrap_or_else(|| {
        agent_config.agents.keys().cloned().collect()
    });

    if requested_agents.is_empty() {
        anyhow::bail!("No agents configured or requested");
    }

    // Get current repo info
    let repo_name = get_repo_name()?;
    let current_dir = std::env::current_dir()?;
    
    let mut agents = Vec::new();
    let prompt_text = req.prompt.clone(); // Clone early to avoid move issues

    for agent_alias in requested_agents {
        let tmux_manager = TmuxManager::new(); // Create new instance each time
        if let Some(agent_command) = agent_config.agents.get(&agent_alias) {
            let agent_id = Uuid::new_v4().to_string();
            let agent_name = format!("{}-{}", agent_alias, &task_name);
            
            // Create worktree for this agent
            let worktree_name = format!("{}-{}", task_name, agent_alias);
            let worktree_path = current_dir.parent()
                .unwrap_or(&current_dir)
                .join(format!("{}-{}", repo_name, worktree_name));

            match create_worktree_for_agent(&worktree_name, &worktree_path).await {
                Ok(_) => {
                    // Create tmux session with agent command
                    let session_name = format!("{}-{}", task_name, agent_alias);
                    let (program, args) = split_cmdline(agent_command)?;
                    
                    if let Err(e) = tmux_manager.create_session_with_command(
                        &session_name, 
                        &worktree_path,
                        &program,
                        &args
                    ) {
                        eprintln!("Warning: Failed to create tmux session for {}: {}", agent_alias, e);
                    }

                    agents.push(Agent {
                        id: agent_id,
                        name: agent_name.clone(),
                        alias: agent_alias.clone(),
                        status: AgentStatus::Ready,
                        worktree_path: Some(worktree_path.to_string_lossy().to_string()),
                        tmux_session: Some(tmux_manager.session_name(&session_name)),
                    });
                    
                    // Send initial prompt to the agent
                    let prompt_clone = prompt_text.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        if let Err(e) = tmux_manager.send_text(&session_name, &prompt_clone) {
                            eprintln!("Failed to send prompt to {}: {}", agent_name, e);
                        }
                        if let Err(e) = tmux_manager.send_enter(&session_name) {
                            eprintln!("Failed to send enter to {}: {}", agent_name, e);
                        }
                    });
                }
                Err(e) => {
                    agents.push(Agent {
                        id: agent_id,
                        name: agent_name,
                        alias: agent_alias,
                        status: AgentStatus::Error(format!("Failed to create worktree: {}", e)),
                        worktree_path: None,
                        tmux_session: None,
                    });
                }
            }
        } else {
            let agent_id = Uuid::new_v4().to_string();
            agents.push(Agent {
                id: agent_id,
                name: agent_alias.clone(),
                alias: agent_alias,
                status: AgentStatus::Error("Agent not found in configuration".to_string()),
                worktree_path: None,
                tmux_session: None,
            });
        }
    }

    Ok(Task {
        id: task_id,
        name: task_name,
        prompt: req.prompt,
        created_at: chrono::Utc::now(),
        agents,
    })
}

async fn create_worktree_for_agent(worktree_name: &str, worktree_path: &PathBuf) -> Result<()> {
    // Create git worktree
    let path_str = worktree_path.to_string_lossy();
    execute_git(&["worktree", "add", "-b", worktree_name, &path_str, "HEAD"])?;
    
    // Update submodules if present
    agentdev::git::update_submodules(worktree_path)?;
    
    // Save to agentdev state
    let mut state = XlaudeState::load()?;
    let repo_name = get_repo_name()?;
    let key = XlaudeState::make_key(&repo_name, worktree_name);
    
    let worktree_info = WorktreeInfo {
        name: worktree_name.to_string(),
        branch: worktree_name.to_string(),
        path: worktree_path.clone(),
        repo_name,
        created_at: chrono::Utc::now(),
        task_id: Some(worktree_name.to_string()),
        initial_prompt: None,
    };
    
    state.worktrees.insert(key, worktree_info);
    state.save()?;
    
    Ok(())
}

/// DELETE /api/tasks/:task_id - Delete a task
pub async fn delete_task(
    State(_state): State<AppState>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    println!("Deleting task: {}", task_id);
    
    match delete_task_impl(&task_id).await {
        Ok(_) => {
            // Remove from memory
            let task_state = get_task_state();
            let mut tasks_map = task_state.lock().unwrap();
            tasks_map.remove(&task_id);
            StatusCode::OK.into_response()
        }
        Err(e) => {
            eprintln!("Failed to delete task: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to delete task: {}", e)).into_response()
        }
    }
}

async fn delete_task_impl(task_id: &str) -> Result<()> {
    let task_state = get_task_state();
    let task = {
        let tasks_map = task_state.lock().unwrap();
        tasks_map.get(task_id).cloned()
    };
    
    let Some(task) = task else {
        anyhow::bail!("Task not found: {}", task_id);
    };
    
    let tmux_manager = TmuxManager::new();
    
    for agent in &task.agents {
        // Kill tmux session
        if let Some(session) = &agent.tmux_session {
            let session_name = session.strip_prefix("agentdev_").unwrap_or(session);
            if let Err(e) = tmux_manager.kill_session(session_name) {
                eprintln!("Failed to kill tmux session {}: {}", session_name, e);
            }
        }
        
        // Remove git worktree
        if let Some(worktree_path) = &agent.worktree_path {
            let path = PathBuf::from(worktree_path);
            if let Err(e) = execute_git(&["worktree", "remove", "--force", &path.to_string_lossy()]) {
                eprintln!("Failed to remove worktree {}: {}", path.display(), e);
            }
            
            // Remove from state
            let mut state = XlaudeState::load()?;
            let keys_to_remove: Vec<String> = state.worktrees
                .iter()
                .filter(|(_, info)| info.path == path)
                .map(|(key, _)| key.clone())
                .collect();
            
            for key in keys_to_remove {
                state.worktrees.remove(&key);
            }
            
            state.save()?;
        }
    }
    
    Ok(())
}

/// GET /api/tasks/:task_id/agents/:agent_id/diff - Get agent diff
pub async fn get_agent_diff(
    State(_state): State<AppState>,
    Path((task_id, agent_id)): Path<(String, String)>,
) -> impl IntoResponse {
    println!("Getting diff for task {} agent {}", task_id, agent_id);
    
    match get_agent_diff_impl(&task_id, &agent_id).await {
        Ok(diff) => diff.into_response(),
        Err(e) => {
            eprintln!("Failed to get diff: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get diff: {}", e)).into_response()
        }
    }
}

async fn get_agent_diff_impl(task_id: &str, agent_id: &str) -> Result<String> {
    let task_state = get_task_state();
    let task = {
        let tasks_map = task_state.lock().unwrap();
        tasks_map.get(task_id).cloned()
    };
    
    let Some(task) = task else {
        anyhow::bail!("Task not found: {}", task_id);
    };
    
    let agent = task.agents.iter()
        .find(|a| a.id == agent_id)
        .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", agent_id))?;
    
    let Some(worktree_path) = &agent.worktree_path else {
        anyhow::bail!("Agent has no worktree path");
    };
    
    let path = PathBuf::from(worktree_path);
    
    if !path.exists() {
        anyhow::bail!("Worktree path does not exist: {}", path.display());
    }
    
    get_diff_for_path(&path)
}
