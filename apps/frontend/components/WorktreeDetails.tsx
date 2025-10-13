'use client';

import { WorktreeSummary } from '@/types';

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
  return `${date.toLocaleString()}`;
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

  return (
    <div className="h-full overflow-y-auto px-6 py-6">
      <div className="max-w-4xl space-y-6">
        <section className="bg-white border border-gray-200 rounded-lg shadow-sm">
          <div className="px-6 py-4 border-b border-gray-200">
            <h2 className="text-lg font-semibold text-gray-900">{worktree.name}</h2>
            <p className="text-sm text-gray-500 mt-1">
              {worktree.repo_name}/{worktree.branch}
            </p>
          </div>
          <div className="px-6 py-4 grid grid-cols-1 md:grid-cols-2 gap-4 text-sm">
            <div>
              <p className="text-gray-500">Path</p>
              <p className="font-mono text-gray-900 break-all">{worktree.path}</p>
            </div>
            <div>
              <p className="text-gray-500">Last Activity</p>
              <p className="text-gray-900">{formatTimestamp(worktree.last_activity_at)}</p>
            </div>
            {worktree.initial_prompt && (
              <div className="md:col-span-2">
                <p className="text-gray-500">Initial Prompt</p>
                <p className="text-gray-900 whitespace-pre-wrap">{worktree.initial_prompt}</p>
              </div>
            )}
            {worktree.agent_alias && (
              <div>
                <p className="text-gray-500">Default Agent</p>
                <p className="text-gray-900">{worktree.agent_alias}</p>
              </div>
            )}
          </div>
        </section>

        <section className="bg-white border border-gray-200 rounded-lg shadow-sm">
          <div className="px-6 py-4 border-b border-gray-200 flex items-center justify-between">
            <h3 className="text-sm font-semibold text-gray-900 uppercase tracking-wide">
              Git Status
            </h3>
            {status ? (
              <span
                className={`text-xs px-2 py-0.5 rounded-full ${
                  status.is_clean ? 'bg-green-100 text-green-700' : 'bg-yellow-100 text-yellow-800'
                }`}
              >
                {status.is_clean ? 'Clean' : 'Dirty'}
              </span>
            ) : (
              <span className="text-xs text-gray-400">Unavailable</span>
            )}
          </div>
          {status ? (
            <div className="px-6 py-4 grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
              <div>
                <p className="text-gray-500">Ahead / Behind</p>
                <p className="text-gray-900">
                  ↑{status.ahead} / ↓{status.behind}
                </p>
              </div>
              <div>
                <p className="text-gray-500">Staged</p>
                <p className="text-gray-900">{status.staged}</p>
              </div>
              <div>
                <p className="text-gray-500">Unstaged</p>
                <p className="text-gray-900">{status.unstaged}</p>
              </div>
              <div>
                <p className="text-gray-500">Untracked</p>
                <p className="text-gray-900">{status.untracked}</p>
              </div>
              <div>
                <p className="text-gray-500">Conflicts</p>
                <p className="text-gray-900">{status.conflicts}</p>
              </div>
              <div className="md:col-span-3">
                <p className="text-gray-500">Upstream</p>
                <p className="text-gray-900">{status.upstream ?? 'origin'}</p>
              </div>
            </div>
          ) : (
            <div className="px-6 py-6 text-sm text-gray-500">
              Unable to fetch git status for this worktree. Check server logs for details.
            </div>
          )}
        </section>

        <section className="bg-white border border-gray-200 rounded-lg shadow-sm">
          <div className="px-6 py-4 border-b border-gray-200">
            <h3 className="text-sm font-semibold text-gray-900 uppercase tracking-wide">
              Latest Commit
            </h3>
          </div>
          {commit ? (
            <div className="px-6 py-4 space-y-2 text-sm">
              <p className="font-mono text-gray-700">{commit.commit_id}</p>
              <p className="text-gray-900">{commit.summary}</p>
              <p className="text-gray-500 text-xs">
                {commit.timestamp ? formatTimestamp(commit.timestamp) : 'No timestamp'}
              </p>
            </div>
          ) : (
            <div className="px-6 py-6 text-sm text-gray-500">
              No commits found yet for this worktree.
            </div>
          )}
        </section>

        <section className="bg-white border border-gray-200 rounded-lg shadow-sm">
          <div className="px-6 py-4 border-b border-gray-200 flex items-center justify-between">
            <h3 className="text-sm font-semibold text-gray-900 uppercase tracking-wide">
              Sessions
            </h3>
            <span className="text-xs text-gray-400">
              {worktree.sessions.length} total
            </span>
          </div>
          {worktree.sessions.length > 0 ? (
            <ul className="divide-y divide-gray-100">
              {worktree.sessions.slice(0, 5).map((session, idx) => (
                <li key={`${session.provider}-${idx}`} className="px-6 py-4 text-sm">
                  <div className="flex items-start justify-between">
                    <span className="font-medium text-gray-900 capitalize">
                      {session.provider}
                    </span>
                    <span className="text-xs text-gray-500">
                      {formatTimestamp(session.last_timestamp)}
                    </span>
                  </div>
                  <p className="text-gray-600 mt-1">
                    {session.last_user_message || 'No user messages recorded'}
                  </p>
                </li>
              ))}
              {worktree.sessions.length > 5 && (
                <li className="px-6 py-3 text-xs text-gray-400">
                  +{worktree.sessions.length - 5} more session(s)
                </li>
              )}
            </ul>
          ) : (
            <div className="px-6 py-6 text-sm text-gray-500">
              No captured sessions yet for this worktree.
            </div>
          )}
        </section>
      </div>
    </div>
  );
}
