'use client';

import { RefObject, useEffect, useMemo, useState } from 'react';
import {
  WorktreeCommitInfo,
  WorktreeCommitsAhead,
  WorktreeGitDetails,
  WorktreeGitStatus,
} from '@/types';
import { apiUrl } from '@/lib/api';
import GitDiffList, { GitDiffListEntry } from '@/components/GitDiffList';

type GitDiffSection = {
  diff: string;
  key: string;
};

function splitGitDiffByFile(diffText: string): GitDiffSection[] {
  const trimmed = diffText.trim();
  if (!trimmed) {
    return [];
  }
  const lines = trimmed.split('\n');
  const sections: GitDiffSection[] = [];
  let current: string[] = [];

  const flush = () => {
    if (!current.length) return;
    const diffChunk = current.join('\n');
    const firstLine = current[0] ?? '';
    sections.push({
      diff: diffChunk,
      key: `${firstLine}|${sections.length}`,
    });
    current = [];
  };

  for (const line of lines) {
    if (line.startsWith('diff --git ') && current.length) {
      flush();
    }
    current.push(line);
  }

  flush();

  return sections;
}

const STATUS_LABELS: Record<string, string> = {
  A: 'Added',
  M: 'Modified',
  D: 'Deleted',
  R: 'Renamed',
  C: 'Copied',
  T: 'Type change',
  U: 'Unmerged',
  '?': 'Untracked',
};

function normaliseStatusLabel(status?: string | null): string | null {
  if (!status) {
    return null;
  }
  const trimmed = status.trim();
  const code = trimmed.charAt(0);
  return STATUS_LABELS[code] ?? trimmed;
}

function computeDiffStats(diffText: string): { additions: number; deletions: number } {
  let additions = 0;
  let deletions = 0;
  const lines = diffText.split('\n');
  for (const line of lines) {
    if (!line.length) continue;
    if (line.startsWith('diff --git') || line.startsWith('index ') || line.startsWith('@@')) continue;
    if (line.startsWith('+++') || line.startsWith('---')) continue;
    if (line.startsWith('+')) additions += 1;
    else if (line.startsWith('-')) deletions += 1;
  }
  return { additions, deletions };
}

