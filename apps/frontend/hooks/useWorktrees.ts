'use client';

import { useCallback, useEffect, useState } from 'react';
import { WorktreeListResponse, WorktreeSummary } from '@/types';
import { apiUrl } from '@/lib/api';

export function useWorktrees() {
  const [worktrees, setWorktrees] = useState<WorktreeSummary[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchWorktrees = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    try {
      const response = await fetch(apiUrl('/api/worktrees'));
      if (!response.ok) {
        throw new Error(`Failed to fetch worktrees: ${response.statusText}`);
      }

      const payload: WorktreeListResponse = await response.json();
      setWorktrees(payload.worktrees ?? []);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Unknown error';
      setError(message);
      console.error('Error fetching worktrees:', err);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchWorktrees();
    const interval = setInterval(fetchWorktrees, 5000);
    return () => clearInterval(interval);
  }, [fetchWorktrees]);

  return {
    worktrees,
    isLoading,
    error,
    refetch: fetchWorktrees,
  };
}
