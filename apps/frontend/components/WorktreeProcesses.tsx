'use client';

import {
  FormEvent,
  ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from 'react';
import { ChevronDown } from 'lucide-react';
import { useWorktreeProcesses } from '@/hooks/useWorktreeProcesses';
import {
  LaunchWorktreeCommandResponse,
  WorktreeProcessStatus,
  WorktreeProcessSummary,
} from '@/types';
import { apiUrl } from '@/lib/api';
import { cn } from '@/lib/utils';

interface WorktreeProcessesProps {
  worktreeId: string | null;
  worktreeName?: string | null;
  isCollapsed?: boolean;
  onToggleCollapsed?: () => void;
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
  const hasStdout = Boolean(process.stdout && process.stdout.length > 0);
  const hasStderr = Boolean(process.stderr && process.stderr.length > 0);
  const hasLogs = hasStdout || hasStderr;
  const [isExpanded, setIsExpanded] = useState(false);

  useEffect(() => {
    setIsExpanded(false);
  }, [process.id]);

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
      {hasLogs && (
        <div className="flex flex-col gap-2">
          <button
            type="button"
            className="self-start text-xs font-medium text-primary hover:underline"
            onClick={() => setIsExpanded((value) => !value)}
            aria-expanded={isExpanded}
          >
            {isExpanded ? 'Hide logs' : 'Show logs'}
          </button>
          {isExpanded && (
            <div className="flex flex-col gap-3">
              {hasStdout && (
                <LogViewer title="stdout" value={process.stdout ?? ''} />
              )}
              {hasStderr && (
                <LogViewer title="stderr" value={process.stderr ?? ''} variant="error" />
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function LogViewer({
  title,
  value,
  variant = 'default',
}: {
  title: string;
  value: string;
  variant?: 'default' | 'error';
}) {
  const trimmed = value.replace(/\s+$/u, '');
  const borderColor =
    variant === 'error'
      ? 'border-rose-200 bg-rose-50 text-rose-800'
      : 'border-muted bg-muted text-foreground';

  return (
    <div className="flex flex-col gap-1">
      <span className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
        {title}
      </span>
      <pre
        className={`max-h-48 overflow-auto rounded-md border px-3 py-2 text-xs font-mono leading-relaxed ${borderColor}`}
      >
        {trimmed || '(empty)'}
      </pre>
    </div>
  );
}

export default function WorktreeProcesses({
  worktreeId,
  worktreeName = null,
  isCollapsed = false,
  onToggleCollapsed,
}: WorktreeProcessesProps) {
  const { processes, isLoading, error, refetch } = useWorktreeProcesses(worktreeId);
  const [isFormOpen, setIsFormOpen] = useState(false);
  const [commandInput, setCommandInput] = useState('');
  const [descriptionInput, setDescriptionInput] = useState('');
  const [launchError, setLaunchError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [optimisticProcesses, setOptimisticProcesses] = useState<WorktreeProcessSummary[]>([]);

  useEffect(() => {
    setOptimisticProcesses([]);
    setIsFormOpen(false);
    setCommandInput('');
    setDescriptionInput('');
    setLaunchError(null);
  }, [worktreeId]);

  useEffect(() => {
    if (processes.length === 0) {
      return;
    }
    setOptimisticProcesses((current) =>
      current.filter((optimistic) => !processes.some((actual) => actual.id === optimistic.id)),
    );
  }, [processes]);

  const commandEndpoint = useMemo(() => {
    if (!worktreeId) {
      return null;
    }
    const encoded = encodeURIComponent(worktreeId);
    return `/api/worktrees/${encoded}/commands`;
  }, [worktreeId]);

  const handleLaunch = useCallback(async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!commandEndpoint) {
      return;
    }

    const trimmedCommand = commandInput.trim();
    if (!trimmedCommand) {
      setLaunchError('Command is required');
      return;
    }

    const trimmedDescription = descriptionInput.trim();

    setIsSubmitting(true);
    setLaunchError(null);

    try {
      const response = await fetch(apiUrl(commandEndpoint), {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          command: trimmedCommand,
          description: trimmedDescription ? trimmedDescription : undefined,
        }),
      });

      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(errorText || `Failed to launch command (status ${response.status})`);
      }

      const payload: LaunchWorktreeCommandResponse = await response.json();
      setOptimisticProcesses((current) => {
        const withoutDuplicate = current.filter((entry) => entry.id !== payload.process.id);
        return [payload.process, ...withoutDuplicate];
      });

      setCommandInput('');
      setDescriptionInput('');
      setIsFormOpen(false);
      void refetch();
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to launch command';
      setLaunchError(message);
    } finally {
      setIsSubmitting(false);
    }
  }, [commandEndpoint, commandInput, descriptionInput, refetch]);

  const handleCancelLaunch = useCallback(() => {
    setIsFormOpen(false);
    setLaunchError(null);
  }, []);

  const handleToggleForm = useCallback(() => {
    setIsFormOpen((prev) => !prev);
    setLaunchError(null);
  }, []);

  const displayProcesses = useMemo(() => {
    const merged = [...optimisticProcesses, ...processes];
    const seen = new Set<string>();
    const unique = merged.filter((process) => {
      if (seen.has(process.id)) {
        return false;
      }
      seen.add(process.id);
      return true;
    });

    const extractTimestamp = (process: WorktreeProcessSummary) => {
      const candidate = process.started_at ?? process.finished_at ?? null;
      if (!candidate) {
        return 0;
      }
      const epoch = new Date(candidate).getTime();
      return Number.isNaN(epoch) ? 0 : epoch;
    };

    return unique.sort((a, b) => extractTimestamp(b) - extractTimestamp(a));
  }, [optimisticProcesses, processes]);

  const showLoadingState = isLoading && displayProcesses.length === 0;
  const showEmptyState = !isLoading && displayProcesses.length === 0;
  const panelId = 'worktree-processes-panel';

  const handleCollapseToggle = useCallback(() => {
    onToggleCollapsed?.();
  }, [onToggleCollapsed]);

  let bodyContent: ReactNode;

  if (!worktreeId) {
    bodyContent = (
      <div className="flex flex-1 items-center justify-center px-6 text-center text-sm text-muted-foreground">
        <div>
          <p className="font-medium text-foreground">Select a worktree to view running commands</p>
          <p className="mt-2 text-xs text-muted-foreground">
            Active processes launched via <code className="rounded bg-muted px-1 py-0.5">agentdev worktree exec</code> will show up here.
          </p>
        </div>
      </div>
    );
  } else if (error) {
    bodyContent = (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 px-6 text-center">
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
  } else {
    bodyContent = (
      <>
        {isFormOpen && (
          <form
            onSubmit={handleLaunch}
            className="flex flex-col gap-3 rounded-md border border-dashed border-border bg-muted/40 p-4"
          >
            <label className="flex flex-col gap-2 text-xs font-medium text-muted-foreground">
              Command
              <input
                type="text"
                value={commandInput}
                onChange={(event) => setCommandInput(event.target.value)}
                placeholder="pnpm dev"
                className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground shadow-sm focus:outline-none focus:ring-2 focus:ring-primary"
                autoFocus
              />
            </label>
            <label className="flex flex-col gap-2 text-xs font-medium text-muted-foreground">
              Description (optional)
              <input
                type="text"
                value={descriptionInput}
                onChange={(event) => setDescriptionInput(event.target.value)}
                placeholder="Launched from dashboard"
                className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground shadow-sm focus:outline-none focus:ring-2 focus:ring-primary/40"
              />
            </label>
            {launchError && (
              <p className="text-xs text-red-600">{launchError}</p>
            )}
            <div className="flex items-center gap-2">
              <button
                type="submit"
                className="rounded-md bg-primary px-3 py-2 text-xs font-semibold text-primary-foreground shadow-sm disabled:opacity-60"
                disabled={isSubmitting || commandInput.trim().length === 0}
              >
                {isSubmitting ? 'Launching…' : 'Launch'}
              </button>
              <button
                type="button"
                onClick={handleCancelLaunch}
                className="rounded-md border border-border bg-background px-3 py-2 text-xs font-medium text-foreground hover:bg-muted disabled:opacity-50"
                disabled={isSubmitting}
              >
                Cancel
              </button>
            </div>
          </form>
        )}

        {showLoadingState && (
          <div className="flex flex-1 items-center justify-center px-6">
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <div className="inline-block h-5 w-5 animate-spin rounded-full border-2 border-muted border-t-primary" />
              <span>Loading processes…</span>
            </div>
          </div>
        )}

        {showEmptyState && !showLoadingState && (
          <div className="flex flex-1 flex-col items-center justify-center gap-3 px-6 text-center text-sm text-muted-foreground">
            <div>
              <p className="font-medium text-foreground">
                No commands recorded for {worktreeName ?? 'this worktree'} yet
              </p>
              <p className="mt-2 text-xs text-muted-foreground">
                Launch a command here or run{' '}
                <code className="rounded bg-muted px-1 py-0.5">agentdev worktree exec</code> in the terminal to see it listed.
              </p>
            </div>
          </div>
        )}

        {!showEmptyState && !showLoadingState && displayProcesses.length > 0 && (
          <div className="space-y-3 pb-4">
            {displayProcesses.map((process) => (
              <ProcessCard key={process.id} process={process} />
            ))}
          </div>
        )}
      </>
    );
  }

  return (
    <div
      className={cn(
        'flex h-full flex-col transition-[padding] duration-200 ease-in-out',
        isCollapsed ? 'gap-2 px-4 py-2' : 'gap-3 px-6 py-4',
      )}
    >
      <div
        className={cn(
          'flex items-center gap-2',
          isCollapsed ? 'justify-between' : 'flex-wrap justify-between',
        )}
      >
        <button
          type="button"
          onClick={handleCollapseToggle}
          aria-controls={panelId}
          aria-expanded={!isCollapsed}
          className="flex min-w-0 flex-1 flex-col items-start gap-1 rounded-md border border-transparent bg-transparent text-left text-sm font-semibold text-foreground transition-colors hover:border-border hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
        >
          <span className="flex w-full items-center gap-2">
            <ChevronDown
              className={cn(
                'h-4 w-4 shrink-0 transition-transform duration-200',
                !isCollapsed ? '-rotate-180' : 'rotate-0',
              )}
              aria-hidden="true"
            />
            <span className="truncate">
              Commands for {worktreeName ?? 'selected worktree'}
            </span>
          </span>
          {!isCollapsed ? (
            <span className="text-xs font-normal text-muted-foreground">
              Launch commands directly from the dashboard. Data refreshes automatically every 5 seconds.
            </span>
          ) : null}
        </button>
        {!isCollapsed ? (
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={handleToggleForm}
              className="rounded-md border border-border bg-background px-3 py-2 text-xs font-medium text-foreground hover:bg-muted disabled:opacity-50"
              disabled={commandEndpoint == null}
            >
              {isFormOpen ? 'Hide form' : 'Run command'}
            </button>
            <button
              type="button"
              onClick={() => refetch()}
              className="rounded-md border border-border bg-background px-3 py-2 text-xs font-medium text-foreground hover:bg-muted disabled:opacity-50"
              disabled={isLoading}
            >
              Refresh
            </button>
          </div>
        ) : null}
      </div>

      <div
        id={panelId}
        className={cn(
          'flex-1',
          isCollapsed ? 'hidden' : 'flex flex-col gap-3 overflow-y-auto',
        )}
        aria-hidden={isCollapsed}
      >
        {bodyContent}
      </div>
    </div>
  );
}
