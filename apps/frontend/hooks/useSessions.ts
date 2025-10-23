'use client';

import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { getJson } from '@/lib/apiClient';
import { queryKeys } from '@/lib/queryKeys';
import type {
  SessionListResponse,
  SessionProviderSummary,
  SessionSummary,
} from '@/types';

function toErrorMessage(error: unknown): string | null {
  if (!error) {
    return null;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

export function useSessions() {
  const query = useQuery({
    queryKey: queryKeys.sessions.list,
    queryFn: ({ signal }) => getJson<SessionListResponse>('/api/sessions', { signal }),
    refetchInterval: 5000,
  });

  const sessionsData = query.data?.sessions;
  const providersData = query.data?.providers;

  const sessions = useMemo<SessionSummary[]>(
    () => sessionsData ?? [],
    [sessionsData],
  );

  const providers = useMemo<SessionProviderSummary[]>(
    () => (Array.isArray(providersData) ? providersData : []),
    [providersData],
  );

  return {
    sessions,
    providers,
    isLoading: query.isLoading && !query.isFetched,
    isFetching: query.isFetching,
    error: toErrorMessage(query.error),
    refetch: query.refetch,
    query,
  };
}
