'use client';

import {
  FormEvent,
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
} from 'react';
import clsx from 'clsx';
import type { MergeStrategyOption, WorktreeSummary } from '@/types';
import WorktreeGitSection from './WorktreeGitSection';
import WorktreeSessions from './WorktreeSessions';
import { useLaunchWorktreeCommand } from '@/features/command/hooks/useLaunchWorktreeCommand';
import { useLaunchWorktreeShell } from '@/features/command/hooks/useLaunchWorktreeShell';
import { useMergeWorktree } from '@/hooks/useMergeWorktree';
import { useDeleteWorktree } from '@/hooks/useDeleteWorktree';
import { ApiError } from '@/lib/apiClient';

interface WorktreeDetailsProps {
  worktree: WorktreeSummary | null;
  isLoading: boolean;
}

type FeedbackMessage = {
  type: 'success' | 'error';
  message: string;
};

const MERGE_STRATEGIES: Array<{
  value: MergeStrategyOption;
  label: string;
  description: string;
}> = [
  {
    value: 'ff-only',
    label: 'Fast-forward',
    description: 'Fails if the branch diverged from default. Keeps history linear.',
  },
  {
    value: 'merge',
    label: 'Merge commit',
    description: 'Creates a merge commit even if fast-forwarding is possible.',
  },
  {
    value: 'squash',
    label: 'Squash',
    description: 'Squashes branch commits into a single commit on the default branch.',
  },
];

function getStrategyLabel(value: MergeStrategyOption): string {
  const match = MERGE_STRATEGIES.find((item) => item.value === value);
  return match ? match.label : value;
}

function toActionErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof ApiError) {
    const body = error.body;
    if (body && typeof body === 'object' && 'message' in body) {
      const message = (body as { message?: unknown }).message;
      if (typeof message === 'string' && message.trim().length > 0) {
        return message.trim();
      }
    }
    return error.message || fallback;
  }
  if (error instanceof Error) {
    return error.message || fallback;
  }
  if (typeof error === 'string') {
    return error.trim() || fallback;
  }
  return fallback;
}

function formatTimestamp(value?: string | null) {
  if (!value) {
    return 'unknown';
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return 'unknown';
  }
  const diffMs = Date.now() - date.getTime();
  const diffStr = formatRelativeTime(diffMs);
  return `${date.toLocaleString()} (${diffStr})`;
}

