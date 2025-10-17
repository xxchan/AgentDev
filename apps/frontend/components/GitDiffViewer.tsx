'use client';

import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from 'react';
import { DiffFile, DiffModeEnum, DiffView } from '@git-diff-view/react';
import clsx from 'clsx';

interface GitDiffViewerProps {
  diffText: string;
  filePath?: string;
  status?: string;
  mode?: DiffModeEnum;
  onModeChange?: (next: DiffModeEnum) => void;
  wrap?: boolean;
  onWrapChange?: (value: boolean) => void;
  showHeader?: boolean;
  autoExpand?: boolean;
  diffFontSize?: number;
  theme?: 'light' | 'dark';
}

type ParsedDiffPaths = {
  oldPath: string | null;
  newPath: string | null;
  label: string;
};

const STATUS_LABELS: Record<string, string> = {
  A: 'Added',
  M: 'Modified',
  D: 'Deleted',
  R: 'Renamed',
  C: 'Copied',
  T: 'Type change',
  U: 'Unmerged',
  '?': 'Untracked',
};

function parseDiffPaths(diffText: string, explicitPath?: string): ParsedDiffPaths {
  const clean = diffText.trim();

  const headerMatch = clean.match(/^diff --git a\/(.+?) b\/(.+)$/m);
  const oldPathFromHeader = headerMatch ? headerMatch[1] ?? null : null;
  const newPathFromHeader = headerMatch ? headerMatch[2] ?? null : null;

  const oldHunkMatch = clean.match(/^---\s+((?:a\/)?[^\n]+)$/m);
  const newHunkMatch = clean.match(/^\+\+\+\s+((?:b\/)?[^\n]+)$/m);
  const oldPathFromHunk = oldHunkMatch ? oldHunkMatch[1] ?? null : null;
  const newPathFromHunk = newHunkMatch ? newHunkMatch[1] ?? null : null;

  const stripPrefix = (value: string | null) => {
    if (!value) return null;
    if (value === '/dev/null') return null;
    return value.replace(/^[ab]\//, '');
  };

  const oldPath = stripPrefix(oldPathFromHeader ?? oldPathFromHunk) ?? explicitPath ?? null;
  const newPath = stripPrefix(newPathFromHeader ?? newPathFromHunk) ?? explicitPath ?? null;

  if (explicitPath) {
    return { oldPath, newPath, label: explicitPath };
  }

  if (oldPath && newPath && oldPath !== newPath) {
    return { oldPath, newPath, label: `${oldPath} → ${newPath}` };
  }

  const fallbackPath = newPath ?? oldPath ?? 'Diff';
  return { oldPath, newPath, label: fallbackPath };
}

function normaliseStatus(status?: string): { code: string | null; label: string | null } {
  if (!status) return { code: null, label: null };
  const firstChar = status.trim().charAt(0);
  const label = STATUS_LABELS[firstChar] ?? null;
  return { code: firstChar || null, label };
}

type DiffViewHandle = {
  getDiffFileInstance: () => DiffFile;
};

export type GitDiffViewerHandle = {
  getDiffFile: () => DiffFile | null;
  expandAll: (mode?: 'split' | 'unified' | 'both') => void;
  collapseAll: (mode?: 'split' | 'unified' | 'both') => void;
};

const GitDiffViewer = forwardRef<GitDiffViewerHandle, GitDiffViewerProps>(
  (
    {
      diffText,
      filePath,
      status,
      mode,
      onModeChange,
      wrap,
      onWrapChange,
      showHeader = true,
      autoExpand = true,
      diffFontSize,
      theme = 'light',
    },
    ref,
  ) => {
    const hasDiff = diffText.trim().length > 0;
    const [modeState, setModeState] = useState<DiffModeEnum>(mode ?? DiffModeEnum.SplitGitHub);
    const [wrapState, setWrapState] = useState<boolean>(wrap ?? false);
    const diffViewRef = useRef<DiffViewHandle | null>(null);

    const parsedPaths = useMemo(() => parseDiffPaths(diffText, filePath), [diffText, filePath]);

    const diffData = useMemo(() => {
      if (!hasDiff) return null;
      return {
        oldFile: {
          fileName: parsedPaths.oldPath ?? parsedPaths.label,
          content: '',
        },
        newFile: {
          fileName: parsedPaths.newPath ?? parsedPaths.label,
          content: '',
        },
        hunks: [diffText],
      };
    }, [diffText, hasDiff, parsedPaths.label, parsedPaths.newPath, parsedPaths.oldPath]);

    const statusInfo = useMemo(() => (showHeader ? normaliseStatus(status) : { code: null, label: null }), [status, showHeader]);

    const effectiveMode = mode ?? modeState;
    const effectiveWrap = wrap ?? wrapState;
    const effectiveFontSize = diffFontSize ?? 12;
    const effectiveTheme = theme;

    const handleCopy = useCallback(async () => {
      try {
        await navigator.clipboard.writeText(diffText);
      } catch (err) {
        console.error('Failed to copy diff to clipboard', err);
      }
    }, [diffText]);

    const expandAll = useCallback(
      (targetMode: 'split' | 'unified' | 'both' = 'both') => {
        const diffFile = diffViewRef.current?.getDiffFileInstance();
        if (!diffFile) return;
        if (targetMode === 'both' || targetMode === 'split') {
          diffFile.onAllExpand('split');
        }
        if (targetMode === 'both' || targetMode === 'unified') {
          diffFile.onAllExpand('unified');
        }
      },
      [],
    );

    const collapseAll = useCallback(
      (targetMode: 'split' | 'unified' | 'both' = 'both') => {
        const diffFile = diffViewRef.current?.getDiffFileInstance();
        if (!diffFile) return;
        if (targetMode === 'both' || targetMode === 'split') {
          diffFile.onAllCollapse('split');
        }
        if (targetMode === 'both' || targetMode === 'unified') {
          diffFile.onAllCollapse('unified');
        }
      },
      [],
    );

    useImperativeHandle(
      ref,
      () => ({
        getDiffFile: () => diffViewRef.current?.getDiffFileInstance() ?? null,
        expandAll,
        collapseAll,
      }),
      [expandAll, collapseAll],
    );

    useEffect(() => {
      if (!hasDiff || !autoExpand) return;

      const raf = window.requestAnimationFrame(() => expandAll('both'));

      return () => window.cancelAnimationFrame(raf);
    }, [autoExpand, expandAll, hasDiff, diffText]);

    useEffect(() => {
      if (mode !== undefined) {
        setModeState(mode);
      }
    }, [mode]);

    useEffect(() => {
      if (wrap !== undefined) {
        setWrapState(wrap);
      }
    }, [wrap]);

    if (!hasDiff || !diffData) {
      return (
        <div className="rounded-md border border-dashed border-gray-300 bg-gray-50 px-4 py-5 text-xs text-gray-500">
          No diff output available for this file.
        </div>
      );
    }

    return (
      <div className={clsx('overflow-hidden rounded-md border', theme === 'light' ? 'border-gray-200 bg-white shadow-sm' : 'border-gray-800 bg-gray-950/95')}>
        {showHeader && (
          <div
            className={clsx(
              'flex flex-wrap items-center justify-between gap-3 border-b px-4 py-2',
              theme === 'light' ? 'border-gray-200 bg-gray-50/95' : 'border-gray-800 bg-gray-900/80',
            )}
          >
            <div className="flex flex-wrap items-center gap-2">
              {statusInfo.code && (
                <span
                  className={clsx(
                    'inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide',
                    theme === 'light' ? 'bg-blue-50 text-blue-600 border border-blue-100' : 'bg-gray-800 text-gray-200',
                  )}
                >
                  {statusInfo.label ?? statusInfo.code}
                </span>
              )}
              <span
                className={clsx(
                  'font-mono text-xs',
                  theme === 'light' ? 'text-gray-700' : 'text-gray-200',
                )}
              >
                {parsedPaths.label}
              </span>
            </div>

            <div className="flex items-center gap-2 text-[10px]">
              <button
                type="button"
                onClick={() => {
                  if (mode === undefined) {
                    setModeState(DiffModeEnum.SplitGitHub);
                  }
                  onModeChange?.(DiffModeEnum.SplitGitHub);
                }}
                className={clsx(
                  'rounded-md border px-2 py-0.5 transition',
                  effectiveMode === DiffModeEnum.SplitGitHub
                    ? theme === 'light'
                      ? 'border-blue-500 bg-blue-500/10 text-blue-600'
                      : 'border-blue-500 bg-blue-500/10 text-blue-200'
                    : theme === 'light'
                      ? 'border-gray-300 text-gray-600 hover:border-gray-400 hover:text-gray-800'
                      : 'border-gray-700 text-gray-300 hover:border-gray-600 hover:text-gray-100',
                )}
              >
                Split
              </button>
              <button
                type="button"
                onClick={() => {
                  if (mode === undefined) {
                    setModeState(DiffModeEnum.Unified);
                  }
                  onModeChange?.(DiffModeEnum.Unified);
                }}
                className={clsx(
                  'rounded-md border px-2 py-0.5 transition',
                  effectiveMode === DiffModeEnum.Unified
                    ? theme === 'light'
                      ? 'border-blue-500 bg-blue-500/10 text-blue-600'
                      : 'border-blue-500 bg-blue-500/10 text-blue-200'
                    : theme === 'light'
                      ? 'border-gray-300 text-gray-600 hover:border-gray-400 hover:text-gray-800'
                      : 'border-gray-700 text-gray-300 hover:border-gray-600 hover:text-gray-100',
                )}
              >
                Inline
              </button>
              <button
                type="button"
                onClick={() => {
                  const next = !(wrap ?? wrapState);
                  if (wrap === undefined) {
                    setWrapState(next);
                  }
                  onWrapChange?.(next);
                }}
                className={clsx(
                  'rounded-md border px-2 py-0.5 transition',
                  effectiveWrap
                    ? theme === 'light'
                      ? 'border-blue-500 bg-blue-500/10 text-blue-600'
                      : 'border-blue-500 bg-blue-500/10 text-blue-200'
                    : theme === 'light'
                      ? 'border-gray-300 text-gray-600 hover:border-gray-400 hover:text-gray-800'
                      : 'border-gray-700 text-gray-300 hover:border-gray-600 hover:text-gray-100',
                )}
              >
                {effectiveWrap ? 'No Wrap' : 'Wrap'}
              </button>
              <button
                type="button"
                onClick={handleCopy}
                className={clsx(
                  'rounded-md border px-2 py-0.5 transition',
                  theme === 'light'
                    ? 'border-gray-300 text-gray-600 hover:border-gray-400 hover:text-gray-800'
                    : 'border-gray-700 text-gray-300 hover:border-gray-500 hover:text-gray-100',
                )}
              >
                Copy Patch
              </button>
            </div>
          </div>
        )}

        <div className={clsx('overflow-auto', theme === 'light' ? 'max-h-[40rem] bg-white' : 'max-h-[32rem] bg-gray-950')}>
          <DiffView
            ref={diffViewRef}
            key={`${parsedPaths.label}:${effectiveMode}:${effectiveWrap}`}
            data={diffData}
            diffViewMode={effectiveMode}
            diffViewWrap={effectiveWrap}
            diffViewTheme={effectiveTheme}
            diffViewFontSize={effectiveFontSize}
            diffViewHighlight={false}
          />
        </div>
      </div>
    );
  },
);

GitDiffViewer.displayName = 'GitDiffViewer';

export default GitDiffViewer;
