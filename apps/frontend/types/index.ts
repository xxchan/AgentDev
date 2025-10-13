export interface Task {
  id: string;
  name: string;
  prompt: string;
  created_at: string;
  agents: Agent[];
}

export interface Agent {
  id: string;
  name: string;
  alias: string;
  status: AgentStatus;
  worktree_path?: string;
  tmux_session?: string;
}

export type AgentStatus = 
  | 'Initializing' 
  | 'Ready' 
  | 'Running' 
  | { Error: string };

export interface CreateTaskRequest {
  prompt: string;
  agents?: string[];
  name?: string;
}

export interface CreateTaskResponse {
  task: Task;
}

export interface WebSocketMessage {
  type: 'output' | 'input' | 'error' | 'connected' | 'disconnected';
  data: string;
  timestamp?: number;
}

export interface WorktreeGitStatus {
  branch: string;
  upstream?: string | null;
  ahead: number;
  behind: number;
  staged: number;
  unstaged: number;
  untracked: number;
  conflicts: number;
  is_clean: boolean;
}

export interface WorktreeCommitInfo {
  commit_id: string;
  summary: string;
  timestamp?: string | null;
}

export interface WorktreeSessionSummary {
  provider: string;
  last_user_message: string;
  last_timestamp?: string | null;
}

export interface WorktreeSummary {
  id: string;
  name: string;
  branch: string;
  repo_name: string;
  path: string;
  created_at: string;
  last_activity_at: string;
  task_id?: string | null;
  task_name?: string | null;
  initial_prompt?: string | null;
  agent_alias?: string | null;
  git_status?: WorktreeGitStatus | null;
  head_commit?: WorktreeCommitInfo | null;
  sessions: WorktreeSessionSummary[];
}

export interface WorktreeListResponse {
  worktrees: WorktreeSummary[];
}
