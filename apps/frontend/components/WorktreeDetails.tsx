'use client';

import { useEffect, useState } from 'react';
import { WorktreeGitDetails, WorktreeSummary } from '@/types';

interface WorktreeDetailsProps {
  worktree: WorktreeSummary | null;
  isLoading: boolean;
}

type SpecialMessageType = 'user_instructions' | 'environment_context' | 'user_action';

interface TagEntry {
  key: string;
  label: string;
  value: string;
}

interface SpecialMessageBase {
  type: SpecialMessageType;
  title: string;
  collapsible: boolean;
  defaultCollapsed: boolean;
  accent: 'indigo' | 'emerald' | 'blue';
}

interface UserInstructionsMessage extends SpecialMessageBase {
  type: 'user_instructions';
  body: string;
}

interface EnvironmentContextMessage extends SpecialMessageBase {
  type: 'environment_context';
  entries: TagEntry[];
}

interface UserActionMessage extends SpecialMessageBase {
  type: 'user_action';
  sections: TagEntry[];
}

type SpecialMessage = UserInstructionsMessage | EnvironmentContextMessage | UserActionMessage;

type ParsedUserMessage =
  | { kind: 'special'; message: SpecialMessage }
  | { kind: 'default'; text: string; shouldCollapse: boolean };

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

function formatCommitId(commitId?: string | null) {
  if (!commitId) {
    return '';
  }
  return commitId.length > 7 ? commitId.slice(0, 7) : commitId;
}

function shouldCollapsePlainMessage(message: string) {
  if (!message) {
    return false;
  }
  if (message.length > 320) {
    return true;
  }
  return message.split(/\r?\n/).length > 8;
}

function toStartCase(value: string) {
  return value
    .split(/[_\s-]+/)
    .filter(Boolean)
    .map((segment) => segment.charAt(0).toUpperCase() + segment.slice(1))
    .join(' ');
}

function parseTagEntries(body: string): TagEntry[] {
  const pattern = /<([a-z0-9_\-:]+)>([\s\S]*?)<\/\1>/gi;
  const entries: TagEntry[] = [];
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(body)) !== null) {
    const [, key, rawValue] = match;
    const value = rawValue.trim();
    if (value) {
      entries.push({
        key,
        label: toStartCase(key),
        value,
      });
    }
  }
  return entries;
}

function sortEntries(entries: TagEntry[], preferredOrder: string[]) {
  const orderMap = preferredOrder.reduce<Record<string, number>>((acc, key, index) => {
    acc[key] = index;
    return acc;
  }, {});
  entries.sort((a, b) => {
    const aRank = orderMap[a.key] ?? Number.MAX_SAFE_INTEGER;
    const bRank = orderMap[b.key] ?? Number.MAX_SAFE_INTEGER;
    if (aRank !== bRank) {
      return aRank - bRank;
    }
    return a.key.localeCompare(b.key);
  });
}

function parseUserMessage(message: string): ParsedUserMessage {
  const trimmed = message.trim();
  const tagMatch = trimmed.match(/^<([a-z_]+)>([\s\S]*?)<\/\1>\s*$/i);

  if (tagMatch) {
    const tag = tagMatch[1].toLowerCase() as SpecialMessageType | string;
    const body = tagMatch[2].trim();

    if (tag === 'user_instructions') {
      return {
        kind: 'special',
        message: {
          type: 'user_instructions',
          title: 'Codex AGENTS.md',
          collapsible: true,
          defaultCollapsed: true,
          accent: 'indigo',
          body,
        },
      };
    }

    if (tag === 'environment_context') {
      const entries = parseTagEntries(body);
      if (entries.length > 0) {
        sortEntries(entries, ['cwd', 'approval_policy', 'sandbox_mode', 'network_access', 'shell']);
        return {
          kind: 'special',
          message: {
            type: 'environment_context',
            title: 'Codex environment context',
            collapsible: true,
            defaultCollapsed: true,
            accent: 'emerald',
            entries,
          },
        };
      }
    }

    if (tag === 'user_action') {
      const sections = parseTagEntries(body);
      if (sections.length > 0) {
        sortEntries(sections, ['context', 'action', 'results']);
        return {
          kind: 'special',
          message: {
            type: 'user_action',
            title: 'Codex user action',
            collapsible: sections.some((entry) => shouldCollapsePlainMessage(entry.value)),
            defaultCollapsed: false,
            accent: 'blue',
            sections,
          },
        };
      }
    }
  }

  return {
    kind: 'default',
    text: message,
    shouldCollapse: shouldCollapsePlainMessage(message),
  };
}

function getSpecialMessageContainerClasses(type: SpecialMessageType) {
  switch (type) {
    case 'user_instructions':
      return 'border-indigo-200 bg-indigo-50/70';
    case 'environment_context':
      return 'border-emerald-200 bg-emerald-50/70';
    case 'user_action':
      return 'border-blue-200 bg-blue-50/70';
    default:
      return 'border-gray-200 bg-gray-50';
  }
}

