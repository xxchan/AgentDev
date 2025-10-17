'use client';

import {
  useCallback,
  useEffect,
  useMemo,
  useState,
} from 'react';
import clsx from 'clsx';
import { apiUrl } from '@/lib/api';
import { WorktreeSummary } from '@/types';
import WorktreeGitSection from './WorktreeGitSection';
import WorktreeSessions from './WorktreeSessions';

interface WorktreeDetailsProps {
  worktree: WorktreeSummary | null;
  isLoading: boolean;
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
  const commandEndpoint = useMemo(() => {
    if (!worktree) {
      return null;
    }
    return `/api/worktrees/${encodeURIComponent(worktree.id)}/commands`;
  }, [worktree]);

  const [isLaunchingVsCode, setIsLaunchingVsCode] = useState(false);
  const [vsCodeFeedback, setVsCodeFeedback] = useState<{
    type: 'success' | 'error';
    message: string;
  } | null>(null);
  const [activePanel, setActivePanel] = useState<'sessions' | 'git'>('sessions');

  useEffect(() => {
    setIsLaunchingVsCode(false);
    setVsCodeFeedback(null);
  }, [commandEndpoint]);

  useEffect(() => {
    setActivePanel('sessions');
  }, [worktree?.id]);

  useEffect(() => {
    if (!vsCodeFeedback || vsCodeFeedback.type !== 'success') {
      return;
    }
    const timer = window.setTimeout(() => {
      setVsCodeFeedback(null);
    }, 4000);
    return () => window.clearTimeout(timer);
  }, [vsCodeFeedback]);

  const handleOpenVsCode = useCallback(async () => {
    if (!commandEndpoint) {
      return;
    }

    setIsLaunchingVsCode(true);
    setVsCodeFeedback(null);

    try {
      const response = await fetch(apiUrl(commandEndpoint), {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          command: 'code .',
          description: 'Open worktree in VSCode',
        }),
      });

      if (!response.ok) {
        const message = await response.text();
        throw new Error(
          message || `Failed to launch VSCode (status ${response.status})`,
        );
      }

      setVsCodeFeedback({
        type: 'success',
        message: 'VSCode launch requested. Check Processes for status.',
      });
    } catch (err) {
      const message =
        err instanceof Error ? err.message : 'Failed to open in VSCode';
      setVsCodeFeedback({
        type: 'error',
        message,
      });
    } finally {
      setIsLaunchingVsCode(false);
    }
  }, [commandEndpoint]);

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
    <div className="h-full overflow-y-auto">
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
                disabled={!commandEndpoint || isLaunchingVsCode}
                className="rounded-md bg-blue-600 px-3 py-2 text-sm font-semibold text-white shadow-sm hover:bg-blue-500 disabled:opacity-60"
              >
                {isLaunchingVsCode ? 'Opening…' : 'Open in VSCode'}
              </button>
              <button
                type="button"
                disabled
                title="Launching shell is coming soon"
                className="rounded-md border border-gray-200 bg-white px-3 py-2 text-sm text-gray-400"
              >
                Open shell
              </button>
              <button
                type="button"
                disabled
                title="Command runner is coming soon"
                className="rounded-md border border-gray-200 bg-white px-3 py-2 text-sm text-gray-400"
              >
                Run command
              </button>
              <button
                type="button"
                disabled
                title="Merge from dashboard is coming soon"
                className="rounded-md border border-gray-200 bg-white px-3 py-2 text-sm text-gray-400"
              >
                Merge
              </button>
              <button
                type="button"
                disabled
                title="Deletion flow is coming soon"
                className="rounded-md border border-gray-200 bg-white px-3 py-2 text-sm text-gray-400"
              >
                Delete
              </button>
            </div>
          </div>
          {vsCodeFeedback && (
            <div
              className={`mt-4 rounded-md border px-3 py-2 text-xs ${
                vsCodeFeedback.type === 'success'
                  ? 'border-green-200 bg-green-50 text-green-700'
                  : 'border-rose-200 bg-rose-50 text-rose-700'
              }`}
            >
              {vsCodeFeedback.message}
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
            />
          ) : (
            <WorktreeGitSection
              worktreeId={worktree.id}
              status={status}
              commit={commit}
              commitsAhead={commitsAhead}
              formatTimestamp={formatTimestamp}
              defaultExpanded
            />
          )}
        </div>
      </div>
    </div>
  );
}
