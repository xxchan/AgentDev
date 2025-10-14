'use client';

import { useMemo } from 'react';
import { useWorktreeProcesses } from '@/hooks/useWorktreeProcesses';
import {
  WorktreeProcessStatus,
  WorktreeProcessSummary,
} from '@/types';

interface WorktreeProcessesProps {
  worktreeId: string | null;
  worktreeName?: string | null;
}

const STATUS_STYLES: Record<WorktreeProcessStatus, string> = {
  pending: 'bg-amber-100 text-amber-800 border border-amber-200',
  running: 'bg-emerald-100 text-emerald-800 border border-emerald-200',
  succeeded: 'bg-sky-100 text-sky-800 border border-sky-200',
  failed: 'bg-rose-100 text-rose-800 border border-rose-200',
  unknown: 'bg-gray-200 text-gray-700 border border-gray-300',
};

function formatRelativeTime(value?: string | null) {
  if (!value) {
    return null;
  }

  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return null;
  }

  const diffMs = Date.now() - date.getTime();
  const diffMinutes = Math.max(Math.floor(diffMs / (1000 * 60)), 0);

  if (diffMinutes < 1) {
    return 'just now';
  }
  if (diffMinutes < 60) {
    return `${diffMinutes}m ago`;
  }
  const diffHours = Math.floor(diffMinutes / 60);
  if (diffHours < 24) {
    return `${diffHours}h ago`;
  }
  const diffDays = Math.floor(diffHours / 24);
  if (diffDays < 7) {
    return `${diffDays}d ago`;
  }
  const diffWeeks = Math.floor(diffDays / 7);
  if (diffWeeks < 4) {
    return `${diffWeeks}w ago`;
  }
  const diffMonths = Math.floor(diffDays / 30);
  return `${diffMonths}mo ago`;
}

function formatTimestamp(value?: string | null) {
  if (!value) {
    return 'unknown';
  }

  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return 'unknown';
  }

  const relative = formatRelativeTime(value);
  return `${date.toLocaleString()}${relative ? ` (${relative})` : ''}`;
}

function formatCommand(command: string[]) {
  if (command.length === 0) {
    return 'unknown command';
  }
  return command.join(' ');
}

function getStatusLabel(status: WorktreeProcessStatus) {
  switch (status) {
    case 'pending':
      return 'Pending';
    case 'running':
      return 'Running';
    case 'succeeded':
      return 'Succeeded';
    case 'failed':
      return 'Failed';
    default:
      return 'Unknown';
  }
}

function ProcessCard({ process }: { process: WorktreeProcessSummary }) {
  const statusClass = STATUS_STYLES[process.status] ?? STATUS_STYLES.unknown;
  const statusLabel = getStatusLabel(process.status);

  const detailLabel = useMemo(() => {
    if (process.status === 'running') {
      const relative = formatRelativeTime(process.started_at);
      return relative ? `Started ${relative}` : 'Started recently';
    }
    if (process.status === 'pending') {
      return 'Waiting to start';
    }
    const reference = process.finished_at ?? process.started_at;
    const relative = formatRelativeTime(reference);
    if (relative) {
      return `${process.status === 'failed' ? 'Failed' : 'Completed'} ${relative}`;
    }
    return process.status === 'failed' ? 'Failed recently' : 'Completed';
  }, [process.finished_at, process.started_at, process.status]);

  const exitCodeLabel = useMemo(() => {
    if (process.exit_code == null) {
      return null;
    }
    return `Exit code ${process.exit_code}`;
  }, [process.exit_code]);

  return (
    <div className="flex flex-col gap-3 rounded-lg border border-border bg-card px-4 py-4 shadow-sm">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex flex-col gap-1">
          <div className="flex items-center gap-2">
            <span className={`inline-flex items-center rounded-full px-2 py-0.5 text-xs font-semibold ${statusClass}`}>
              {statusLabel}
            </span>
            {exitCodeLabel && (
              <span className="text-xs text-muted-foreground">{exitCodeLabel}</span>
            )}
          </div>
          <code className="rounded bg-muted px-2 py-1 text-sm text-foreground">
            {formatCommand(process.command)}
          </code>
          {process.description && (
            <p className="text-xs text-muted-foreground">{process.description}</p>
          )}
        </div>
        <div className="text-right text-xs text-muted-foreground">
          <div>{detailLabel}</div>
          {process.started_at && (
            <div className="mt-1">Started: {formatTimestamp(process.started_at)}</div>
          )}
          {process.finished_at && (
            <div className="mt-1">Finished: {formatTimestamp(process.finished_at)}</div>
          )}
        </div>
      </div>
      {process.cwd && (
        <div className="text-xs text-muted-foreground">
          CWD: <code className="font-mono text-[0.7rem]">{process.cwd}</code>
        </div>
      )}
    </div>
  );
}

export default function WorktreeProcesses({
  worktreeId,
  worktreeName,
}: WorktreeProcessesProps) {
  const { processes, isLoading, error, refetch } = useWorktreeProcesses(worktreeId);

  if (!worktreeId) {
    return (
      <div className="flex h-full items-center justify-center px-6 text-center text-sm text-muted-foreground">
        <div>
          <p className="font-medium text-foreground">Select a worktree to view running commands</p>
          <p className="mt-2 text-xs text-muted-foreground">
            Active processes launched via <code className="rounded bg-muted px-1 py-0.5">agentdev worktree exec</code> will show up here.
          </p>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
        <div className="text-sm text-red-600">{error}</div>
        <button
          type="button"
          onClick={() => refetch()}
          className="rounded-md border border-border bg-background px-3 py-2 text-xs font-medium text-foreground hover:bg-muted"
        >
          Retry
        </button>
      </div>
    );
  }

  if (isLoading && processes.length === 0) {
    return (
      <div className="flex h-full items-center justify-center px-6">
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <div className="inline-block h-5 w-5 animate-spin rounded-full border-2 border-muted border-t-primary" />
          <span>Loading processes…</span>
        </div>
      </div>
    );
  }

  if (processes.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center text-sm text-muted-foreground">
        <div>
          <p className="font-medium text-foreground">
            No active commands for {worktreeName ?? 'this worktree'}
          </p>
          <p className="mt-2 text-xs text-muted-foreground">
            Launch a command with <code className="rounded bg-muted px-1 py-0.5">agentdev worktree exec</code> and it will appear here with live status updates.
          </p>
        </div>
        <button
          type="button"
          onClick={() => refetch()}
          className="rounded-md border border-border bg-background px-3 py-2 text-xs font-medium text-foreground hover:bg-muted"
          disabled={isLoading}
        >
          Refresh
        </button>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col gap-3 overflow-y-auto px-6 py-4">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-sm font-semibold text-foreground">Commands for {worktreeName ?? 'selected worktree'}</h3>
          <p className="text-xs text-muted-foreground">
            Refreshes automatically every 5 seconds. Use the refresh button to pull the latest state on demand.
          </p>
        </div>
        <button
          type="button"
          onClick={() => refetch()}
          className="rounded-md border border-border bg-background px-3 py-2 text-xs font-medium text-foreground hover:bg-muted"
        >
          Refresh
        </button>
      </div>

      <div className="space-y-3">
        {processes.map((process) => (
          <ProcessCard key={process.id} process={process} />
        ))}
      </div>
    </div>
  );
}
