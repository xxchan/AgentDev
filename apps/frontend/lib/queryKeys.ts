import type { SessionDetailMode } from '@/types';

export const queryKeys = {
  worktrees: {
    list: ['worktrees', 'list'] as const,
    detail: (id: string) => ['worktrees', 'detail', id] as const,
    processes: (id: string) => ['worktrees', 'processes', id] as const,
    git: (id: string) => ['worktrees', 'git', id] as const,
    discovery: (recursive: boolean, root: string | null) =>
      ['worktrees', 'discovery', recursive, root] as const,
  },
  sessions: {
    list: ['sessions', 'list'] as const,
    detail: (provider: string, sessionId: string, mode: SessionDetailMode) =>
      ['sessions', 'detail', provider, sessionId, mode] as const,
  },
} as const;

export type QueryKey =
  | (typeof queryKeys.worktrees.list)
  | ReturnType<(typeof queryKeys.worktrees)['detail']>
  | ReturnType<(typeof queryKeys.worktrees)['processes']>
  | ReturnType<(typeof queryKeys.worktrees)['git']>
  | ReturnType<(typeof queryKeys.worktrees)['discovery']>
  | (typeof queryKeys.sessions.list)
  | ReturnType<(typeof queryKeys.sessions)['detail']>;
