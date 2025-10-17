'use client';

import { useCallback, useEffect, useState } from 'react';
import { SessionListResponse, SessionProviderSummary, SessionSummary } from '@/types';
import { apiUrl } from '@/lib/api';

export function useSessions() {
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [providers, setProviders] = useState<SessionProviderSummary[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchSessions = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    try {
      const response = await fetch(apiUrl('/api/sessions'));
      if (!response.ok) {
        throw new Error(`Failed to fetch sessions: ${response.statusText}`);
      }

      const payload: SessionListResponse = await response.json();
      setSessions(payload.sessions ?? []);
      if (Array.isArray(payload.providers)) {
        setProviders(payload.providers);
      } else {
        setProviders([]);
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Unknown error';
      setError(message);
      console.error('Error fetching sessions:', err);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchSessions();
    const interval = setInterval(fetchSessions, 5000);
    return () => clearInterval(interval);
  }, [fetchSessions]);

  return {
    sessions,
    providers,
    isLoading,
    error,
    refetch: fetchSessions,
  };
}
