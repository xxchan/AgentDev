'use client';

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AlertCircle, Check, Copy, Play } from 'lucide-react';
import { useLaunchWorktreeShell } from '@/features/command/hooks/useLaunchWorktreeShell';
import { ApiError } from '@/lib/apiClient';
import { cn } from '@/lib/utils';

interface ResumeCommandButtonProps {
  provider: string;
  sessionId: string;
  worktreeId?: string | null;
  workingDir?: string | null;
  className?: string;
}

type ActionState =
  | { kind: 'idle' }
  | { kind: 'copying' }
  | { kind: 'copy-success' }
  | { kind: 'copy-error'; message: string }
  | { kind: 'launching' }
  | { kind: 'launch-success' }
  | { kind: 'launch-error'; message: string };

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

function toErrorMessage(error: unknown): string {
  if (error instanceof ApiError) {
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return 'Unexpected error';
}

export default function ResumeCommandButton({
  provider,
  sessionId,
  worktreeId,
  workingDir,
  className,
}: ResumeCommandButtonProps) {
  const [state, setState] = useState<ActionState>({ kind: 'idle' });
  const resetTimerRef = useRef<number | null>(null);
  const { mutateAsync: launchShell } = useLaunchWorktreeShell();

  const command = useMemo(() => resolveResumeCommand(provider, sessionId), [provider, sessionId]);
  const canResume = Boolean(command);
  const normalizedWorktreeId = useMemo(() => (worktreeId ?? '').trim(), [worktreeId]);
  const normalizedWorkingDir = useMemo(() => (workingDir ?? '').trim(), [workingDir]);
  const canLaunchFromWorktree = normalizedWorktreeId.length > 0;
  const canLaunchFromDirectory = normalizedWorkingDir.length > 0;
  const canLaunch = Boolean(canResume && (canLaunchFromWorktree || canLaunchFromDirectory));
  const isBusy = state.kind === 'copying' || state.kind === 'launching';

  useEffect(() => {
    if (state.kind === 'idle' || state.kind === 'copying' || state.kind === 'launching') {
      return;
    }
    const timer = window.setTimeout(() => {
      setState({ kind: 'idle' });
    }, 2500);
    resetTimerRef.current = timer;
    return () => {
      window.clearTimeout(timer);
    };
  }, [state]);

  useEffect(() => {
    return () => {
      if (resetTimerRef.current) {
        window.clearTimeout(resetTimerRef.current);
      }
    };
  }, []);

  const launchResume = useCallback(async () => {
    if (!command || (!canLaunchFromWorktree && !canLaunchFromDirectory)) {
      return;
    }

    setState({ kind: 'launching' });
    try {
      if (canLaunchFromWorktree) {
        await launchShell({
          type: 'worktree',
          worktreeId: normalizedWorktreeId,
          command,
        });
      } else {
        await launchShell({
          type: 'directory',
          workingDir: normalizedWorkingDir,
          command,
        });
      }
      setState({ kind: 'launch-success' });
    } catch (error) {
      console.error('Failed to launch resume command', error);
      setState({
        kind: 'launch-error',
        message: toErrorMessage(error),
      });
    }
  }, [canLaunchFromDirectory, canLaunchFromWorktree, command, launchShell, normalizedWorktreeId, normalizedWorkingDir]);

  const copyCommand = useCallback(async () => {
    if (!command) {
      return;
    }

    setState({ kind: 'copying' });
    try {
      await navigator.clipboard.writeText(command);
      setState({ kind: 'copy-success' });
    } catch (error) {
      console.error('Failed to copy resume command', error);
      setState({
        kind: 'copy-error',
        message: toErrorMessage(error),
      });
    }
  }, [command]);

  const handleClick = useCallback(() => {
    if (!command || isBusy) {
      return;
    }
    if (canLaunch) {
      void launchResume();
    } else {
      void copyCommand();
    }
  }, [canLaunch, command, copyCommand, isBusy, launchResume]);

  const errorMessage =
    state.kind === 'copy-error' || state.kind === 'launch-error' ? state.message : null;

  const label = useMemo(() => {
    if (!canResume) {
      return 'Resume unavailable';
    }
    if (canLaunch) {
      switch (state.kind) {
        case 'launching':
          return 'Launching…';
        case 'launch-success':
          return 'Resume launched';
        case 'launch-error':
          return 'Launch failed';
        default:
          return 'Resume in shell';
      }
    }
    switch (state.kind) {
      case 'copying':
        return 'Copying…';
      case 'copy-success':
        return 'Command copied';
      case 'copy-error':
        return 'Copy failed';
      default:
        return 'Copy resume command';
    }
  }, [canLaunch, canResume, state]);

  const title = useMemo(() => {
    if (!canResume) {
      return `Resume via CLI is not available for "${provider}" yet`;
    }
    if (canLaunch) {
      if (errorMessage) {
        return `Failed to launch: ${errorMessage}`;
      }
      if (canLaunchFromWorktree) {
        return `Launch "${command}" in a shell rooted at this worktree`;
      }
      const target = normalizedWorkingDir || 'the session directory';
      return `Launch "${command}" in a shell rooted at ${target}`;
    }
    if (errorMessage) {
      return `Failed to copy: ${errorMessage}`;
    }
    return `Copy "${command}" to your clipboard`;
  }, [canLaunch, canLaunchFromWorktree, canResume, command, errorMessage, normalizedWorkingDir, provider]);

  const icon = useMemo(() => {
    if (!canResume) {
      return <AlertCircle className="h-3.5 w-3.5" />;
    }
    if (state.kind === 'launching') {
      return (
        <span className="inline-block h-3 w-3 animate-spin rounded-full border-2 border-border border-t-primary" />
      );
    }
    if (state.kind === 'copying') {
      return (
        <span className="inline-block h-3 w-3 animate-spin rounded-full border-2 border-border border-t-primary" />
      );
    }
    if (state.kind === 'launch-success' || state.kind === 'copy-success') {
      return <Check className="h-3.5 w-3.5" />;
    }
    if (state.kind === 'launch-error' || state.kind === 'copy-error') {
      return <AlertCircle className="h-3.5 w-3.5" />;
    }
    if (canLaunch) {
      return <Play className="h-3.5 w-3.5" />;
    }
    return <Copy className="h-3.5 w-3.5" />;
  }, [canLaunch, canResume, state]);

  const buttonClassName = cn(
    'inline-flex items-center gap-1 rounded-md border px-2 py-1 text-xs transition',
    !canResume && 'cursor-not-allowed border-gray-200 text-gray-400',
    canResume && !canLaunch && 'border-gray-300 text-gray-600 hover:border-gray-400 hover:text-gray-800',
    canLaunch && 'border-blue-200 text-blue-600 hover:border-blue-300 hover:text-blue-700',
    (state.kind === 'launch-success' || state.kind === 'copy-success') &&
      'border-emerald-300 text-emerald-700 hover:border-emerald-300',
    (state.kind === 'launch-error' || state.kind === 'copy-error') &&
      'border-rose-300 text-rose-700 hover:border-rose-300',
    className,
  );

  return (
    <button
      type="button"
      onClick={handleClick}
      disabled={!canResume || isBusy}
      className={buttonClassName}
      title={title}
    >
      {icon}
      <span>{label}</span>
    </button>
  );
}
