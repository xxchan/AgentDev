'use client';

import { useEffect, useMemo, useState, type ChangeEvent } from 'react';
import MainLayout from '@/components/layout/MainLayout';
import SessionListView, { SessionListItem } from '@/components/SessionListView';
import { useSessions } from '@/hooks/useSessions';
import { SessionSummary } from '@/types';
import { cn } from '@/lib/utils';

type SessionGroupKind = 'all' | 'worktree' | 'directory' | 'unassigned';

interface SessionGroup {
  id: string;
  label: string;
  description?: string;
  kind: SessionGroupKind;
  count: number;
  latestActivity: number;
  worktreeId?: string;
  workingDir?: string;
  workingDirKey?: string;
}

interface SessionGroupSection {
  title: string;
  groups: SessionGroup[];
}

interface SessionIndex {
  groups: SessionGroup[];
  groupsById: Map<string, SessionGroup>;
  sessionsByGroup: Map<string, SessionSummary[]>;
  sessionByKey: Map<string, SessionSummary>;
  defaultGroupId: string;
}

interface InternalSessionGroup extends SessionGroup {
  sessions: SessionSummary[];
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

function describeRepo(session: SessionSummary) {
  if (session.repo_name && session.branch) {
    return `${session.repo_name}/${session.branch}`;
  }
  if (session.repo_name) {
    return session.repo_name;
  }
  return undefined;
}

function normalizeWorkingDirKey(path: string): string {
  const trimmed = path.trim();
  if (!trimmed) {
    return '__root__';
  }
  const normalized = trimmed.replace(/\\/g, '/');
  const withoutTrailing = normalized.replace(/\/+$/, '');
  return withoutTrailing || '__root__';
}

function getSessionKey(session: SessionSummary) {
  return `${session.provider}-${session.session_id}`;
}

function buildSessionIndex(sessions: SessionSummary[]): SessionIndex {
  const groups = new Map<string, InternalSessionGroup>();
  const sessionsByGroup = new Map<string, SessionSummary[]>();
  const sessionByKey = new Map<string, SessionSummary>();

  const dedupedSessions: SessionSummary[] = [];
  const seenKeys = new Set<string>();
  sessions.forEach((session) => {
    const key = getSessionKey(session);
    if (seenKeys.has(key)) {
      return;
    }
    seenKeys.add(key);
    dedupedSessions.push(session);
  });

  const ensureGroup = (id: string, create: () => SessionGroup): InternalSessionGroup => {
    const existing = groups.get(id);
    if (existing) {
      return existing;
    }
    const base = create();
    const internal: InternalSessionGroup = {
      ...base,
      count: base.count ?? 0,
      latestActivity: base.latestActivity ?? 0,
      sessions: [],
    };
    groups.set(id, internal);
    sessionsByGroup.set(id, internal.sessions);
    return internal;
  };

  const updateGroup = (group: InternalSessionGroup, session: SessionSummary, timestamp: number) => {
    group.count += 1;
    if (timestamp > group.latestActivity) {
      group.latestActivity = timestamp;
    }
    group.sessions.push(session);
  };

  const allGroup = ensureGroup('all', () => ({
    id: 'all',
    label: 'All Sessions',
    description: 'Every captured conversation',
    kind: 'all',
    count: 0,
    latestActivity: 0,
  }));

  dedupedSessions.forEach((session) => {
    const timestamp = session.last_timestamp ? Date.parse(session.last_timestamp) : 0;
    sessionByKey.set(getSessionKey(session), session);
    updateGroup(allGroup, session, timestamp);

    if (session.worktree_id) {
      const groupId = `worktree:${session.worktree_id}`;
      const group = ensureGroup(groupId, () => ({
        id: groupId,
        label: session.worktree_name ?? session.worktree_id ?? 'Unknown worktree',
        description: session.working_dir ?? describeRepo(session),
        kind: 'worktree',
        count: 0,
        latestActivity: 0,
        worktreeId: session.worktree_id ?? undefined,
        workingDir: session.working_dir ?? undefined,
      }));

      if (!group.description) {
        group.description = describeRepo(session);
      }

      updateGroup(group, session, timestamp);
      return;
    }

    if (session.working_dir) {
      const workingDirKey = normalizeWorkingDirKey(session.working_dir);
      const groupId = `directory:${workingDirKey}`;
      const group = ensureGroup(groupId, () => ({
        id: groupId,
        label: session.working_dir ?? 'Unknown directory',
        description: undefined,
        kind: 'directory',
        count: 0,
        latestActivity: 0,
        workingDir: session.working_dir ?? undefined,
        workingDirKey,
      }));
      updateGroup(group, session, timestamp);
      return;
    }

    const unassigned = ensureGroup('unassigned', () => ({
      id: 'unassigned',
      label: 'Unassigned',
      description: 'Sessions without a working directory',
      kind: 'unassigned',
      count: 0,
      latestActivity: 0,
    }));
    updateGroup(unassigned, session, timestamp);
  });
  const groupsArray: SessionGroup[] = Array.from(groups.values()).map((group) => ({
    id: group.id,
    label: group.label,
    description: group.description,
    kind: group.kind,
    count: group.count,
    latestActivity: group.latestActivity,
    worktreeId: group.worktreeId,
    workingDir: group.workingDir,
    workingDirKey: group.workingDirKey,
  }));

  groupsArray.sort((a, b) => {
    const kindOrder: Record<SessionGroupKind, number> = {
      all: 0,
      worktree: 1,
      directory: 2,
      unassigned: 3,
    };
    if (kindOrder[a.kind] !== kindOrder[b.kind]) {
      return kindOrder[a.kind] - kindOrder[b.kind];
    }
    if (a.latestActivity !== b.latestActivity) {
      return b.latestActivity - a.latestActivity;
    }
    return a.label.localeCompare(b.label);
  });

  sessionsByGroup.forEach((bucket) => {
    bucket.sort((first, second) => {
      const firstTime = first.last_timestamp ? Date.parse(first.last_timestamp) : 0;
      const secondTime = second.last_timestamp ? Date.parse(second.last_timestamp) : 0;
      return secondTime - firstTime;
    });
  });

  const groupsById = new Map<string, SessionGroup>();
  groupsArray.forEach((group) => groupsById.set(group.id, group));

  const defaultGroupId = groupsArray[0]?.id ?? 'all';

  return {
    groups: groupsArray,
    groupsById,
    sessionsByGroup,
    sessionByKey,
    defaultGroupId,
  };
}

function groupSessionGroups(groups: SessionGroup[]): SessionGroupSection[] {
  const bucket: Record<SessionGroupKind, SessionGroup[]> = {
    all: [],
    worktree: [],
    directory: [],
    unassigned: [],
  };

  groups.forEach((group) => {
    bucket[group.kind].push(group);
  });

  const sections: SessionGroupSection[] = [];
  if (bucket.all.length > 0) {
    sections.push({ title: 'Overview', groups: bucket.all });
  }
  if (bucket.worktree.length > 0) {
    sections.push({ title: 'Worktrees', groups: bucket.worktree });
  }
  if (bucket.directory.length > 0) {
    sections.push({ title: 'Directories', groups: bucket.directory });
  }
  if (bucket.unassigned.length > 0) {
    sections.push({ title: 'Unassigned', groups: bucket.unassigned });
  }
  return sections;
}

function buildMetadataParts(session: SessionSummary): string[] {
  const metadata: string[] = [];
  if (session.worktree_name) {
    metadata.push(`Worktree: ${session.worktree_name}`);
  }
  const repoDescriptor = describeRepo(session);
  if (repoDescriptor) {
    metadata.push(`Repo: ${repoDescriptor}`);
  }
  if (session.working_dir) {
    metadata.push(`Directory: ${session.working_dir}`);
  }
  return metadata;
}

function truncate(value: string, maxLength: number): string {
  if (value.length <= maxLength) {
    return value;
  }
  return `${value.slice(0, maxLength - 1)}…`;
}

function buildSessionPreview(session: SessionSummary): string {
  if (session.last_user_message && session.last_user_message.trim().length > 0) {
    return truncate(session.last_user_message.trim(), 160);
  }
  for (let index = session.user_messages.length - 1; index >= 0; index -= 1) {
    const candidate = session.user_messages[index];
    if (candidate && candidate.trim().length > 0) {
      return truncate(candidate.trim(), 160);
    }
  }
  return 'No user messages yet';
}

interface SessionGroupSidebarProps {
  groups: SessionGroup[];
  selectedGroupId: string;
  onSelect: (groupId: string) => void;
  isLoading: boolean;
}

function SessionGroupSidebar({
  groups,
  selectedGroupId,
  onSelect,
  isLoading,
}: SessionGroupSidebarProps) {
  const sections = groupSessionGroups(groups);

  return (
    <div className="flex h-full flex-col">
      <div className="border-b border-border px-3 pb-2 pt-3">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
          Session Folders
        </h2>
        <p className="mt-1 text-[0.7rem] text-muted-foreground/80">
          Grouped by worktree or directory
        </p>
      </div>
      {isLoading && groups.length === 0 ? (
        <div className="px-3 py-4 text-sm text-muted-foreground">
          <div className="flex items-center space-x-2">
            <div className="inline-block h-4 w-4 animate-spin rounded-full border-2 border-border border-t-primary" />
            <span>Loading sessions…</span>
          </div>
        </div>
      ) : null}
      <div className="flex-1 overflow-y-auto">
        {sections.length === 0 ? (
          <div className="px-3 py-4 text-xs text-muted-foreground/80">
            {isLoading ? 'Collecting session metadata…' : 'No session groups found.'}
          </div>
        ) : (
          sections.map((section) => (
            <div key={section.title} className="px-2 py-2">
              <p className="px-1 text-[0.65rem] font-semibold uppercase tracking-wide text-muted-foreground/70">
                {section.title}
              </p>
              <div className="mt-1 space-y-1">
                {section.groups.map((group) => {
                  const isSelected = group.id === selectedGroupId;
                  return (
                    <button
                      key={group.id}
                      type="button"
                      onClick={() => onSelect(group.id)}
                      className={cn(
                        'w-full border-l-2 border-transparent px-3 py-2 text-left transition-colors',
                        isSelected
                          ? 'border-primary/70 bg-primary/10 text-foreground'
                          : 'hover:bg-muted',
                      )}
                    >
                      <div className="flex items-center justify-between">
                        <span className="text-sm font-medium text-foreground">
                          {group.label}
                        </span>
                        <span className="text-[0.7rem] text-muted-foreground">
                          {group.count}
                        </span>
                      </div>
                      {group.description ? (
                        <p className="mt-1 text-[0.7rem] text-muted-foreground">
                          {group.description}
                        </p>
                      ) : null}
                    </button>
                  );
                })}
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
interface SessionSummaryListProps {
  sessions: SessionSummary[];
  selectedSessionKey: string | null;
  onSelect: (sessionKey: string) => void;
  searchTerm: string;
  onSearchTermChange: (value: string) => void;
  isLoading: boolean;
  selectedGroup?: SessionGroup;
  formatTimestamp: (value?: string | null) => string;
}

function SessionSummaryList({
  sessions,
  selectedSessionKey,
  onSelect,
  searchTerm,
  onSearchTermChange,
  isLoading,
  selectedGroup,
  formatTimestamp: format,
}: SessionSummaryListProps) {
  const handleSearchChange = (event: ChangeEvent<HTMLInputElement>) => {
    onSearchTermChange(event.target.value);
  };

  return (
    <div className="flex h-full flex-col rounded-lg border border-border bg-card">
      <div className="border-b border-border px-4 py-3">
        <div className="flex flex-col gap-2">
          <div>
            <h3 className="text-sm font-semibold uppercase tracking-wide text-muted-foreground">
              {selectedGroup?.label ?? 'Sessions'}
            </h3>
            {selectedGroup?.description ? (
              <p className="text-xs text-muted-foreground/80">
                {selectedGroup.description}
              </p>
            ) : null}
          </div>
          <input
            type="text"
            value={searchTerm}
            onChange={handleSearchChange}
            placeholder="Search sessions"
            className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground/70 focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
          />
          <div className="flex items-center justify-between text-[0.7rem] text-muted-foreground">
            <span>
              {isLoading ? 'Refreshing…' : `${sessions.length} session${sessions.length === 1 ? '' : 's'}`}
            </span>
          </div>
        </div>
      </div>
      <div className="flex-1 overflow-y-auto">
        {sessions.length === 0 ? (
          <div className="px-4 py-6 text-sm text-muted-foreground/80">
            {isLoading ? 'Loading sessions…' : 'No sessions match your filters.'}
          </div>
        ) : (
          <ul className="divide-y divide-border">
            {sessions.map((session) => {
              const sessionKey = getSessionKey(session);
              const isSelected = sessionKey === selectedSessionKey;
              const preview = buildSessionPreview(session);
              const metadata = buildMetadataParts(session);
              const messageCount = session.user_messages.length;
              return (
                <li key={sessionKey}>
                  <button
                    type="button"
                    onClick={() => onSelect(sessionKey)}
                    className={cn(
                      'flex w-full flex-col gap-2 border-l-2 border-transparent px-4 py-3 text-left transition-colors',
                      isSelected
                        ? 'border-primary/70 bg-primary/10 text-foreground'
                        : 'hover:bg-muted',
                    )}
                  >
                    <div className="flex items-center justify-between gap-3">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="rounded-full bg-blue-50 px-2 py-0.5 text-xs font-medium uppercase tracking-wide text-blue-700">
                          {session.provider}
                        </span>
                        <span className="text-xs text-muted-foreground">
                          {format(session.last_timestamp)}
                        </span>
                        <span className="text-[0.65rem] text-muted-foreground">
                          {messageCount} msg{messageCount === 1 ? '' : 's'}
                        </span>
                      </div>
                      <span className="font-mono text-xs text-muted-foreground" title={session.session_id}>
                        {session.session_id}
                      </span>
                    </div>
                    <p className="text-sm text-foreground/90">{preview}</p>
                    {metadata.length > 0 ? (
                      <div className="flex flex-wrap gap-x-4 gap-y-1 text-[0.7rem] text-muted-foreground">
                        {metadata.map((line) => (
                          <span key={line}>{line}</span>
                        ))}
                      </div>
                    ) : null}
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </div>
  );
}

interface SessionDetailPanelProps {
  selectedSession: SessionSummary | null;
  sessionItems: SessionListItem[];
}

function SessionDetailPanel({ selectedSession, sessionItems }: SessionDetailPanelProps) {
  if (!selectedSession) {
    return (
      <div className="flex h-full items-center justify-center rounded-lg border border-border bg-card px-6 py-10 text-sm text-muted-foreground">
        Select a session on the left to inspect its transcript.
      </div>
    );
  }

  return (
    <SessionListView
      title={`Session ${selectedSession.session_id}`}
      description={selectedSession.working_dir ?? describeRepo(selectedSession)}
      sessions={sessionItems}
      formatTimestamp={formatTimestamp}
      emptyState="No session data."
    />
  );
}
export default function SessionsPage() {
  const { sessions, isLoading } = useSessions();

  const sessionIndex = useMemo(() => buildSessionIndex(sessions), [sessions]);
  const { groups, groupsById, sessionsByGroup, sessionByKey, defaultGroupId } = sessionIndex;

  const [selectedGroupId, setSelectedGroupId] = useState<string>(defaultGroupId);
  const [searchTerm, setSearchTerm] = useState('');
  const [selectedSessionKey, setSelectedSessionKey] = useState<string | null>(null);

  useEffect(() => {
    if (groups.length === 0) {
      setSelectedGroupId('all');
      return;
    }
    if (!groupsById.has(selectedGroupId)) {
      setSelectedGroupId(defaultGroupId);
    }
  }, [groups, groupsById, selectedGroupId, defaultGroupId]);

  const normalizedSearch = useMemo(() => searchTerm.trim().toLowerCase(), [searchTerm]);
  const visibleSessions = useMemo(() => {
    const base = sessionsByGroup.get(selectedGroupId) ?? [];
    if (!normalizedSearch) {
      return base;
    }
    return base.filter((session) => {
      const haystack = [
        session.session_id,
        session.provider,
        session.last_user_message ?? '',
        session.worktree_name ?? '',
        session.worktree_id ?? '',
        session.repo_name ?? '',
        session.branch ?? '',
        session.working_dir ?? '',
      ]
        .join(' ')
        .toLowerCase();
      return haystack.includes(normalizedSearch);
    });
  }, [sessionsByGroup, selectedGroupId, normalizedSearch]);

  useEffect(() => {
    if (visibleSessions.length === 0) {
      if (selectedSessionKey !== null) {
        setSelectedSessionKey(null);
      }
      return;
    }
    const retainsSelection =
      selectedSessionKey !== null &&
      visibleSessions.some((session) => getSessionKey(session) === selectedSessionKey);
    if (!retainsSelection) {
      setSelectedSessionKey(getSessionKey(visibleSessions[0]));
    }
  }, [visibleSessions, selectedSessionKey]);

  const selectedSession = useMemo(
    () => (selectedSessionKey ? sessionByKey.get(selectedSessionKey) ?? null : null),
    [selectedSessionKey, sessionByKey],
  );

  const detailItems = useMemo<SessionListItem[]>(() => {
    if (!selectedSession) {
      return [];
    }
    const metadataParts = buildMetadataParts(selectedSession);
    return [
      {
        sessionKey: getSessionKey(selectedSession),
        provider: selectedSession.provider,
        sessionId: selectedSession.session_id,
        lastTimestamp: selectedSession.last_timestamp,
        userMessages: selectedSession.user_messages,
        metadata:
          metadataParts.length > 0 ? (
            <div className="flex flex-wrap gap-x-4 gap-y-1">
              {metadataParts.map((line) => (
                <span key={line} className="text-xs text-gray-600">
                  {line}
                </span>
              ))}
            </div>
          ) : undefined,
        headerActions: (
          <button
            type="button"
            disabled
            title="Resume session coming soon"
            className="rounded-md border border-gray-200 px-2 py-1 text-xs text-gray-400"
          >
            Resume (soon)
          </button>
        ),
      },
    ];
  }, [selectedSession]);

  const selectedGroup = groupsById.get(selectedGroupId);

  const sidebar = (
    <SessionGroupSidebar
      groups={groups}
      selectedGroupId={selectedGroupId}
      onSelect={setSelectedGroupId}
      isLoading={isLoading}
    />
  );

  const main = (
    <div className="mx-auto flex h-full w-full max-w-6xl flex-col gap-6 px-6 py-6">
      <div className="flex h-full flex-1 flex-col gap-6 lg:flex-row">
        <div className="lg:w-[360px] lg:flex-shrink-0">
          <SessionSummaryList
            sessions={visibleSessions}
            selectedSessionKey={selectedSessionKey}
            onSelect={setSelectedSessionKey}
            searchTerm={searchTerm}
            onSearchTermChange={setSearchTerm}
            isLoading={isLoading && sessions.length === 0}
            selectedGroup={selectedGroup}
            formatTimestamp={formatTimestamp}
          />
        </div>
        <div className="flex-1">
          <SessionDetailPanel selectedSession={selectedSession} sessionItems={detailItems} />
        </div>
      </div>
    </div>
  );

  return <MainLayout sidebar={sidebar} main={main} />;
}
