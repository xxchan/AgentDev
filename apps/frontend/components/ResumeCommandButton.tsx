'use client';

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AlertCircle, Check, Copy } from 'lucide-react';
import { cn } from '@/lib/utils';

interface ResumeCommandButtonProps {
  provider: string;
  sessionId: string;
  className?: string;
}

type CopyStatus = 'idle' | 'copied' | 'error';

function resolveResumeCommand(provider: string, sessionId: string): string | null {
  const normalized = provider.trim().toLowerCase();
  if (!normalized) {
    return null;
  }
  const base = normalized.split(':')[0];
  if (base === 'codex') {
    return `codex resume ${sessionId}`;
  }
  if (base === 'claude') {
    return `claude resume ${sessionId}`;
  }
  return null;
}

export default function ResumeCommandButton({
  provider,
  sessionId,
  className,
}: ResumeCommandButtonProps) {
  const [status, setStatus] = useState<CopyStatus>('idle');
  const resetTimerRef = useRef<number | null>(null);

  const command = useMemo(() => resolveResumeCommand(provider, sessionId), [provider, sessionId]);
  const isSupported = Boolean(command);

  useEffect(() => {
    if (status === 'idle') {
      return;
    }
    const timer = window.setTimeout(() => {
      setStatus('idle');
    }, 2000);
    resetTimerRef.current = timer;
    return () => {
      window.clearTimeout(timer);
    };
  }, [status]);

  useEffect(
    () => () => {
      if (resetTimerRef.current) {
        window.clearTimeout(resetTimerRef.current);
      }
    },
    [],
  );

  const handleCopy = useCallback(async () => {
    if (!command) {
      return;
    }
    try {
      await navigator.clipboard.writeText(command);
      setStatus('copied');
    } catch (error) {
      console.error('Failed to copy resume command', error);
      setStatus('error');
    }
  }, [command]);

  const statusLabel =
    status === 'copied'
      ? 'Command copied'
      : status === 'error'
        ? 'Copy failed'
        : isSupported
          ? 'Copy resume command'
          : 'Resume not supported';

  const title = isSupported
    ? `Copy "${command}" to your clipboard`
    : `Resume via CLI is not available for "${provider}" yet`;

  const icon =
    status === 'copied' ? (
      <Check className="h-3.5 w-3.5" />
    ) : status === 'error' ? (
      <AlertCircle className="h-3.5 w-3.5" />
    ) : (
      <Copy className="h-3.5 w-3.5" />
    );

  const buttonClassName = cn(
    'inline-flex items-center gap-1 rounded-md border px-2 py-1 text-xs transition',
    isSupported
      ? 'border-gray-300 text-gray-600 hover:border-gray-400 hover:text-gray-800'
      : 'cursor-not-allowed border-gray-200 text-gray-400',
    status === 'copied' && 'border-emerald-300 text-emerald-700 hover:border-emerald-300',
    status === 'error' && 'border-rose-300 text-rose-700 hover:border-rose-300',
    className,
  );

  return (
    <button
      type="button"
      onClick={handleCopy}
      disabled={!isSupported}
      className={buttonClassName}
      title={title}
    >
      {icon}
      <span>{statusLabel}</span>
    </button>
  );
}
