'use client';

import { useCallback, useEffect, useState } from 'react';
import { SESSION_DETAIL_MODE_STORAGE_KEY } from '@/lib/session-utils';
import type { SessionDetailMode } from '@/types';

const VALID_MODES: SessionDetailMode[] = ['user_only', 'conversation', 'full'];

export function useSessionDetailMode(
  defaultMode: SessionDetailMode = 'user_only',
): [SessionDetailMode, (mode: SessionDetailMode) => void] {
  const [mode, setMode] = useState<SessionDetailMode>(defaultMode);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return;
    }
    const stored = window.localStorage.getItem(SESSION_DETAIL_MODE_STORAGE_KEY);
    if (stored && VALID_MODES.includes(stored as SessionDetailMode)) {
      setMode(stored as SessionDetailMode);
    }
  }, []);

  const updateMode = useCallback((next: SessionDetailMode) => {
    setMode(next);
    if (typeof window === 'undefined') {
      return;
    }
    try {
      window.localStorage.setItem(SESSION_DETAIL_MODE_STORAGE_KEY, next);
    } catch (error) {
      console.warn('Failed to persist session detail mode preference', error);
    }
  }, []);

  return [mode, updateMode];
}
