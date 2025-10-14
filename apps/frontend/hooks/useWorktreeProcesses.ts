'use client';

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  WorktreeProcessListResponse,
  WorktreeProcessSummary,
} from '@/types';
import { apiUrl } from '@/lib/api';

interface UseWorktreeProcessesOptions {
  pollIntervalMs?: number;
}

export function useWorktreeProcesses(
  worktreeId: string | null,
  { pollIntervalMs = 5000 }: UseWorktreeProcessesOptions = {},
) {
  const [processes, setProcesses] = useState<WorktreeProcessSummary[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const abortControllerRef = useRef<AbortController | null>(null);

  const endpoint = useMemo(() => {
    if (!worktreeId) {
      return null;
    }
    const encoded = encodeURIComponent(worktreeId);
    return `/api/worktrees/${encoded}/processes`;
  }, [worktreeId]);

  const fetchProcesses = useCallback(async () => {
    if (!endpoint) {
      setProcesses([]);
      setIsLoading(false);
      setError(null);
      return;
    }

    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }

    const controller = new AbortController();
    abortControllerRef.current = controller;

    setIsLoading(true);
    setError(null);

    try {
      const response = await fetch(apiUrl(endpoint), {
        signal: controller.signal,
      });

      if (!response.ok) {
        throw new Error(`Failed to fetch processes: ${response.statusText}`);
      }

      const payload: WorktreeProcessListResponse = await response.json();
      setProcesses(payload.processes ?? []);
    } catch (err) {
      if ((err as Error).name === 'AbortError') {
        return;
      }
      const message = err instanceof Error ? err.message : 'Unknown error';
      setError(message);
      setProcesses([]);
      console.error('Error fetching worktree processes:', err);
    } finally {
      setIsLoading(false);
    }
  }, [endpoint]);

  useEffect(() => {
    fetchProcesses();

    if (!endpoint) {
      return () => undefined;
    }

    const interval = window.setInterval(fetchProcesses, pollIntervalMs);
    return () => {
      window.clearInterval(interval);
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }
    };
  }, [endpoint, fetchProcesses, pollIntervalMs]);

  return {
    processes,
    isLoading,
    error,
    refetch: fetchProcesses,
  };
}
