'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
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

  const detailCacheRef = useRef(detailCache);
  const detailErrorsRef = useRef(detailErrors);
  const loadingMapRef = useRef(loadingMap);

  useEffect(() => {
    detailCacheRef.current = detailCache;
  }, [detailCache]);

  useEffect(() => {
    detailErrorsRef.current = detailErrors;
  }, [detailErrors]);

  useEffect(() => {
    loadingMapRef.current = loadingMap;
  }, [loadingMap]);

  const requestDetail = useCallback(
    async ({ provider, sessionId, mode, signal, force = false }: RequestSessionDetailArgs) => {
      const sessionKey = getSessionKey({ provider, session_id: sessionId });
      const detailKey = buildDetailCacheKey(sessionKey, mode);

      const currentCache = detailCacheRef.current;
      const currentLoading = loadingMapRef.current;

      if (!force && (currentCache[detailKey] || currentLoading[detailKey])) {
        return;
      }

      setLoadingMap((prev) => {
        const next = { ...prev, [detailKey]: true };
        loadingMapRef.current = next;
        return next;
      });
      setDetailErrors((prev) => {
        if (!(detailKey in prev)) {
          detailErrorsRef.current = prev;
          return prev;
        }
        const next = { ...prev };
        delete next[detailKey];
        detailErrorsRef.current = next;
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

        setDetailCache((prev) => {
          if (prev[detailKey] === detail) {
            return prev;
          }
          const next = { ...prev, [detailKey]: detail };
          detailCacheRef.current = next;
          return next;
        });
      } catch (error) {
        if (signal?.aborted) {
          return;
        }
        const message = error instanceof Error ? error.message : 'Unknown error';
        setDetailErrors((prev) => {
          const next = { ...prev, [detailKey]: message };
          detailErrorsRef.current = next;
          return next;
        });
        console.error('Failed to load session detail', error);
      } finally {
        setLoadingMap((prev) => {
          if (!(detailKey in prev)) {
            loadingMapRef.current = prev;
            return prev;
          }
          const next = { ...prev };
          delete next[detailKey];
          loadingMapRef.current = next;
          return next;
        });
      }
    },
    [],
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
