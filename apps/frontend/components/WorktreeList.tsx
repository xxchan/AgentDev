'use client';

import { useMemo } from 'react';
import { WorktreeSummary } from '@/types';

interface WorktreeListProps {
  worktrees: WorktreeSummary[];
  isLoading: boolean;
  selectedId: string | null;
  onSelect: (id: string) => void;
}

function formatRelativeTime(isoString: string) {
  const timestamp = new Date(isoString);
  if (Number.isNaN(timestamp.getTime())) {
    return 'unknown';
  }

  const diffMs = Date.now() - timestamp.getTime();
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

  return timestamp.toLocaleDateString();
}

export default function WorktreeList({
  worktrees,
  isLoading,
  selectedId,
  onSelect,
}: WorktreeListProps) {
  const sortedWorktrees = useMemo(() => {
    return [...worktrees].sort((a, b) =>
      b.last_activity_at.localeCompare(a.last_activity_at),
    );
  }, [worktrees]);

  if (isLoading && sortedWorktrees.length === 0) {
    return (
      <div className="p-4 text-sm text-gray-500">
        <div className="flex items-center space-x-2">
          <div className="inline-block w-4 h-4 border-2 border-gray-300 border-t-blue-500 rounded-full animate-spin" />
          <span>Loading worktrees...</span>
        </div>
      </div>
    );
  }

  if (!isLoading && sortedWorktrees.length === 0) {
    return (
      <div className="p-6 text-center text-gray-500">
        <p className="text-sm font-medium">No managed worktrees yet</p>
        <p className="text-xs mt-2">
          Use <code className="px-1 py-0.5 bg-gray-100 rounded">agentdev worktree create</code>{' '}
          or <code className="px-1 py-0.5 bg-gray-100 rounded">agentdev worktree add</code> to get started.
        </p>
      </div>
    );
  }

  return (
    <div className="py-3">
      <div className="px-4 pb-3 border-b border-gray-200">
        <h2 className="text-sm font-semibold text-gray-700 uppercase tracking-wide">
          Worktrees
        </h2>
        <p className="text-xs text-gray-500 mt-1">Sorted by recent activity</p>
      </div>
      <div className="mt-2">
        {sortedWorktrees.map((worktree) => {
          const isSelected = worktree.id === selectedId;
          const status = worktree.git_status;
          const dirty =
            status &&
            (!status.is_clean ||
              status.ahead > 0 ||
              status.behind > 0 ||
              status.conflicts > 0);

          return (
            <button
              key={worktree.id}
              type="button"
              onClick={() => onSelect(worktree.id)}
              className={`w-full text-left px-4 py-3 transition-colors ${
                isSelected ? 'bg-blue-50 border-r-2 border-blue-500' : 'hover:bg-gray-50'
              }`}
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-2">
                  <span className="font-medium text-gray-900 truncate">{worktree.name}</span>
                  {status && (
                    <span
                      className={`text-xs px-2 py-0.5 rounded-full ${
                        dirty
                          ? 'bg-yellow-100 text-yellow-800'
                          : 'bg-green-100 text-green-700'
                      }`}
                    >
                      {dirty ? 'Dirty' : 'Clean'}
                    </span>
                  )}
                </div>
                <span className="text-xs text-gray-500">
                  {formatRelativeTime(worktree.last_activity_at)}
                </span>
              </div>
              <div className="mt-1 text-xs text-gray-600 flex flex-wrap items-center gap-2">
                <span className="font-mono bg-gray-100 px-1.5 py-0.5 rounded">
                  {worktree.repo_name}/{worktree.branch}
                </span>
                {status && (status.ahead > 0 || status.behind > 0) && (
                  <span className="bg-blue-50 text-blue-700 px-1.5 py-0.5 rounded">
                    {status.ahead > 0 && <span>↑{status.ahead}</span>}
                    {status.ahead > 0 && status.behind > 0 && <span className="mx-1">·</span>}
                    {status.behind > 0 && <span>↓{status.behind}</span>}
                  </span>
                )}
              </div>
            </button>
          );
        })}
      </div>
    </div>
  );
}
