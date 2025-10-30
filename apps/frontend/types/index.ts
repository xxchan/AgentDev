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

export interface BaseSessionSummary {
  provider: string;
  session_id: string;
  last_user_message: string;
  last_timestamp?: string | null;
  user_message_count: number;
  user_messages_preview: string[];
}

export type WorktreeSessionSummary = BaseSessionSummary;

export interface SessionSummary extends BaseSessionSummary {
  worktree_id?: string | null;
  worktree_name?: string | null;
  repo_name?: string | null;
  branch?: string | null;
  working_dir?: string | null;
}

export interface SessionProviderSummary {
  provider: string;
  session_count: number;
  session_ids: string[];
  latest_timestamp?: string | null;
}

export type SessionDetailMode = 'user_only' | 'conversation' | 'full';

export interface SessionEvent {
  actor?: string;
  category: string;
  label?: string;
  text?: string;
  summary_text?: string;
  data?: Record<string, unknown> | null;
  timestamp?: string;
  raw?: unknown;
  tool?: SessionToolEvent | null;
}

export type SessionToolPhase = 'use' | 'result';

export interface SessionToolEvent {
  phase: SessionToolPhase;
  name?: string | null;
  identifier?: string | null;
  input?: unknown;
  output?: unknown;
  working_dir?: string | null;
  extras?: Record<string, unknown>;
}

export interface SessionDetailResponse {
  provider: string;
  session_id: string;
  last_timestamp?: string | null;
  working_dir?: string | null;
  mode: SessionDetailMode;
  events: SessionEvent[];
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

export interface SessionListResponse {
  sessions: SessionSummary[];
  providers?: SessionProviderSummary[];
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
  stdout?: string | null;
  stderr?: string | null;
}

export interface WorktreeProcessListResponse {
  processes: WorktreeProcessSummary[];
}

export interface DiscoveredWorktree {
  repo: string;
  path: string;
  branch?: string | null;
  head?: string | null;
  locked?: string | null;
  prunable?: string | null;
  bare: boolean;
}

export interface LaunchWorktreeCommandResponse {
  process: WorktreeProcessSummary;
}

export interface LaunchWorktreeShellResponse {
  status: 'launched';
}

export type MergeStrategyOption = 'ff-only' | 'merge' | 'squash';

export interface MergeWorktreeRequest {
  strategy?: MergeStrategyOption;
  push?: boolean;
  cleanup?: boolean;
}

export interface MergeWorktreeResponse {
  exit_code: number;
  stdout?: string;
  stderr?: string;
}

export interface DeleteWorktreeRequest {
  force?: boolean;
}

export interface DeleteWorktreeResponse {
  exit_code: number;
  removed: boolean;
  stdout?: string;
  stderr?: string;
}

export interface CommandFailurePayload {
  message: string;
  stdout?: string;
  stderr?: string;
  exit_code?: number;
}
