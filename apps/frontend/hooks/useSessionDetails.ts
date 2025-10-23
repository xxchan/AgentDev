'use client';

import { useCallback } from 'react';
import {
  useQuery,
  useQueryClient,
  type UseQueryOptions,
  type UseQueryResult,
} from '@tanstack/react-query';
import { getJson } from '@/lib/apiClient';
import { queryKeys } from '@/lib/queryKeys';
import type { SessionDetailMode, SessionDetailResponse } from '@/types';

export interface SessionDetailQueryArgs {
  provider: string;
  sessionId: string;
  mode: SessionDetailMode;
}

export interface RequestSessionDetailArgs extends SessionDetailQueryArgs {
  force?: boolean;
}

function buildSessionDetailPath({ provider, sessionId, mode }: SessionDetailQueryArgs) {
  return `/api/sessions/${encodeURIComponent(provider)}/${encodeURIComponent(sessionId)}?mode=${mode}`;
}

export function sessionDetailQueryOptions(args: SessionDetailQueryArgs) {
  const { provider, sessionId, mode } = args;
  return {
    queryKey: queryKeys.sessions.detail(provider, sessionId, mode),
    queryFn: ({ signal }) =>
      getJson<SessionDetailResponse>(buildSessionDetailPath(args), { signal }),
    staleTime: 30_000,
  } satisfies UseQueryOptions<
    SessionDetailResponse,
    unknown,
    SessionDetailResponse,
    ReturnType<typeof queryKeys.sessions.detail>
  >;
}

export function useSessionDetailQuery(
  args: SessionDetailQueryArgs & { enabled?: boolean },
): UseQueryResult<SessionDetailResponse> {
  const { enabled = true, ...rest } = args;
  const options = sessionDetailQueryOptions(rest);
  return useQuery({
    ...options,
    enabled,
  });
}

export function useSessionDetails() {
  const queryClient = useQueryClient();

  const getDetail = useCallback(
    ({ provider, sessionId, mode }: SessionDetailQueryArgs) =>
      queryClient.getQueryData<SessionDetailResponse>(
        queryKeys.sessions.detail(provider, sessionId, mode),
      ) ?? null,
    [queryClient],
  );

  const getError = useCallback(
    ({ provider, sessionId, mode }: SessionDetailQueryArgs) => {
      const state = queryClient.getQueryState<SessionDetailResponse, unknown>(
        queryKeys.sessions.detail(provider, sessionId, mode),
      );
      const error = state?.error;
      if (!error) {
        return null;
      }
      if (error instanceof Error) {
        return error.message;
      }
      return String(error);
    },
    [queryClient],
  );

  const isFetching = useCallback(
    ({ provider, sessionId, mode }: SessionDetailQueryArgs) => {
      const state = queryClient.getQueryState<SessionDetailResponse>(
        queryKeys.sessions.detail(provider, sessionId, mode),
      );
      return state?.fetchStatus === 'fetching';
    },
    [queryClient],
  );

  const requestDetail = useCallback(
    async ({ provider, sessionId, mode, force = false }: RequestSessionDetailArgs) => {
      const queryKey = queryKeys.sessions.detail(provider, sessionId, mode);
      const existing = queryClient.getQueryState<SessionDetailResponse>(queryKey);
      if (!force && (existing?.data || existing?.fetchStatus === 'fetching')) {
        return;
      }
      await queryClient.fetchQuery(sessionDetailQueryOptions({ provider, sessionId, mode }));
    },
    [queryClient],
  );

  return {
    getDetail,
    getError,
    requestDetail,
    isFetching,
    queryClient,
  };
}
