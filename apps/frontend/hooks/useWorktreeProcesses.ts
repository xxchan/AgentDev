'use client';

import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { getJson } from '@/lib/apiClient';
import { queryKeys } from '@/lib/queryKeys';
import type {
  WorktreeProcessListResponse,
  WorktreeProcessSummary,
} from '@/types';

interface UseWorktreeProcessesOptions {
  pollIntervalMs?: number;
}

function toErrorMessage(error: unknown): string | null {
  if (!error) {
    return null;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

export function useWorktreeProcesses(
  worktreeId: string | null,
  { pollIntervalMs = 5000 }: UseWorktreeProcessesOptions = {},
) {
  const queryKey = worktreeId
    ? queryKeys.worktrees.processes(worktreeId)
    : (['worktrees', 'processes', 'none'] as const);

  const query = useQuery({
    queryKey,
    queryFn: async ({ signal }) => {
      if (!worktreeId) {
        return [] as WorktreeProcessSummary[];
      }
      const response = await getJson<WorktreeProcessListResponse>(
        `/api/worktrees/${encodeURIComponent(worktreeId)}/processes`,
        { signal },
      );
      return response.processes ?? [];
    },
    enabled: Boolean(worktreeId),
    refetchInterval: worktreeId ? pollIntervalMs : false,
    refetchOnMount: true,
  });

  const processes = useMemo<WorktreeProcessSummary[]>(
    () => query.data ?? [],
    [query.data],
  );

  return {
    processes,
    isLoading: query.isLoading && Boolean(worktreeId) && !query.isFetched,
    isFetching: query.isFetching,
    error: toErrorMessage(query.error),
    refetch: query.refetch,
    query,
  };
}
