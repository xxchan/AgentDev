'use client';

import { useEffect, useState } from 'react';
import { WorktreeSummary } from '@/types';

interface WorktreeDetailsProps {
  worktree: WorktreeSummary | null;
  isLoading: boolean;
}

type TabKey = 'overview' | 'git' | 'sessions' | 'processes';

const TABS: Array<{ key: TabKey; label: string }> = [
  { key: 'overview', label: 'Overview' },
  { key: 'git', label: 'Git' },
  { key: 'sessions', label: 'Sessions' },
  { key: 'processes', label: 'Processes' },
];

function formatTimestamp(value?: string | null) {
  if (!value) {
    return 'unknown';
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return 'unknown';
  }
  return `${date.toLocaleString()}`;
}

export default function WorktreeDetails({
  worktree,
  isLoading,
}: WorktreeDetailsProps) {
  const [activeTab, setActiveTab] = useState<TabKey>('overview');

  useEffect(() => {
    setActiveTab('overview');
  }, [worktree?.id]);

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
            className="rounded-lg border border-gray-200 bg-white px-4 py-3"
          >
            <p className="text-xs font-medium uppercase tracking-wide text-gray-500">
              {card.label}
            </p>
            <p
              className={`mt-1 text-sm text-gray-900 ${
                card.monospace ? 'font-mono break-all' : ''
              }`}
            >
              {card.value}
            </p>
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

      <section className="rounded-lg border border-gray-200 bg-white px-4 py-4">
        <div className="flex items-center justify-between gap-2">
          <h3 className="text-sm font-semibold uppercase tracking-wide text-gray-900">
            Latest Commit
          </h3>
          {commit ? (
            <span className="text-xs text-gray-500">
              {commit.timestamp ? formatTimestamp(commit.timestamp) : 'Time unknown'}
            </span>
          ) : null}
        </div>
        {commit ? (
          <div className="mt-3 space-y-2">
            <p className="font-mono text-sm text-gray-700">{commit.commit_id}</p>
            <p className="text-sm text-gray-900">{commit.summary}</p>
          </div>
        ) : (
          <p className="mt-3 text-sm text-gray-500">
            No commits found yet for this worktree.
          </p>
        )}
      </section>
    </div>
  );

  const renderGit = () => (
    <div className="space-y-4">
      <section className="rounded-lg border border-gray-200 bg-white px-4 py-4">
        <div className="flex items-center justify-between gap-2">
          <h3 className="text-sm font-semibold uppercase tracking-wide text-gray-900">
            Status Summary
          </h3>
          {status ? (
            <span
              className={`text-xs px-2 py-0.5 rounded-full ${
                status.is_clean
                  ? 'bg-green-100 text-green-700'
                  : 'bg-yellow-100 text-yellow-800'
              }`}
            >
              {status.is_clean ? 'Clean' : 'Changes pending'}
            </span>
          ) : (
            <span className="text-xs text-gray-400">Unavailable</span>
          )}
        </div>
        {status ? (
          <dl className="mt-4 grid grid-cols-2 gap-4 text-sm md:grid-cols-3">
            <div>
              <dt className="text-gray-500">Ahead / Behind</dt>
              <dd className="text-gray-900">
                ↑{status.ahead} / ↓{status.behind}
              </dd>
            </div>
            <div>
              <dt className="text-gray-500">Staged</dt>
              <dd className="text-gray-900">{status.staged}</dd>
            </div>
            <div>
              <dt className="text-gray-500">Unstaged</dt>
              <dd className="text-gray-900">{status.unstaged}</dd>
            </div>
            <div>
              <dt className="text-gray-500">Untracked</dt>
              <dd className="text-gray-900">{status.untracked}</dd>
            </div>
            <div>
              <dt className="text-gray-500">Conflicts</dt>
              <dd className="text-gray-900">{status.conflicts}</dd>
            </div>
            <div>
              <dt className="text-gray-500">Upstream</dt>
              <dd className="text-gray-900">{status.upstream ?? 'origin'}</dd>
            </div>
          </dl>
        ) : (
          <p className="mt-4 text-sm text-gray-500">
            Unable to fetch git status for this worktree. Check server logs for details.
          </p>
        )}
      </section>

      <section className="rounded-lg border border-dashed border-gray-300 bg-gray-50 px-4 py-6 text-sm text-gray-600">
        <p className="font-medium text-gray-700">Diff preview coming soon</p>
        <p className="mt-2 leading-relaxed">
          The dashboard will surface staged versus unstaged files and let you preview diffs
          inline. Today you can continue to run{' '}
          <code className="rounded bg-white px-1 py-0.5">git status</code> or{' '}
          <code className="rounded bg-white px-1 py-0.5">git diff</code> inside the worktree
          directory while we land the richer UI.
        </p>
      </section>
    </div>
  );

  const renderSessions = () => (
    <section className="rounded-lg border border-gray-200 bg-white">
      <header className="flex items-center justify-between border-b border-gray-200 px-4 py-3">
        <div>
          <h3 className="text-sm font-semibold uppercase tracking-wide text-gray-900">
            Sessions
          </h3>
          <p className="text-xs text-gray-500">
            Captured conversations scoped to this worktree
          </p>
        </div>
        <span className="text-xs text-gray-400">
          {worktree.sessions.length} total
        </span>
      </header>
      {worktree.sessions.length > 0 ? (
        <ul className="divide-y divide-gray-100">
          {worktree.sessions.map((session, idx) => (
            <li key={`${session.provider}-${idx}`} className="px-4 py-4 text-sm">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex items-center gap-2">
                  <span className="rounded-full bg-blue-50 px-2 py-0.5 text-xs font-medium uppercase tracking-wide text-blue-700">
                    {session.provider}
                  </span>
                  <span className="text-xs text-gray-500">
                    {formatTimestamp(session.last_timestamp)}
                  </span>
                </div>
                <button
                  type="button"
                  disabled
                  title="Resume session coming soon"
                  className="rounded-md border border-gray-200 px-2 py-1 text-xs text-gray-400"
                >
                  Resume (soon)
                </button>
              </div>
              <p className="mt-2 text-gray-700">
                {session.last_user_message || 'No user messages recorded'}
              </p>
            </li>
          ))}
        </ul>
      ) : (
        <div className="px-4 py-6 text-sm text-gray-500">
          No captured sessions yet for this worktree. Conversations launched via Codex or Claude
          will appear here automatically.
        </div>
      )}
    </section>
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

  const renderActiveTab = () => {
    switch (activeTab) {
      case 'overview':
        return renderOverview();
      case 'git':
        return renderGit();
      case 'sessions':
        return renderSessions();
      case 'processes':
        return renderProcesses();
      default:
        return null;
    }
  };

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

        <nav className="flex gap-2 border-b border-gray-200 px-2">
          {TABS.map((tab) => {
            const isActive = tab.key === activeTab;
            return (
              <button
                key={tab.key}
                type="button"
                onClick={() => setActiveTab(tab.key)}
                className={`rounded-t-md px-4 py-2 text-sm font-medium ${
                  isActive
                    ? 'bg-white text-blue-600 shadow-sm'
                    : 'text-gray-500 hover:text-gray-700'
                }`}
              >
                {tab.label}
              </button>
            );
          })}
        </nav>

        <div className="pb-12">{renderActiveTab()}</div>
      </div>
    </div>
  );
}