export default function WorktreeDetails({
  worktree,
  isLoading,
}: WorktreeDetailsProps) {
  const [gitDetails, setGitDetails] = useState<WorktreeGitDetails | null>(null);
  const [gitError, setGitError] = useState<string | null>(null);
  const [isGitLoading, setIsGitLoading] = useState(false);
  const [expandedDiffKey, setExpandedDiffKey] = useState<string | null>(null);
  const [isGitSectionExpanded, setIsGitSectionExpanded] = useState(false);
  const [expandedSessionMessages, setExpandedSessionMessages] = useState<Record<string, boolean>>(
    {},
  );
  const worktreeId = worktree?.id ?? null;

  useEffect(() => {
    setGitDetails(null);
    setGitError(null);
    setIsGitLoading(false);
    setExpandedDiffKey(null);
    setIsGitSectionExpanded(false);
    setExpandedSessionMessages({});
  }, [worktreeId]);

  useEffect(() => {
    if (!isGitSectionExpanded || !worktreeId) {
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
          `/api/worktrees/${encodeURIComponent(currentWorktreeId)}/git`,
          { signal: controller.signal },
        );
        if (!response.ok) {
          const message = await response.text();
          throw new Error(
            message || `Failed to load git details (${response.status})`,
          );
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
  }, [worktreeId, isGitSectionExpanded, gitDetails]);

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

  const handleGitToggle = () => {
    setIsGitSectionExpanded((prev) => {
      const next = !prev;
      if (!next) {
        setExpandedDiffKey(null);
      }
      return next;
    });
  };

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
  const commitsAheadList = commitsAhead?.commits ?? [];

  const totalDiffCount = gitDetails
    ? gitDetails.staged.length + gitDetails.unstaged.length + gitDetails.untracked.length
    : null;

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
            <div
              key={itemKey}
              className="overflow-hidden rounded-lg border border-gray-200 bg-white"
            >
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
                    <span className="font-mono text-xs text-gray-600">
                      {entry.display_path}
                    </span>
                  </div>
                  {!isOpen && (
                    <p className="text-xs text-gray-400">
                      Click to preview unified diff
                    </p>
                  )}
                </div>
                <span className="text-xs text-gray-400">
                  {isOpen ? 'Hide diff' : 'Show diff'}
                </span>
              </button>

              {isOpen && (
                <div className="border-t border-gray-200 bg-gray-950/95 px-4 py-4">
                  {entry.diff.trim() ? (
                    <pre className="max-h-96 overflow-auto text-xs leading-relaxed text-gray-100">
                      {entry.diff}
                    </pre>
                  ) : (
                    <p className="text-xs text-gray-300">
                      No diff output available for this file.
                    </p>
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
    const mergeBaseLabel = commitsAhead?.merge_base
      ? formatCommitId(commitsAhead.merge_base)
      : 'unknown';
    const headShort = commit?.commit_id ? formatCommitId(commit.commit_id) : null;
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
                <span className="font-mono text-gray-700" title={commit.commit_id}>
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
                    <span
                      className="font-mono text-sm text-gray-700"
                      title={entry.commit_id}
                    >
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
            <p className="font-mono text-sm text-gray-700 break-all">{commit.commit_id}</p>
            <p className="text-sm text-gray-900">{commit.summary}</p>
            <p className="text-xs text-gray-400">
              {lastCommitTime ?? 'Time unknown'}
            </p>
          </div>
        </div>
      ) : (
        <p className="mt-5 text-sm text-gray-500">
          No commits found yet for this worktree.
        </p>
      )}
    </section>
    );
  };

  const renderGit = () => (
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
              onClick={handleGitToggle}
              className="rounded-md border border-gray-200 px-2 py-1 text-xs font-medium text-gray-600 hover:bg-gray-50"
            >
              {isGitSectionExpanded ? 'Hide details' : 'Show details'}
            </button>
          </div>
        </header>

        <div className="mt-4 space-y-5">
          {isGitSectionExpanded ? (
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
                        {renderDiffGroup(
                          gitDetails.staged,
                          'staged',
                          'No staged changes detected.',
                        )}
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
                        {renderDiffGroup(
                          gitDetails.untracked,
                          'untracked',
                          'No new files detected.',
                        )}
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

  const toggleSessionMessage = (key: string, defaultExpanded: boolean) => {
    setExpandedSessionMessages((prev) => {
      const current = prev[key];
      const isExpanded = current ?? defaultExpanded;
      return {
        ...prev,
        [key]: !isExpanded,
      };
    });
  };

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
          {worktree.sessions.map((session) => (
            <li
              key={`${session.provider}-${session.session_id}`}
              className="px-4 py-4 text-sm"
            >
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex items-center gap-2">
                  <span className="rounded-full bg-blue-50 px-2 py-0.5 text-xs font-medium uppercase tracking-wide text-blue-700">
                    {session.provider}
                  </span>
                  <span className="text-xs text-gray-500">
                    {formatTimestamp(session.last_timestamp)}
                  </span>
                </div>
                <span
                  className="rounded bg-gray-100 px-2 py-0.5 font-mono text-xs text-gray-700"
                  title={session.session_id}
                >
                  {session.session_id}
                </span>
                <button
                  type="button"
                  disabled
                  title="Resume session coming soon"
                  className="rounded-md border border-gray-200 px-2 py-1 text-xs text-gray-400"
                >
                  Resume (soon)
                </button>
              </div>
              {session.user_messages.length > 0 ? (
                <ol className="mt-3 space-y-2">
                  {session.user_messages.map((message, messageIdx) => {
                    const messageKey = `${session.provider}-${session.session_id}-${messageIdx}`;
                    const parsed = parseUserMessage(message);
                    const defaultExpanded =
                      parsed.kind === 'special'
                        ? !parsed.message.defaultCollapsed
                        : !parsed.shouldCollapse;
                    const showToggle =
                      parsed.kind === 'special'
                        ? parsed.message.collapsible
                        : parsed.shouldCollapse;
                    const storedExpansion = expandedSessionMessages[messageKey];
                    const isExpanded = showToggle
                      ? storedExpansion ?? defaultExpanded
                      : true;
                    const handleToggle = () => {
                      toggleSessionMessage(messageKey, defaultExpanded);
                    };
                    const headerContent = (
                      <div className="flex flex-wrap items-center gap-2 text-xs text-gray-500">
                        {parsed.kind === 'special' ? (
                          <span className="rounded-full bg-white/80 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-gray-700">
                            {parsed.message.title}
                          </span>
                        ) : null}
                        <span>User message #{messageIdx + 1}</span>
                      </div>
                    );
                    return (
                      <li
                        key={messageKey}
                        className={`rounded-lg border overflow-hidden ${
                          parsed.kind === 'special'
                            ? getSpecialMessageContainerClasses(parsed.message.type)
                            : 'border-gray-200 bg-gray-50'
                        }`}
                      >
                        {showToggle ? (
                          <button
                            type="button"
                            onClick={handleToggle}
                            className="flex w-full items-center justify-between gap-2 px-3 py-2 text-left cursor-pointer hover:bg-gray-900/5 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-200"
                            aria-expanded={isExpanded}
                          >
                            {headerContent}
                            <span className="text-xs font-medium text-blue-600">
                              {isExpanded ? 'Collapse' : 'Expand'}
                            </span>
                          </button>
                        ) : (
                          <div className="flex w-full items-center justify-between gap-2 px-3 py-2">
                            {headerContent}
                          </div>
                        )}
                        {(!showToggle || isExpanded) && (
                          <div className="px-3 pb-3">
                            {parsed.kind === 'special' ? (
                              <>
                                {parsed.message.type === 'user_instructions' ? (
                                  <pre className="mt-2 max-h-96 overflow-auto whitespace-pre-wrap text-sm text-gray-800">
                                    {parsed.message.body}
                                  </pre>
                                ) : null}
                                {parsed.message.type === 'environment_context' ? (
                                  <dl className="mt-3 grid grid-cols-1 gap-2 text-sm text-gray-700 sm:grid-cols-2">
                                    {parsed.message.entries.map((entry) => (
                                      <div
                                        key={entry.key}
                                        className="rounded border border-emerald-200/60 bg-white/70 px-3 py-2"
                                      >
                                        <dt className="text-xs font-semibold uppercase tracking-wide text-emerald-700">
                                          {entry.label}
                                        </dt>
                                        <dd className="mt-1 break-all font-mono text-xs text-gray-800">
                                          {entry.value}
                                        </dd>
                                      </div>
                                    ))}
                                  </dl>
                                ) : null}
                                {parsed.message.type === 'user_action' ? (
                                  <div className="mt-2 space-y-3">
                                    {parsed.message.sections.map((section) => (
                                      <div
                                        key={section.key}
                                        className="rounded border border-blue-200 bg-white/80 px-3 py-2"
                                      >
                                        <p className="text-xs font-semibold uppercase tracking-wide text-blue-700">
                                          {section.label}
                                        </p>
                                        <p className="mt-1 whitespace-pre-wrap text-sm text-gray-800">
                                          {section.value}
                                        </p>
                                      </div>
                                    ))}
                                  </div>
                                ) : null}
                              </>
                            ) : (
                              <p className="mt-2 whitespace-pre-wrap text-sm text-gray-700">
                                {message}
                              </p>
                            )}
                          </div>
                        )}
                      </li>
                    );
                  })}
                </ol>
              ) : (
                <p className="mt-3 text-sm text-gray-500">
                  No user messages recorded
                </p>
              )}
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
          {renderGit()}
          {renderSessions()}
          {renderProcesses()}
        </div>
      </div>
    </div>
  );
}
