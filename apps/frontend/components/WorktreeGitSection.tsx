'use client';

import { useEffect, useState } from 'react';
import {
  WorktreeCommitInfo,
  WorktreeCommitsAhead,
  WorktreeGitDetails,
  WorktreeGitStatus,
} from '@/types';
import { apiUrl } from '@/lib/api';

interface WorktreeGitSectionProps {
  worktreeId: string | null;
  status?: WorktreeGitStatus | null;
  commit?: WorktreeCommitInfo | null;
  commitsAhead?: WorktreeCommitsAhead | null;
  formatTimestamp: (value?: string | null) => string;
}

function formatCommitId(commitId?: string | null) {
  if (!commitId) {
    return '';
  }
  return commitId.length > 7 ? commitId.slice(0, 7) : commitId;
}

export default function WorktreeGitSection({
  worktreeId,
  status,
  commit,
  commitsAhead,
  formatTimestamp,
}: WorktreeGitSectionProps) {
  const [gitDetails, setGitDetails] = useState<WorktreeGitDetails | null>(null);
  const [gitError, setGitError] = useState<string | null>(null);
  const [isGitLoading, setIsGitLoading] = useState(false);
  const [expandedDiffKey, setExpandedDiffKey] = useState<string | null>(null);
  const [isExpanded, setIsExpanded] = useState(false);

  useEffect(() => {
    setGitDetails(null);
    setGitError(null);
    setIsGitLoading(false);
    setExpandedDiffKey(null);
    setIsExpanded(false);
  }, [worktreeId]);

  useEffect(() => {
    if (!isExpanded || !worktreeId) {
      return;
    }

    if (gitDetails) {
      return;
    }

    let cancelled = false;
    const controller = new AbortController();
    const currentWorktreeId = worktreeId;

    async function loadGitDetails() {
      setIsGitLoading(true);
      setGitError(null);

      try {
        const response = await fetch(
          apiUrl(`/api/worktrees/${encodeURIComponent(currentWorktreeId)}/git`),
          { signal: controller.signal },
        );
        if (!response.ok) {
          const message = await response.text();
          throw new Error(message || `Failed to load git details (${response.status})`);
        }

        const payload: WorktreeGitDetails = await response.json();
        if (!cancelled) {
          setGitDetails(payload);
        }
      } catch (err) {
        if (cancelled || (err instanceof DOMException && err.name === 'AbortError')) {
          return;
        }
        const message = err instanceof Error ? err.message : 'Unknown error';
        setGitError(message);
        setGitDetails(null);
      } finally {
        if (!cancelled) {
          setIsGitLoading(false);
        }
      }
    }

    loadGitDetails();

    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [worktreeId, isExpanded, gitDetails]);

  useEffect(() => {
    if (!gitDetails || expandedDiffKey) {
      return;
    }

    const pickFirstDiffKey = () => {
      const groups: Array<[string, WorktreeGitDetails['staged']]> = [
        ['staged', gitDetails.staged],
        ['unstaged', gitDetails.unstaged],
        ['untracked', gitDetails.untracked],
      ];
      for (const [group, entries] of groups) {
        for (let i = 0; i < entries.length; i += 1) {
          const diff = entries[i]?.diff?.trim();
          if (diff) {
            return `${group}:${i}`;
          }
        }
      }
      return null;
    };

    const candidate = pickFirstDiffKey();
    if (candidate) {
      setExpandedDiffKey(candidate);
    }
  }, [gitDetails, expandedDiffKey]);

  const handleToggle = () => {
    setIsExpanded((prev) => {
      const next = !prev;
      if (!next) {
        setExpandedDiffKey(null);
      }
      return next;
    });
  };

  const renderDiffGroup = (
    entries: WorktreeGitDetails['staged'],
    groupKey: string,
    emptyMessage: string,
  ) => {
    if (!entries.length) {
      return (
        <div className="rounded-md border border-dashed border-gray-200 bg-gray-50 px-4 py-3 text-sm text-gray-500">
          {emptyMessage}
        </div>
      );
    }

    return (
      <div className="space-y-3">
        {entries.map((entry, index) => {
          const itemKey = `${groupKey}:${index}`;
          const isOpen = expandedDiffKey === itemKey;

          return (
            <div key={itemKey} className="overflow-hidden rounded-lg border border-gray-200 bg-white">
              <button
                type="button"
                onClick={() =>
                  setExpandedDiffKey((prev) => (prev === itemKey ? null : itemKey))
                }
                className={`flex w-full items-center justify-between gap-4 px-4 py-3 text-left transition ${
                  isOpen ? 'bg-blue-50' : 'hover:bg-gray-50'
                }`}
              >
                <div className="flex flex-1 flex-col gap-1">
                  <div className="flex items-center gap-2">
                    <span className="inline-flex min-w-[1.5rem] justify-center rounded-full bg-gray-100 px-2 py-0.5 text-xs font-semibold uppercase tracking-wide text-gray-700">
                      {entry.status}
                    </span>
                    <span className="font-mono text-xs text-gray-600">{entry.display_path}</span>
                  </div>
                  {!isOpen && <p className="text-xs text-gray-400">Click to preview unified diff</p>}
                </div>
                <span className="text-xs text-gray-400">{isOpen ? 'Hide diff' : 'Show diff'}</span>
              </button>

              {isOpen && (
                <div className="border-t border-gray-200 bg-gray-950/95 px-4 py-4">
                  {entry.diff.trim() ? (
                    <pre className="max-h-96 overflow-auto text-xs leading-relaxed text-gray-100">
                      {entry.diff}
                    </pre>
                  ) : (
                    <p className="text-xs text-gray-300">No diff output available for this file.</p>
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>
    );
  };

  const renderCommitStack = () => {
    const commitsAheadList = commitsAhead?.commits ?? [];
    const mergeBaseLabel = commitsAhead?.merge_base
      ? formatCommitId(commitsAhead.merge_base)
      : 'unknown';
    const commitId = commit?.commit_id;
    const headShort = commitId ? formatCommitId(commitId) : null;
    const lastCommitTime = commit?.timestamp ? formatTimestamp(commit.timestamp) : null;

    return (
      <section className="rounded-lg border border-gray-200 bg-white px-4 py-4">
        <div className="flex items-center justify-between gap-2">
          <h3 className="text-sm font-semibold uppercase tracking-wide text-gray-900">
            Commits vs {commitsAhead?.base_branch ?? 'default branch'}
          </h3>
          {commitsAhead && (
            <span className="text-xs text-gray-400">
              {commitsAheadList.length}{' '}
              {commitsAheadList.length === 1 ? 'commit' : 'commits'}
            </span>
          )}
        </div>

        {commitsAhead ? (
          <>
            <div className="mt-3 flex flex-wrap items-center gap-3 text-xs text-gray-500">
              <span>
                Merge base:{' '}
                <span
                  className="font-mono text-gray-700"
                  title={commitsAhead.merge_base ?? undefined}
                >
                  {mergeBaseLabel}
                </span>
              </span>
              {headShort && (
                <span>
                  HEAD:{' '}
                  <span
                    className="font-mono text-gray-700"
                    title={commitId ?? undefined}
                  >
                    {headShort}
                  </span>
                </span>
              )}
              {lastCommitTime && <span>Last update {lastCommitTime}</span>}
            </div>

            {commitsAheadList.length > 0 ? (
              <ol className="mt-4 space-y-3">
                {commitsAheadList.map((entry, index) => (
                  <li
                    key={entry.commit_id || `${entry.summary}-${index}`}
                    className="rounded-lg border border-gray-200 bg-gray-50 px-3 py-3"
                  >
                    <div className="flex flex-wrap items-center justify-between gap-3">
                      <span className="font-mono text-sm text-gray-700" title={entry.commit_id}>
                        {formatCommitId(entry.commit_id)}
                      </span>
                      <span className="text-xs text-gray-400">
                        {entry.timestamp ? formatTimestamp(entry.timestamp) : 'unknown'}
                      </span>
                    </div>
                    <p className="mt-2 text-sm text-gray-900">
                      {entry.summary || '(no summary provided)'}
                    </p>
                  </li>
                ))}
              </ol>
            ) : (
              <p className="mt-4 text-sm text-gray-600">
                Branch is up to date with {commitsAhead.base_branch}.
              </p>
            )}
          </>
        ) : (
          <p className="mt-3 text-sm text-gray-500">
            Unable to determine default branch comparison. Latest commit details shown below.
          </p>
        )}

        {commit ? (
          <div className="mt-5 rounded-lg border border-dashed border-gray-200 bg-white px-3 py-3">
            <p className="text-xs font-semibold uppercase tracking-wide text-gray-500">
              Latest commit
            </p>
            <div className="mt-2 space-y-2">
              <p className="break-all font-mono text-sm text-gray-700">{commit.commit_id}</p>
              <p className="text-sm text-gray-900">{commit.summary}</p>
              <p className="text-xs text-gray-400">{lastCommitTime ?? 'Time unknown'}</p>
            </div>
          </div>
        ) : (
          <p className="mt-5 text-sm text-gray-500">No commits found yet for this worktree.</p>
        )}
      </section>
    );
  };

  const totalDiffCount = gitDetails
    ? gitDetails.staged.length + gitDetails.unstaged.length + gitDetails.untracked.length
    : null;

  return (
    <div className="space-y-4">
      {renderCommitStack()}
      <section className="rounded-lg border border-gray-200 bg-white px-4 py-4">
        <div className="flex items-center justify-between gap-2">
          <h3 className="text-sm font-semibold uppercase tracking-wide text-gray-900">
            Status Summary
          </h3>
          {status ? (
            <span
              className={`text-xs px-2 py-0.5 rounded-full ${
                status.is_clean ? 'bg-green-100 text-green-700' : 'bg-yellow-100 text-yellow-800'
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

      <section className="rounded-lg border border-gray-200 bg-white px-4 py-4">
        <header className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h3 className="text-sm font-semibold uppercase tracking-wide text-gray-900">
              Diff Breakdown
            </h3>
            <p className="text-xs text-gray-500">
              Staged, unstaged, and untracked changes pulled directly from git
            </p>
          </div>
          <div className="flex items-center gap-2">
            {totalDiffCount !== null && (
              <span className="text-xs text-gray-400">
                {totalDiffCount} {totalDiffCount === 1 ? 'entry' : 'entries'}
              </span>
            )}
            <button
              type="button"
              onClick={handleToggle}
              className="rounded-md border border-gray-200 px-2 py-1 text-xs font-medium text-gray-600 hover:bg-gray-50"
            >
              {isExpanded ? 'Hide details' : 'Show details'}
            </button>
          </div>
        </header>

        <div className="mt-4 space-y-5">
          {isExpanded ? (
            <>
              {isGitLoading ? (
                <div className="flex items-center gap-2 text-sm text-gray-500">
                  <div className="inline-block h-4 w-4 animate-spin rounded-full border-2 border-gray-200 border-t-blue-500" />
                  <span>Loading diffs from git…</span>
                </div>
              ) : gitError ? (
                <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                  Failed to load diff details: {gitError}
                </div>
              ) : gitDetails ? (
                <>
                  {gitDetails.commit_diff && gitDetails.commit_diff.diff.trim() && (
                    <div className="rounded-lg border border-gray-200 bg-gray-50 px-4 py-3">
                      <p className="text-xs font-semibold uppercase tracking-wide text-gray-700">
                        Divergence vs {gitDetails.commit_diff.reference}
                      </p>
                      <pre className="mt-3 max-h-80 overflow-auto bg-gray-950/95 px-3 py-3 text-xs leading-relaxed text-gray-100">
                        {gitDetails.commit_diff.diff}
                      </pre>
                    </div>
                  )}

                  <div className="space-y-6">
                    <div>
                      <h4 className="text-xs font-semibold uppercase tracking-wide text-gray-600">
                        Staged ({gitDetails.staged.length})
                      </h4>
                      <div className="mt-2">
                        {renderDiffGroup(gitDetails.staged, 'staged', 'No staged changes detected.')}
                      </div>
                    </div>

                    <div>
                      <h4 className="text-xs font-semibold uppercase tracking-wide text-gray-600">
                        Unstaged ({gitDetails.unstaged.length})
                      </h4>
                      <div className="mt-2">
                        {renderDiffGroup(
                          gitDetails.unstaged,
                          'unstaged',
                          'Working tree matches staged content.',
                        )}
                      </div>
                    </div>

                    <div>
                      <h4 className="text-xs font-semibold uppercase tracking-wide text-gray-600">
                        Untracked ({gitDetails.untracked.length})
                      </h4>
                      <div className="mt-2">
                        {renderDiffGroup(gitDetails.untracked, 'untracked', 'No new files detected.')}
                      </div>
                    </div>
                  </div>
                </>
              ) : (
                <p className="text-sm text-gray-500">
                  Diff details will appear once git metadata loads for this worktree.
                </p>
              )}
            </>
          ) : (
            <div className="rounded-md border border-dashed border-gray-200 bg-gray-50 px-4 py-3 text-sm text-gray-500">
              Expand to inspect staged, unstaged, and untracked diffs.
            </div>
          )}
        </div>
      </section>
    </div>
  );
}
