'use client';

import { FormEvent, useEffect, useMemo, useState } from 'react';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import type { DiscoveredWorktree, WorktreeSummary } from '@/types';
import type { DiscoveryParams } from '@/hooks/useDiscoveredWorktrees';

interface WorktreeListProps {
  worktrees: WorktreeSummary[];
  isLoading: boolean;
  selectedId: string | null;
  onSelect: (id: string) => void;
  discoveredWorktrees: DiscoveredWorktree[];
  isDiscoveryLoading: boolean;
  discoveryError: string | null;
  hasDiscoveryRun: boolean;
  lastDiscoveryParams: DiscoveryParams | null;
  onRunDiscovery: (params: DiscoveryParams) => void;
  onRefreshDiscovery?: () => void;
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
  discoveredWorktrees,
  isDiscoveryLoading,
  discoveryError,
  hasDiscoveryRun,
  lastDiscoveryParams,
  onRunDiscovery,
  onRefreshDiscovery = () => {},
}: WorktreeListProps) {
  const sortedWorktrees = useMemo(() => {
    return [...worktrees].sort((a, b) =>
      b.last_activity_at.localeCompare(a.last_activity_at),
    );
  }, [worktrees]);
  const groupedWorktrees = useMemo(() => {
    const byRepo = new Map<
      string,
      { items: WorktreeSummary[]; latestActivity: number }
    >();

    for (const worktree of sortedWorktrees) {
      const activity = Date.parse(worktree.last_activity_at) || 0;
      const entry = byRepo.get(worktree.repo_name);
      if (entry) {
        entry.items.push(worktree);
        entry.latestActivity = Math.max(entry.latestActivity, activity);
      } else {
        byRepo.set(worktree.repo_name, {
          items: [worktree],
          latestActivity: activity,
        });
      }
    }

    return Array.from(byRepo.entries())
      .map(([repoName, { items, latestActivity }]) => ({
        repoName,
        items: items.sort((a, b) =>
          b.last_activity_at.localeCompare(a.last_activity_at),
        ),
        latestActivity,
      }))
      .sort((a, b) => {
        if (a.latestActivity !== b.latestActivity) {
          return b.latestActivity - a.latestActivity;
        }
        return a.repoName.localeCompare(b.repoName);
      });
  }, [sortedWorktrees]);

  const [isDiscoveryFormOpen, setDiscoveryFormOpen] = useState(false);
  const [rootInput, setRootInput] = useState('');
  const [isRecursive, setIsRecursive] = useState(true);

  useEffect(() => {
    if (isDiscoveryFormOpen) {
      setRootInput(lastDiscoveryParams?.root ?? '');
      setIsRecursive(lastDiscoveryParams?.recursive ?? true);
    }
  }, [isDiscoveryFormOpen, lastDiscoveryParams]);

  const handleSubmitDiscovery = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    onRunDiscovery({
      recursive: isRecursive,
      root: rootInput.trim() === '' ? null : rootInput.trim(),
    });
    setDiscoveryFormOpen(false);
  };

  return (
    <div className="flex flex-col gap-6 py-2">
      <section>
        <div className="border-b border-border px-3 pb-2">
          <h2 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
            Worktrees
          </h2>
          <p className="mt-1 text-[0.7rem] text-muted-foreground/80">
            Sorted by recent activity
          </p>
        </div>
        {isLoading && sortedWorktrees.length === 0 ? (
          <div className="px-3 py-4 text-sm text-muted-foreground">
            <div className="flex items-center space-x-2">
              <div className="inline-block h-4 w-4 animate-spin rounded-full border-2 border-border border-t-primary" />
              <span>Loading worktrees...</span>
            </div>
          </div>
        ) : sortedWorktrees.length === 0 ? (
          <div className="px-3 py-4 text-xs text-muted-foreground">
            <p className="font-medium">No managed worktrees yet.</p>
            <p className="mt-2">
              Use <code className="rounded bg-muted px-1 py-0.5">agentdev worktree create</code> or{' '}
              <code className="rounded bg-muted px-1 py-0.5">agentdev worktree add</code> to get started.
            </p>
          </div>
        ) : (
          <div className="mt-2 space-y-4">
            {groupedWorktrees.map(({ repoName, items }) => (
              <div key={repoName}>
                <div className="px-3 text-[0.65rem] font-semibold uppercase tracking-wide text-muted-foreground/70">
                  {repoName}
                </div>
                <div className="mt-1.5 space-y-1">
                  {items.map((worktree) => {
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
                          'w-full border-l-2 border-transparent px-3 py-2 text-left transition-colors',
                          isSelected
                            ? 'border-primary/70 bg-primary/10 text-foreground'
                            : 'hover:bg-muted',
                        )}
                      >
                        <div className="flex items-center justify-between">
                          <div className="flex items-center space-x-1.5">
                            <span className="truncate text-sm font-medium text-foreground">
                              {worktree.name}
                            </span>
                            {status && (
                              <span
                                className={`text-[0.65rem] px-1.5 py-0.5 rounded-full ${
                                  dirty
                                    ? 'bg-yellow-100 text-yellow-800'
                                    : 'bg-green-100 text-green-700'
                                }`}
                              >
                                {dirty ? 'Dirty' : 'Clean'}
                              </span>
                            )}
                          </div>
                          <span className="text-[0.7rem] text-muted-foreground">
                            {formatRelativeTime(worktree.last_activity_at)}
                          </span>
                        </div>
                        <div className="mt-1 flex flex-wrap items-center gap-1 text-[0.7rem] text-muted-foreground">
                          <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-[0.7rem]">
                            {worktree.repo_name}/{worktree.branch}
                          </span>
                          {status && (status.ahead > 0 || status.behind > 0) && (
                            <span className="rounded bg-primary/10 px-1.5 py-0.5 text-primary">
                              {status.ahead > 0 && <span>↑{status.ahead}</span>}
                              {status.ahead > 0 && status.behind > 0 && (
                                <span className="mx-1">·</span>
                              )}
                              {status.behind > 0 && <span>↓{status.behind}</span>}
                            </span>
                          )}
                        </div>
                      </button>
                    );
                  })}
                </div>
              </div>
            ))}
          </div>
        )}
      </section>

      <section className="border-t border-border pt-2">
        <div className="flex items-center justify-between px-3 pb-2">
          <div>
            <h2 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
              Unmanaged Worktrees
            </h2>
            <p className="mt-1 text-[0.7rem] text-muted-foreground/80">
              Trigger on-demand discovery across git repositories
            </p>
          </div>
          <div className="flex items-center gap-2">
            {hasDiscoveryRun && (
              <Button
                size="sm"
                variant="ghost"
                onClick={onRefreshDiscovery}
                disabled={isDiscoveryLoading}
              >
                Refresh
              </Button>
            )}
            <Button
              size="sm"
              variant={isDiscoveryFormOpen ? 'default' : 'outline'}
              onClick={() => setDiscoveryFormOpen((prev) => !prev)}
            >
              {isDiscoveryFormOpen ? 'Close' : 'Discover'}
            </Button>
          </div>
        </div>
        {isDiscoveryFormOpen && (
          <form
            onSubmit={handleSubmitDiscovery}
            className="space-y-3 px-3 pb-3 text-xs text-muted-foreground"
          >
            <div className="space-y-1">
              <label className="block font-medium text-muted-foreground">
                Scan root directory
              </label>
              <input
                type="text"
                value={rootInput}
                onChange={(event) => setRootInput(event.target.value)}
                placeholder="Leave blank to scan current repository"
                className="w-full rounded border border-border bg-background px-2 py-1 text-[0.75rem] text-foreground focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary"
              />
              <p className="text-[0.65rem] text-muted-foreground">
                Provide an absolute path on the AgentDev host. The scan respects git worktree limits and recursion depth.
              </p>
            </div>
            <label className="flex items-center space-x-2">
              <input
                type="checkbox"
                checked={isRecursive}
                onChange={(event) => setIsRecursive(event.target.checked)}
                className="h-3.5 w-3.5 rounded border border-border"
              />
              <span>Search subdirectories recursively</span>
            </label>
            <div className="flex items-center gap-2">
              <Button type="submit" size="sm" disabled={isDiscoveryLoading}>
                Run discovery
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={() => setDiscoveryFormOpen(false)}
              >
                Cancel
              </Button>
            </div>
          </form>
        )}
        {discoveryError ? (
          <div className="px-3 text-xs text-destructive">
            Failed to discover worktrees: {discoveryError}
          </div>
        ) : isDiscoveryLoading && !hasDiscoveryRun ? (
          <div className="px-3 py-2 text-sm text-muted-foreground">
            <div className="flex items-center space-x-2">
              <div className="inline-block h-3.5 w-3.5 animate-spin rounded-full border-2 border-border border-t-primary" />
              <span>Scanning for unmanaged worktrees…</span>
            </div>
          </div>
        ) : !hasDiscoveryRun ? (
          <div className="px-3 py-2 text-xs text-muted-foreground">
            No scan has been run yet. Choose a directory and start discovery to surface unmanaged worktrees.
          </div>
        ) : discoveredWorktrees.length === 0 ? (
          <div className="px-3 py-2 text-xs text-muted-foreground">
            All detected worktrees are already managed.
          </div>
        ) : (
          <div className="space-y-2 px-3">
            {lastDiscoveryParams?.root && (
              <div className="text-[0.65rem] text-muted-foreground">
                Last scan root: {lastDiscoveryParams.root}
              </div>
            )}
            {discoveredWorktrees.map((entry) => (
              <div
                key={`${entry.repo}:${entry.path}`}
                className="rounded border border-border bg-muted/40 p-2 text-[0.75rem]"
              >
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span className="font-medium text-foreground">{entry.path}</span>
                  {entry.branch && (
                    <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-[0.7rem]">
                      {entry.branch}
                    </span>
                  )}
                </div>
                <div className="mt-1 flex flex-wrap items-center gap-1 text-[0.65rem] text-muted-foreground">
                  <span className="rounded bg-background px-1.5 py-0.5 font-mono text-[0.65rem]">
                    {entry.repo}
                  </span>
                  {entry.head && (
                    <span className="rounded bg-background px-1.5 py-0.5 font-mono text-[0.65rem]">
                      HEAD {entry.head.slice(0, 7)}
                    </span>
                  )}
                  {entry.bare && (
                    <span className="rounded bg-stone-200 px-1.5 py-0.5 text-stone-700">
                      bare
                    </span>
                  )}
                  {entry.locked && (
                    <span className="rounded bg-amber-100 px-1.5 py-0.5 text-amber-800">
                      locked
                    </span>
                  )}
                  {entry.prunable && (
                    <span className="rounded bg-amber-100 px-1.5 py-0.5 text-amber-800">
                      prunable
                    </span>
                  )}
                </div>
                {entry.locked && (
                  <div className="mt-1 text-[0.65rem] text-muted-foreground">
                    {entry.locked}
                  </div>
                )}
                {entry.prunable && entry.prunable !== entry.locked && (
                  <div className="mt-1 text-[0.65rem] text-muted-foreground">
                    {entry.prunable}
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}
