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

export interface WorktreeCommitsAhead {
  base_branch: string;
  merge_base?: string | null;
  commits: WorktreeCommitInfo[];
}

export interface WorktreeSessionSummary {
  provider: string;
  session_id: string;
  last_user_message: string;
  last_timestamp?: string | null;
  user_messages: string[];
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
  commits_ahead?: WorktreeCommitsAhead | null;
  sessions: WorktreeSessionSummary[];
}

export interface WorktreeListResponse {
  worktrees: WorktreeSummary[];
}

export interface WorktreeCommitDiff {
  reference: string;
  diff: string;
}

export interface WorktreeFileDiff {
  path: string;
  display_path: string;
  status: string;
  diff: string;
}

export interface WorktreeGitDetails {
  commit_diff?: WorktreeCommitDiff | null;
  staged: WorktreeFileDiff[];
  unstaged: WorktreeFileDiff[];
  untracked: WorktreeFileDiff[];
}

export type WorktreeProcessStatus =
  | 'pending'
  | 'running'
  | 'succeeded'
  | 'failed'
  | 'unknown';

export interface WorktreeProcessSummary {
  id: string;
  command: string[];
  status: WorktreeProcessStatus;
  started_at?: string | null;
  finished_at?: string | null;
  exit_code?: number | null;
  cwd?: string | null;
  description?: string | null;
}

export interface WorktreeProcessListResponse {
  processes: WorktreeProcessSummary[];
}
