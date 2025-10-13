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
