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

export interface DiscoveryParams {
  recursive: boolean;
  root?: string | null;
}

export function useDiscoveredWorktrees(params: DiscoveryParams | null) {
  const recursive = params?.recursive ?? true;
  const root = params?.root?.trim() ?? null;
  const query = useQuery({
    enabled: params !== null,
    queryKey: queryKeys.worktrees.discovery(recursive, root),
    queryFn: ({ signal }) => {
      if (params === null) {
        return Promise.resolve([] as DiscoveredWorktree[]);
      }
      const searchParams = new URLSearchParams();
      searchParams.set('recursive', String(recursive));
      if (root) {
        searchParams.set('root', root);
      }
      return getJson<DiscoveredWorktree[]>(
        `/api/worktrees/discovery?${searchParams.toString()}`,
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
    hasRequested: params !== null,
  };
}
