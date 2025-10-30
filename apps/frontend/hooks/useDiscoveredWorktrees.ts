'use client';

import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { getJson } from '@/lib/apiClient';
import { queryKeys } from '@/lib/queryKeys';
import type { DiscoveredWorktree } from '@/types';

function toErrorMessage(error: unknown): string | null {
  if (!error) {
    return null;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

export function useDiscoveredWorktrees(recursive: boolean = true) {
  const query = useQuery({
    queryKey: queryKeys.worktrees.discovery(recursive),
    queryFn: ({ signal }) => {
      const search = recursive ? '1' : '0';
      return getJson<DiscoveredWorktree[]>(
        `/api/worktrees/discovery?recursive=${search}`,
        { signal },
      );
    },
    staleTime: 60_000,
    refetchInterval: false,
  });

  const worktrees = useMemo<DiscoveredWorktree[]>(
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