function extractDiffLabel(diffText: string, fallback: string): string {
  const headerMatch = diffText.match(/^diff --git a\/(.+?) b\/(.+)$/m);
  const stripPrefix = (value: string | null | undefined) => {
    if (!value) return null;
    if (value === '/dev/null') return null;
    return value.replace(/^[ab]\//, '');
  };

  if (!headerMatch) {
    return fallback;
  }

  const oldPath = stripPrefix(headerMatch[1]) ?? fallback;
  const newPath = stripPrefix(headerMatch[2]) ?? fallback;

  if (oldPath && newPath && oldPath !== newPath) {
    return `${oldPath} → ${newPath}`;
  }
  return newPath ?? oldPath ?? fallback;
}

interface WorktreeGitSectionProps {
  worktreeId: string | null;
  status?: WorktreeGitStatus | null;
  commit?: WorktreeCommitInfo | null;
  commitsAhead?: WorktreeCommitsAhead | null;
  formatTimestamp: (value?: string | null) => string;
  defaultExpanded?: boolean;
  scrollContainerRef?: RefObject<HTMLElement>;
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
  defaultExpanded = false,
  scrollContainerRef,
}: WorktreeGitSectionProps) {
  const [gitDetails, setGitDetails] = useState<WorktreeGitDetails | null>(null);
  const [gitError, setGitError] = useState<string | null>(null);
  const [isGitLoading, setIsGitLoading] = useState(false);
  const [isExpanded, setIsExpanded] = useState(defaultExpanded);

  useEffect(() => {
    setGitDetails(null);
    setGitError(null);
    setIsGitLoading(false);
    setIsExpanded(defaultExpanded);
  }, [worktreeId, defaultExpanded]);

  useEffect(() => {
    setIsExpanded(defaultExpanded);
  }, [defaultExpanded]);

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

  const handleToggle = () => {
    setIsExpanded((prev) => {
      const next = !prev;
      return next;
    });
  };

  const commitDiffSections = useMemo(
    () =>
      gitDetails?.commit_diff?.diff
        ? splitGitDiffByFile(gitDetails.commit_diff.diff)
        : [],
    [gitDetails?.commit_diff?.diff],
  );

  const diffEntries = useMemo<GitDiffListEntry[]>(() => {
    if (!gitDetails) return [];

    const result: GitDiffListEntry[] = [];

    const pushGroup = (
      items: WorktreeGitDetails['staged'],
      groupKey: string,
      groupLabel: string,
    ) => {
      items.forEach((entry, index) => {
        const diffText = entry.diff ?? '';
        const label = entry.display_path || entry.path || `File ${index + 1}`;
        const { additions, deletions } = computeDiffStats(diffText);
        result.push({
          key: `${groupKey}:${index}:${label}`,
          title: label,
          groupKey,
          groupLabel,
          status: entry.status,
          statusLabel: normaliseStatusLabel(entry.status),
          diffText,
          additions,
          deletions,
        });
      });
    };

    if (commitDiffSections.length) {
      const commitLabel = gitDetails.commit_diff?.reference
        ? `Divergence vs ${gitDetails.commit_diff.reference}`
        : 'Divergence vs base';
      commitDiffSections.forEach((section, index) => {
        const { additions, deletions } = computeDiffStats(section.diff);
        const title = extractDiffLabel(section.diff, `Commit diff ${index + 1}`);
        result.push({
          key: `commit:${index}:${title}`,
          title,
          groupKey: 'commit',
          groupLabel: commitLabel,
          status: 'C',
          statusLabel: 'Commit',
          diffText: section.diff,
          additions,
          deletions,
        });
      });
    }

    pushGroup(gitDetails.staged, 'staged', 'Staged');
    pushGroup(gitDetails.unstaged, 'unstaged', 'Unstaged');
    pushGroup(gitDetails.untracked, 'untracked', 'Untracked');

    return result;
  }, [gitDetails, commitDiffSections]);

  const renderGitOverview = () => {
    const commitsAheadList = commitsAhead?.commits ?? [];
    const mergeBaseLabel = commitsAhead?.merge_base
      ? formatCommitId(commitsAhead.merge_base)
      : 'unknown';
    const commitId = commit?.commit_id;
    const headShort = commitId ? formatCommitId(commitId) : null;
    const lastCommitTime = commit?.timestamp ? formatTimestamp(commit.timestamp) : null;
    const baseBranch = commitsAhead?.base_branch ?? 'default branch';

    const statusItems =
      status
        ? [
            {
              label: 'Ahead / Behind',
              value: `↑${status.ahead} / ↓${status.behind}`,
            },
            {
              label: 'Staged',
              value: status.staged.toString(),
            },
            {
              label: 'Unstaged',
              value: status.unstaged.toString(),
            },
            {
              label: 'Untracked',
              value: status.untracked.toString(),
            },
            {
              label: 'Conflicts',
              value: status.conflicts.toString(),
            },
            {
              label: 'Upstream',
              value: status.upstream ?? 'origin',
              monospace: true,
            },
          ]
        : [];

    const metaItems = [
      mergeBaseLabel
        ? {
            label: 'Merge base',
            value: mergeBaseLabel,
            title: commitsAhead?.merge_base ?? undefined,
          }
        : null,
      headShort
        ? {
            label: 'HEAD',
            value: headShort,
            title: commitId ?? undefined,
          }
        : null,
      lastCommitTime
        ? {
            label: 'Last update',
            value: lastCommitTime,
          }
        : null,
    ].filter(Boolean) as Array<{ label: string; value: string; title?: string }>;

    return (
      <section className="rounded-lg border border-gray-200 bg-white px-4 py-4 shadow-sm">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h3 className="text-sm font-semibold uppercase tracking-wide text-gray-900">
              Git Overview
            </h3>
            <p className="text-xs text-gray-500">
              Comparing to {baseBranch}
              {commitsAhead ? ` · ${commitsAheadList.length} ${commitsAheadList.length === 1 ? 'commit' : 'commits'} ahead` : ''}
            </p>
          </div>
          {status ? (
            <span
              className={`text-xs px-2 py-0.5 rounded-full ${
                status.is_clean ? 'bg-green-100 text-green-700' : 'bg-yellow-100 text-yellow-800'
              }`}
            >
              {status.is_clean ? 'Clean' : 'Changes pending'}
            </span>
          ) : (
            <span className="text-xs text-gray-400">Status unavailable</span>
          )}
        </div>

        {(statusItems.length > 0 || metaItems.length > 0) && (
          <div className="mt-3 flex flex-wrap gap-3 text-xs">
            {statusItems.map((item) => (
              <div
                key={item.label}
                className="flex items-center gap-1 rounded-md border border-gray-200 bg-gray-50 px-2 py-1 text-gray-600"
              >
                <span className="font-semibold text-gray-700">{item.label}:</span>
                <span className={item.monospace ? 'font-mono text-gray-800' : 'text-gray-700'}>
                  {item.value}
                </span>
              </div>
            ))}
            {metaItems.map((item) => (
              <div
                key={item.label}
                className="flex items-center gap-1 rounded-md border border-gray-200 bg-gray-50 px-2 py-1 text-gray-600"
              >
                <span className="font-semibold text-gray-700">{item.label}:</span>
                <span className="font-mono text-gray-800" title={item.title}>
                  {item.value}
                </span>
              </div>
            ))}
          </div>
        )}

        {!commitsAhead && (
          <p className="mt-3 text-sm text-gray-500">
            Unable to determine default branch comparison. Latest commit details shown below.
          </p>
        )}

        {commitsAhead && commitsAheadList.length === 0 && (
          <p className="mt-3 text-sm text-gray-600">
            Branch is up to date with {baseBranch}.
          </p>
        )}

        {commitsAhead && commitsAheadList.length > 0 && (
          <div className="mt-4">
            <p className="text-xs font-semibold uppercase tracking-wide text-gray-500">
              Ahead commits
            </p>
            <ol className="mt-2 divide-y divide-gray-200 overflow-hidden rounded-md border border-gray-200 bg-gray-50">
              {commitsAheadList.map((entry, index) => (
                <li key={entry.commit_id || `${entry.summary}-${index}`} className="px-3 py-2">
                  <div className="flex flex-wrap items-center justify-between gap-3 text-xs text-gray-500">
                    <span className="font-mono text-sm text-gray-800" title={entry.commit_id}>
                      {formatCommitId(entry.commit_id)}
                    </span>
                    <span>
                      {entry.timestamp ? formatTimestamp(entry.timestamp) : 'unknown'}
                    </span>
                  </div>
                  {entry.summary ? (
                    <p className="mt-1 text-sm text-gray-900">{entry.summary}</p>
                  ) : (
                    <p className="mt-1 text-sm text-gray-500">(no summary provided)</p>
                  )}
                </li>
              ))}
            </ol>
          </div>
        )}

        {commit ? (
          <div className="mt-4 rounded-md border border-dashed border-gray-200 bg-gray-50 px-3 py-3">
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

  const totalDiffCount = gitDetails ? diffEntries.length : null;

  return (
    <div className="space-y-4">
      {renderGitOverview()}
      <section className="rounded-lg border border-gray-200 bg-white px-4 py-4 shadow-sm">
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

        <div className="mt-4">
          {isExpanded ? (
            isGitLoading ? (
              <div className="flex items-center gap-2 text-sm text-gray-500">
                <div className="inline-block h-4 w-4 animate-spin rounded-full border-2 border-gray-200 border-t-blue-500" />
                <span>Loading diffs from git…</span>
              </div>
            ) : gitError ? (
              <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                Failed to load diff details: {gitError}
              </div>
            ) : gitDetails ? (
              diffEntries.length > 0 ? (
                <GitDiffList
                  entries={diffEntries}
                  emptyMessage="Working tree matches staged content."
                  scrollContainerRef={scrollContainerRef}
                />
              ) : (
                <div className="rounded-md border border-dashed border-gray-200 bg-gray-50 px-4 py-3 text-sm text-gray-500">
                  Working tree matches staged content.
                </div>
              )
            ) : (
              <p className="text-sm text-gray-500">
                Diff details will appear once git metadata loads for this worktree.
              </p>
            )
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
