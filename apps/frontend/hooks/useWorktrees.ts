'use client';

import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { getJson } from '@/lib/apiClient';
import { queryKeys } from '@/lib/queryKeys';
import type { WorktreeListResponse, WorktreeSummary } from '@/types';

function toErrorMessage(error: unknown): string | null {
  if (!error) {
    return null;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

export function useWorktrees() {
  const query = useQuery({
    queryKey: queryKeys.worktrees.list,
    queryFn: ({ signal }) =>
      getJson<WorktreeListResponse>('/api/worktrees', { signal }).then(
        (response) => response.worktrees ?? [],
      ),
    refetchInterval: 5000,
  });

  const worktrees = useMemo<WorktreeSummary[]>(
    () => query.data ?? [],
    [query.data],
  );

  return {
    worktrees,
    isLoading: query.isLoading && !query.isFetched,
    isFetching: query.isFetching,
    error: toErrorMessage(query.error),
    refetch: query.refetch,
    query,
  };
}
