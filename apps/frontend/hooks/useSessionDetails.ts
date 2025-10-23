'use client';

import { useCallback, useState } from 'react';
import { apiUrl } from '@/lib/api';
import { buildDetailCacheKey, getSessionKey } from '@/lib/session-utils';
import type { SessionDetailMode, SessionDetailResponse } from '@/types';

interface RequestSessionDetailArgs {
  provider: string;
  sessionId: string;
  mode: SessionDetailMode;
  signal?: AbortSignal;
  force?: boolean;
}

type DetailCache = Record<string, SessionDetailResponse>;
type DetailErrors = Record<string, string>;
type DetailLoadingMap = Record<string, boolean>;

export function useSessionDetails() {
  const [detailCache, setDetailCache] = useState<DetailCache>({});
  const [detailErrors, setDetailErrors] = useState<DetailErrors>({});
  const [loadingMap, setLoadingMap] = useState<DetailLoadingMap>({});

  const requestDetail = useCallback(
    async ({ provider, sessionId, mode, signal, force = false }: RequestSessionDetailArgs) => {
      const sessionKey = getSessionKey({ provider, session_id: sessionId });
      const detailKey = buildDetailCacheKey(sessionKey, mode);

      if (!force && (detailCache[detailKey] || loadingMap[detailKey])) {
        return;
      }

      setLoadingMap((prev) => ({ ...prev, [detailKey]: true }));
      setDetailErrors((prev) => {
        if (!(detailKey in prev)) {
          return prev;
        }
        const next = { ...prev };
        delete next[detailKey];
        return next;
      });

      try {
        const response = await fetch(
          apiUrl(
            `/api/sessions/${encodeURIComponent(provider)}/${encodeURIComponent(sessionId)}?mode=${mode}`,
          ),
          { signal },
        );

        if (!response.ok) {
          throw new Error(`Failed to load session transcript: ${response.statusText}`);
        }

        const detail: SessionDetailResponse = await response.json();
        if (signal?.aborted) {
          return;
        }

        setDetailCache((prev) => ({ ...prev, [detailKey]: detail }));
      } catch (error) {
        if (signal?.aborted) {
          return;
        }
        const message = error instanceof Error ? error.message : 'Unknown error';
        setDetailErrors((prev) => ({ ...prev, [detailKey]: message }));
        console.error('Failed to load session detail', error);
      } finally {
        setLoadingMap((prev) => {
          if (!(detailKey in prev)) {
            return prev;
          }
          const next = { ...prev };
          delete next[detailKey];
          return next;
        });
      }
    },
    [detailCache, loadingMap],
  );

  const isLoading = useCallback(
    (key?: string | null) => Boolean(key && loadingMap[key]),
    [loadingMap],
  );

  return {
    detailCache,
    detailErrors,
    requestDetail,
    isLoading,
  };
}
