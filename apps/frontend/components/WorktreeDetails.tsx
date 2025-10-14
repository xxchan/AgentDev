'use client';

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
  const diffMonths = Math.floor(diffDays / 30);
  return `${diffMonths}mo ago`;
}

export default function WorktreeDetails({
  worktree,
  isLoading,
}: WorktreeDetailsProps) {
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

  const renderProcesses = () => (
    <section className="rounded-lg border border-dashed border-gray-300 bg-gray-50 px-4 py-6 text-sm text-gray-600">
      <p className="font-medium text-gray-700">Command runner on deck</p>
      <p className="mt-2 leading-relaxed">
        Soon you will be able to launch <code className="rounded bg-white px-1 py-0.5">pnpm dev</code>{' '}
        and other ad-hoc commands directly from the dashboard and stream their logs here. For now,
        continue using <code className="rounded bg-white px-1 py-0.5">agentdev worktree exec</code>{' '}
        in the terminal.
      </p>
    </section>
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
        </section>

        <div className="space-y-6 pb-12">
          {renderOverview()}
          <WorktreeSessions sessions={worktree.sessions} formatTimestamp={formatTimestamp} />

          <WorktreeGitSection
            worktreeId={worktree.id}
            status={status}
            commit={commit}
            commitsAhead={commitsAhead}
            formatTimestamp={formatTimestamp}
          />
          {renderProcesses()}
        </div>
      </div>
    </div>
  );
}