function formatRelativeTime(diffMs: number) {
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

export default function WorktreeDetails({
  worktree,
  isLoading,
}: WorktreeDetailsProps) {
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const [isLaunchingVsCode, setIsLaunchingVsCode] = useState(false);
  const [isRunCommandDialogOpen, setIsRunCommandDialogOpen] = useState(false);
  const [runCommandInput, setRunCommandInput] = useState('');
  const [runCommandError, setRunCommandError] = useState<string | null>(null);
  const [actionFeedback, setActionFeedback] = useState<FeedbackMessage | null>(null);
  const [activePanel, setActivePanel] = useState<'sessions' | 'git'>('sessions');
  const [isMergeDialogOpen, setIsMergeDialogOpen] = useState(false);
  const [mergeStrategy, setMergeStrategy] = useState<MergeStrategyOption>('ff-only');
  const [mergePushEnabled, setMergePushEnabled] = useState(false);
  const [mergeCleanupEnabled, setMergeCleanupEnabled] = useState(false);
  const [mergeError, setMergeError] = useState<string | null>(null);
  const [isDeleteDialogOpen, setIsDeleteDialogOpen] = useState(false);
  const [forceDelete, setForceDelete] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const { mutateAsync: launchWorktreeCommand, reset: resetLaunchCommand } =
    useLaunchWorktreeCommand();
  const {
    mutateAsync: launchWorktreeShell,
    isPending: isLaunchingShell,
    reset: resetLaunchShell,
  } = useLaunchWorktreeShell();
  const {
    mutateAsync: mergeWorktree,
    reset: resetMerge,
    isPending: isMerging,
  } = useMergeWorktree();
  const {
    mutateAsync: deleteWorktree,
    reset: resetDelete,
    isPending: isDeleting,
  } = useDeleteWorktree();

  const mergeDialogTitleId = useId();
  const mergeDialogDescriptionId = useId();
  const mergePushCheckboxId = useId();
  const mergeCleanupCheckboxId = useId();
  const deleteDialogTitleId = useId();
  const deleteDialogDescriptionId = useId();
  const deleteForceCheckboxId = useId();
  const runCommandDialogTitleId = useId();
  const runCommandDialogDescriptionId = useId();

  const hasWorktree = Boolean(worktree?.id);
  const selectedMergeStrategy = MERGE_STRATEGIES.find(
    (item) => item.value === mergeStrategy,
  );

  const closeMergeDialog = useCallback(() => {
    setIsMergeDialogOpen(false);
    setMergeError(null);
    setMergeStrategy('ff-only');
    setMergePushEnabled(false);
    setMergeCleanupEnabled(false);
    resetMerge();
  }, [resetMerge]);

  const closeDeleteDialog = useCallback(() => {
    setIsDeleteDialogOpen(false);
    setDeleteError(null);
    setForceDelete(false);
    resetDelete();
  }, [resetDelete]);

  const openMergeDialog = useCallback(() => {
    if (!worktree?.id) {
      return;
    }
    setMergeError(null);
    resetMerge();
    setIsMergeDialogOpen(true);
  }, [resetMerge, worktree?.id]);

  const openDeleteDialog = useCallback(() => {
    if (!worktree?.id) {
      return;
    }
    setDeleteError(null);
    resetDelete();
    setIsDeleteDialogOpen(true);
  }, [resetDelete, worktree?.id]);

  const submitMerge = useCallback(async () => {
    if (!worktree?.id || !worktree?.name) {
      return;
    }
    setMergeError(null);
    try {
      await mergeWorktree({
        worktreeId: worktree.id,
        strategy: mergeStrategy,
        push: mergePushEnabled,
        cleanup: mergeCleanupEnabled,
      });
      const strategyLabel = getStrategyLabel(mergeStrategy);
      setActionFeedback({
        type: 'success',
        message: `Merge requested for ${worktree.name} (${strategyLabel})`,
      });
      closeMergeDialog();
    } catch (error) {
      setMergeError(toActionErrorMessage(error, 'Failed to merge worktree'));
    }
  }, [
    closeMergeDialog,
    mergeCleanupEnabled,
    mergePushEnabled,
    mergeStrategy,
    mergeWorktree,
    worktree?.id,
    worktree?.name,
  ]);

  const submitDelete = useCallback(async () => {
    if (!worktree?.id || !worktree?.name) {
      return;
    }
    setDeleteError(null);
    try {
      await deleteWorktree({
        worktreeId: worktree.id,
        force: forceDelete,
      });
      setActionFeedback({
        type: 'success',
        message: `Deletion requested for ${worktree.name}`,
      });
      closeDeleteDialog();
    } catch (error) {
      setDeleteError(toActionErrorMessage(error, 'Failed to delete worktree'));
    }
  }, [closeDeleteDialog, deleteWorktree, forceDelete, worktree?.id, worktree?.name]);

  useEffect(() => {
    setIsLaunchingVsCode(false);
    setIsRunCommandDialogOpen(false);
    setRunCommandInput('');
    setRunCommandError(null);
    setActionFeedback(null);
    resetLaunchCommand();
    resetLaunchShell();
    closeMergeDialog();
    closeDeleteDialog();
  }, [
    closeDeleteDialog,
    closeMergeDialog,
    resetLaunchShell,
    resetLaunchCommand,
    worktree?.id,
  ]);

  useEffect(() => {
    setActivePanel('sessions');
  }, [worktree?.id]);

  useEffect(() => {
    if (!actionFeedback || actionFeedback.type !== 'success') {
      return;
    }
    const timer = window.setTimeout(() => {
      setActionFeedback(null);
    }, 4000);
    return () => window.clearTimeout(timer);
  }, [actionFeedback]);

  const handleOpenVsCode = useCallback(async () => {
    const worktreeId = worktree?.id;
    if (!worktreeId) {
      return;
    }

    setIsLaunchingVsCode(true);
    setActionFeedback(null);

    try {
      await launchWorktreeCommand({
        worktreeId,
        command: 'code .',
        description: 'Open worktree in VSCode',
      });
      setActionFeedback({
        type: 'success',
        message: 'VSCode launch requested. Check Processes for status.',
      });
    } catch (err) {
      const message =
        err instanceof Error ? err.message : 'Failed to open in VSCode';
      setActionFeedback({
        type: 'error',
        message,
      });
    } finally {
      setIsLaunchingVsCode(false);
    }
  }, [launchWorktreeCommand, worktree?.id]);

  const summarizeCommand = useCallback((value: string) => {
    const maxLength = 80;
    const trimmed = value.trim();
    if (trimmed.length <= maxLength) {
      return trimmed;
    }
    return `${trimmed.slice(0, maxLength - 1)}…`;
  }, []);

  const handleOpenShell = useCallback(async () => {
    const worktreeId = worktree?.id;
    if (!worktreeId) {
      return;
    }

    setActionFeedback(null);

    try {
      await launchWorktreeShell({
        type: 'worktree',
        worktreeId,
      });
      setActionFeedback({
        type: 'success',
        message: 'Shell launch requested. Check your terminal.',
      });
    } catch (error) {
      setActionFeedback({
        type: 'error',
        message: toActionErrorMessage(error, 'Failed to open shell'),
      });
    }
  }, [launchWorktreeShell, worktree?.id]);

  const openRunCommandDialog = useCallback(() => {
    if (!worktree?.id) {
      return;
    }
    setRunCommandInput('');
    setRunCommandError(null);
    setIsRunCommandDialogOpen(true);
  }, [worktree?.id]);

  const closeRunCommandDialog = useCallback(() => {
    setIsRunCommandDialogOpen(false);
    setRunCommandError(null);
    setRunCommandInput('');
  }, []);

  const handleRunCommandSubmit = useCallback(async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!worktree?.id) {
      return;
    }
    const trimmed = runCommandInput.trim();
    if (!trimmed) {
      setRunCommandError('Command is required');
      return;
    }

    setRunCommandError(null);
    setActionFeedback(null);

    try {
      await launchWorktreeShell({
        type: 'worktree',
        worktreeId: worktree.id,
        command: trimmed,
      });
      setActionFeedback({
        type: 'success',
        message: `Launching "${summarizeCommand(trimmed)}" in shell.`,
      });
      setIsRunCommandDialogOpen(false);
      setRunCommandInput('');
    } catch (error) {
      setRunCommandError(toActionErrorMessage(error, 'Failed to run command in shell'));
    }
  }, [launchWorktreeShell, runCommandInput, summarizeCommand, worktree?.id]);

  if (!worktree) {
    return (
      <div className="h-full flex items-center justify-center">
        {isLoading ? (
          <div className="flex items-center space-x-2 text-gray-500">
            <div className="inline-block w-5 h-5 border-2 border-gray-300 border-t-blue-500 rounded-full animate-spin" />
            <span>Loading worktrees...</span>
          </div>
        ) : (
          <div className="text-center text-gray-500">
            <p className="text-sm font-medium">Select a worktree to inspect</p>
            <p className="text-xs mt-2">
              Git status, recent commits, and session summaries will appear here.
            </p>
          </div>
        )}
      </div>
    );
  }

  const status = worktree.git_status ?? undefined;
  const commit = worktree.head_commit ?? undefined;
  const commitsAhead = worktree.commits_ahead ?? undefined;
  const sessionCount = worktree.sessions.length;
  const diffEstimate =
    status !== undefined
      ? status.staged + status.unstaged + status.untracked
      : null;
  const sessionTabLabel = `Sessions (${sessionCount})`;
  const diffTabLabel =
    diffEstimate !== null ? `Git Diff (${diffEstimate})` : 'Git Diff';
  const openShellLabel =
    isLaunchingShell && !isRunCommandDialogOpen ? 'Opening shell…' : 'Open shell';
  const runCommandSubmitLabel = isLaunchingShell ? 'Launching…' : 'Run';

  const overviewCards = [
    {
      label: 'Path',
      value: worktree.path,
      monospace: true,
    },
    {
      label: 'Repo / Branch',
      value: `${worktree.repo_name}/${worktree.branch}`,
      monospace: true,
    },
    {
      label: 'Last Activity',
      value: formatTimestamp(worktree.last_activity_at),
    },
    {
      label: 'Created',
      value: formatTimestamp(worktree.created_at),
    },
    worktree.agent_alias
      ? {
          label: 'Default Agent',
          value: worktree.agent_alias,
        }
      : null,
  ].filter(Boolean) as Array<{ label: string; value: string; monospace?: boolean }>;

  const renderOverview = () => (
    <div className="space-y-6">
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {overviewCards.map((card) => (
          <div
            key={card.label}
            className="flex flex-wrap items-baseline gap-x-2 gap-y-1"
          >
            <span className="text-xs font-medium uppercase tracking-wide text-gray-500">
              {card.label}
            </span>
            <span
              className={`text-sm text-gray-900 ${
                card.monospace ? 'font-mono break-all' : ''
              }`}
            >
              {card.value}
            </span>
          </div>
        ))}
      </div>

      {worktree.initial_prompt && (
        <section className="rounded-lg border border-gray-200 bg-white px-4 py-4">
          <div className="flex items-center justify-between gap-4">
            <h3 className="text-sm font-semibold uppercase tracking-wide text-gray-900">
              Initial Prompt
            </h3>
            <span className="text-xs text-gray-400">
              Captured from `agentdev start`
            </span>
          </div>
          <pre className="mt-3 whitespace-pre-wrap text-sm text-gray-800">
            {worktree.initial_prompt}
          </pre>
        </section>
      )}
    </div>
  );

  return (
    <>
      <div ref={scrollContainerRef} className="h-full overflow-y-auto">
        <div className="mx-auto flex max-w-5xl flex-col gap-6 px-6 py-6">
          <section className="rounded-lg border border-gray-200 bg-white px-6 py-5 shadow-sm">
            <div className="flex flex-wrap items-start justify-between gap-4">
              <div>
                <p className="text-xs uppercase tracking-wide text-gray-500">Worktree</p>
                <h2 className="text-xl font-semibold text-gray-900">{worktree.name}</h2>
                <p className="mt-1 text-sm text-gray-500">
                  Managed as <span className="font-mono">{worktree.id}</span>
                </p>
              </div>
              <div className="flex flex-wrap gap-2">
                <button
                  type="button"
                  onClick={handleOpenVsCode}
                  disabled={!worktree?.id || isLaunchingVsCode}
                  className="rounded-md bg-blue-600 px-3 py-2 text-sm font-semibold text-white shadow-sm hover:bg-blue-500 disabled:opacity-60"
                >
                  {isLaunchingVsCode ? 'Opening…' : 'Open in VSCode'}
                </button>
                <button
                  type="button"
                  onClick={handleOpenShell}
                  disabled={!hasWorktree || isLaunchingShell}
                  className="rounded-md border border-gray-200 bg-white px-3 py-2 text-sm text-gray-600 transition hover:border-gray-300 hover:text-gray-800 disabled:opacity-60"
                >
                  {openShellLabel}
                </button>
                <button
                  type="button"
                  onClick={openRunCommandDialog}
                  disabled={!hasWorktree || isLaunchingShell}
                  className="rounded-md border border-gray-200 bg-white px-3 py-2 text-sm text-gray-600 transition hover:border-gray-300 hover:text-gray-800 disabled:opacity-60"
                >
                  Run command
                </button>
                <button
                  type="button"
                  onClick={openMergeDialog}
                  disabled={!hasWorktree || isMerging}
                  className="rounded-md border border-gray-200 bg-white px-3 py-2 text-sm text-gray-600 transition hover:border-gray-300 hover:text-gray-800 disabled:opacity-60"
                >
                  {isMerging ? 'Merging…' : 'Merge'}
                </button>
                <button
                  type="button"
                  onClick={openDeleteDialog}
                  disabled={!hasWorktree || isDeleting}
                  className="rounded-md border border-rose-200 bg-white px-3 py-2 text-sm text-rose-600 transition hover:border-rose-300 hover:text-rose-700 disabled:opacity-60"
                >
                  {isDeleting ? 'Deleting…' : 'Delete'}
                </button>
              </div>
            </div>
            {actionFeedback && (
              <div
                className={`mt-4 rounded-md border px-3 py-2 text-xs ${
                  actionFeedback.type === 'success'
                    ? 'border-green-200 bg-green-50 text-green-700'
                    : 'border-rose-200 bg-rose-50 text-rose-700'
                }`}
              >
                {actionFeedback.message}
              </div>
            )}
          </section>

          <div className="space-y-6 pb-12">
            {renderOverview()}

          <section className="rounded-lg border border-gray-200 bg-white px-4 py-4 shadow-sm">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <h3 className="text-sm font-semibold uppercase tracking-wide text-gray-900">
                  Worktree Activity
                </h3>
                <p className="text-xs text-gray-500">
                  Switch between conversation history and git diff insights.
                </p>
              </div>
              <div className="flex items-center gap-3 text-xs text-gray-400">
                <span>{sessionCount} sessions</span>
                {diffEstimate !== null && <span>{diffEstimate} changes</span>}
              </div>
            </div>

            <div className="mt-4 flex flex-wrap items-center gap-2">
              <button
                type="button"
                onClick={() => setActivePanel('sessions')}
                className={clsx(
                  'rounded-md border px-3 py-1.5 text-xs font-medium transition',
                  activePanel === 'sessions'
                    ? 'border-blue-500 bg-blue-500/10 text-blue-600 shadow-sm'
                    : 'border-gray-200 text-gray-600 hover:border-gray-300 hover:text-gray-800',
                )}
              >
                {sessionTabLabel}
              </button>
              <button
                type="button"
                onClick={() => setActivePanel('git')}
                className={clsx(
                  'rounded-md border px-3 py-1.5 text-xs font-medium transition',
                  activePanel === 'git'
                    ? 'border-blue-500 bg-blue-500/10 text-blue-600 shadow-sm'
                    : 'border-gray-200 text-gray-600 hover:border-gray-300 hover:text-gray-800',
                )}
              >
                {diffTabLabel}
              </button>
            </div>
          </section>

          {activePanel === 'sessions' ? (
            <WorktreeSessions
              sessions={worktree.sessions}
              formatTimestamp={formatTimestamp}
              worktreeId={worktree.id}
            />
          ) : (
            <WorktreeGitSection
              worktreeId={worktree.id}
              status={status}
              commit={commit}
              commitsAhead={commitsAhead}
              formatTimestamp={formatTimestamp}
              defaultExpanded
              scrollContainerRef={scrollContainerRef}
            />
          )}
        </div>
        </div>
      </div>

      {isRunCommandDialogOpen && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 px-4"
          onClick={closeRunCommandDialog}
        >
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby={runCommandDialogTitleId}
            aria-describedby={runCommandDialogDescriptionId}
            className="w-full max-w-md rounded-lg border border-gray-200 bg-white shadow-xl"
            onClick={(event) => event.stopPropagation()}
          >
            <form onSubmit={handleRunCommandSubmit} className="space-y-4">
              <div className="border-b border-gray-200 px-5 py-4">
                <h3 id={runCommandDialogTitleId} className="text-base font-semibold text-gray-900">
                  Run command in shell
                </h3>
                <p id={runCommandDialogDescriptionId} className="mt-1 text-sm text-gray-500">
                  The command runs in a new terminal window rooted at this worktree.
                </p>
              </div>
              <div className="flex flex-col gap-4 px-5 pb-5">
                <label className="flex flex-col gap-2 text-xs font-medium text-gray-600">
                  Command
                  <input
                    type="text"
                    value={runCommandInput}
                    onChange={(event) => setRunCommandInput(event.target.value)}
                    placeholder="pnpm dev"
                    className="w-full rounded-md border border-gray-300 bg-white px-3 py-2 text-sm text-gray-900 shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                    autoFocus
                  />
                </label>
                {runCommandError && (
                  <div className="rounded-md border border-rose-200 bg-rose-50 px-3 py-2 text-xs text-rose-700">
                    {runCommandError}
                  </div>
                )}
                <div className="flex items-center justify-end gap-2">
                  <button
                    type="button"
                    onClick={closeRunCommandDialog}
                    className="rounded-md border border-gray-300 px-3 py-2 text-sm text-gray-700 hover:bg-gray-100 disabled:opacity-60"
                    disabled={isLaunchingShell}
                  >
                    Cancel
                  </button>
                  <button
                    type="submit"
                    className="rounded-md bg-blue-600 px-3 py-2 text-sm font-semibold text-white shadow-sm hover:bg-blue-500 disabled:opacity-60"
                    disabled={isLaunchingShell || runCommandInput.trim().length === 0}
                  >
                    {runCommandSubmitLabel}
                  </button>
                </div>
              </div>
            </form>
          </div>
        </div>
      )}

      {isMergeDialogOpen && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 px-4"
          onClick={closeMergeDialog}
        >
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby={mergeDialogTitleId}
            aria-describedby={mergeDialogDescriptionId}
            className="w-full max-w-md rounded-lg border border-gray-200 bg-white shadow-xl"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="border-b border-gray-200 px-5 py-4">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <h3 id={mergeDialogTitleId} className="text-base font-semibold text-gray-900">
                    Merge worktree
                  </h3>
                  <p
                    id={mergeDialogDescriptionId}
                    className="mt-1 text-sm text-gray-500"
                  >
                    Merge <span className="font-medium text-gray-900">{worktree.name}</span> into the default branch.
                  </p>
                </div>
                <button
                  type="button"
                  onClick={closeMergeDialog}
                  className="rounded-md border border-transparent px-2 py-1 text-sm text-gray-500 hover:text-gray-700"
                >
                  Close
                </button>
              </div>
            </div>
            <form
              onSubmit={(event) => {
                event.preventDefault();
                void submitMerge();
              }}
            >
              <div className="space-y-5 px-5 py-4">
                <div>
                  <label
                    htmlFor={`${mergeDialogTitleId}-strategy`}
                    className="text-sm font-medium text-gray-900"
                  >
                    Strategy
                  </label>
                  <select
                    id={`${mergeDialogTitleId}-strategy`}
                    className="mt-2 w-full rounded-md border border-gray-300 bg-white px-3 py-2 text-sm text-gray-900 shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                    value={mergeStrategy}
                    onChange={(event) =>
                      setMergeStrategy(event.target.value as MergeStrategyOption)
                    }
                  >
                    {MERGE_STRATEGIES.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                  {selectedMergeStrategy && (
                    <p className="mt-2 text-xs text-gray-500">
                      {selectedMergeStrategy.description}
                    </p>
                  )}
                </div>

                <div className="space-y-3">
                  <label htmlFor={mergePushCheckboxId} className="flex items-start gap-3">
                    <input
                      id={mergePushCheckboxId}
                      type="checkbox"
                      className="mt-1 h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
                      checked={mergePushEnabled}
                      onChange={(event) => setMergePushEnabled(event.target.checked)}
                    />
                    <span>
                      <span className="text-sm font-medium text-gray-900">Push after merge</span>
                      <span className="mt-0.5 block text-xs text-gray-500">
                        Runs <code className="font-mono">git push</code> on the default branch when the merge succeeds.
                      </span>
                    </span>
                  </label>

                  <label htmlFor={mergeCleanupCheckboxId} className="flex items-start gap-3">
                    <input
                      id={mergeCleanupCheckboxId}
                      type="checkbox"
                      className="mt-1 h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
                      checked={mergeCleanupEnabled}
                      onChange={(event) =>
                        setMergeCleanupEnabled(event.target.checked)
                      }
                    />
                    <span>
                      <span className="text-sm font-medium text-gray-900">Delete worktree after merge</span>
                      <span className="mt-0.5 block text-xs text-gray-500">
                        Attempts to delete the worktree once the merge completes. Use when the branch is ready to remove.
                      </span>
                    </span>
                  </label>
                </div>

                {mergeError && (
                  <div className="rounded-md border border-rose-200 bg-rose-50 px-3 py-2 text-xs text-rose-700">
                    {mergeError}
                  </div>
                )}
              </div>

              <div className="flex items-center justify-end gap-2 border-t border-gray-200 bg-gray-50 px-5 py-3">
                <button
                  type="button"
                  onClick={closeMergeDialog}
                  className="rounded-md border border-gray-300 px-3 py-2 text-sm text-gray-700 hover:bg-gray-100"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={isMerging}
                  className="rounded-md bg-blue-600 px-3 py-2 text-sm font-semibold text-white shadow-sm hover:bg-blue-500 disabled:opacity-60"
                >
                  {isMerging ? 'Merging…' : 'Confirm merge'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {isDeleteDialogOpen && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 px-4"
          onClick={closeDeleteDialog}
        >
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby={deleteDialogTitleId}
            aria-describedby={deleteDialogDescriptionId}
            className="w-full max-w-md rounded-lg border border-gray-200 bg-white shadow-xl"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="border-b border-gray-200 px-5 py-4">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <h3 id={deleteDialogTitleId} className="text-base font-semibold text-gray-900">
                    Delete worktree
                  </h3>
                  <p
                    id={deleteDialogDescriptionId}
                    className="mt-1 text-sm text-gray-500"
                  >
                    Remove <span className="font-medium text-gray-900">{worktree.name}</span> from disk and from AgentDev tracking.
                  </p>
                </div>
                <button
                  type="button"
                  onClick={closeDeleteDialog}
                  className="rounded-md border border-transparent px-2 py-1 text-sm text-gray-500 hover:text-gray-700"
                >
                  Close
                </button>
              </div>
            </div>
            <form
              onSubmit={(event) => {
                event.preventDefault();
                void submitDelete();
              }}
            >
              <div className="space-y-4 px-5 py-4">
                <p className="text-sm text-gray-600">
                  The worktree directory and its branch will be removed if it is fully merged.
                  Pending changes or unmerged commits may prevent deletion unless forced.
                </p>

                <label htmlFor={deleteForceCheckboxId} className="flex items-start gap-3">
                  <input
                    id={deleteForceCheckboxId}
                    type="checkbox"
                    className="mt-1 h-4 w-4 rounded border-gray-300 text-rose-600 focus:ring-rose-500"
                    checked={forceDelete}
                    onChange={(event) => setForceDelete(event.target.checked)}
                  />
                  <span>
                    <span className="text-sm font-medium text-gray-900">Force delete even if there is pending work</span>
                    <span className="mt-0.5 block text-xs text-gray-500">
                      Answers all confirmations for you. The git branch is kept if it is not fully merged.
                    </span>
                  </span>
                </label>

                {deleteError && (
                  <div className="rounded-md border border-rose-200 bg-rose-50 px-3 py-2 text-xs text-rose-700">
                    {deleteError}
                  </div>
                )}
              </div>

              <div className="flex items-center justify-end gap-2 border-t border-gray-200 bg-gray-50 px-5 py-3">
                <button
                  type="button"
                  onClick={closeDeleteDialog}
                  className="rounded-md border border-gray-300 px-3 py-2 text-sm text-gray-700 hover:bg-gray-100"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={isDeleting}
                  className="rounded-md bg-rose-600 px-3 py-2 text-sm font-semibold text-white shadow-sm hover:bg-rose-500 disabled:opacity-60"
                >
                  {isDeleting ? 'Deleting…' : 'Confirm delete'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </>
  );
}
