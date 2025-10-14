'use client';

import { useMemo } from 'react';
import { cn } from '@/lib/utils';
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
      <div className="p-4 text-sm text-muted-foreground">
        <div className="flex items-center space-x-2">
          <div className="inline-block h-4 w-4 animate-spin rounded-full border-2 border-border border-t-primary" />
          <span>Loading worktrees...</span>
        </div>
      </div>
    );
  }

  if (!isLoading && sortedWorktrees.length === 0) {
    return (
      <div className="p-6 text-center text-muted-foreground">
        <p className="text-sm font-medium">No managed worktrees yet</p>
        <p className="mt-2 text-xs">
          Use <code className="rounded bg-muted px-1 py-0.5">agentdev worktree create</code>{' '}
          or <code className="rounded bg-muted px-1 py-0.5">agentdev worktree add</code> to get started.
        </p>
      </div>
    );
  }

  return (
    <div className="py-3">
      <div className="border-b border-border px-4 pb-3">
        <h2 className="text-sm font-semibold uppercase tracking-wide text-muted-foreground">
          Worktrees
        </h2>
        <p className="mt-1 text-xs text-muted-foreground/80">
          Sorted by recent activity
        </p>
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
              className={cn(
                'w-full border-l-2 border-transparent px-4 py-3 text-left transition-colors',
                isSelected
                  ? 'border-primary/70 bg-primary/10 text-foreground'
                  : 'hover:bg-muted',
              )}
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-2">
                  <span className="truncate font-medium text-foreground">
                    {worktree.name}
                  </span>
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
                <span className="text-xs text-muted-foreground">
                  {formatRelativeTime(worktree.last_activity_at)}
                </span>
              </div>
              <div className="mt-1 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                <span className="rounded bg-muted px-1.5 py-0.5 font-mono">
                  {worktree.repo_name}/{worktree.branch}
                </span>
                {status && (status.ahead > 0 || status.behind > 0) && (
                  <span className="rounded bg-primary/10 px-1.5 py-0.5 text-primary">
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
