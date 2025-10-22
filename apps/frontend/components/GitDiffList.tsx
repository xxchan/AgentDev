'use client';

import {
  ComponentPropsWithoutRef,
  RefObject,
  forwardRef,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { DiffModeEnum } from '@git-diff-view/react';
import clsx from 'clsx';
import { Virtuoso, VirtuosoHandle } from 'react-virtuoso';

import GitDiffViewer, { GitDiffViewerHandle } from '@/components/GitDiffViewer';

const DIFF_MODE_STORAGE_KEY = 'agentdev.diff.mode';
const DIFF_WRAP_STORAGE_KEY = 'agentdev.diff.wrap';

function readStoredDiffMode(): DiffModeEnum {
  if (typeof window === 'undefined') {
    return DiffModeEnum.SplitGitHub;
  }

  try {
    const raw = window.localStorage.getItem(DIFF_MODE_STORAGE_KEY);
    if (!raw) {
      return DiffModeEnum.SplitGitHub;
    }

    const parsed = Number.parseInt(raw, 10);
    if (Number.isNaN(parsed)) {
      return DiffModeEnum.SplitGitHub;
    }

    switch (parsed) {
      case DiffModeEnum.SplitGitHub:
      case DiffModeEnum.SplitGitLab:
      case DiffModeEnum.Split:
      case DiffModeEnum.Unified:
        return parsed as DiffModeEnum;
      default:
        return DiffModeEnum.SplitGitHub;
    }
  } catch (error) {
    console.warn('Failed to read diff mode preference from localStorage', error);
    return DiffModeEnum.SplitGitHub;
  }
}

function readStoredWrapPreference(): boolean {
  if (typeof window === 'undefined') {
    return false;
  }

  try {
    const raw = window.localStorage.getItem(DIFF_WRAP_STORAGE_KEY);
    if (raw === null) {
      return false;
    }
    return raw === 'true';
  } catch (error) {
    console.warn('Failed to read diff wrap preference from localStorage', error);
    return false;
  }
}

type DiffListRow =
  | { kind: 'group'; key: string; label: string; count: number }
  | { kind: 'diff'; key: string; entry: GitDiffListEntry }
  | { kind: 'empty'; key: string; message: string };

export interface GitDiffListEntry {
  key: string;
  title: string;
  groupKey: string;
  groupLabel: string;
  status?: string | null;
  statusLabel?: string | null;
  diffText: string;
  additions: number;
  deletions: number;
}

interface GitDiffListProps {
  entries: GitDiffListEntry[];
  emptyMessage?: string;
  scrollContainerRef?: RefObject<HTMLElement>;
}

const STATUS_COLORS: Record<string, string> = {
  Added: 'bg-emerald-50 text-emerald-600 border-emerald-200',
  Modified: 'bg-blue-50 text-blue-600 border-blue-200',
  Deleted: 'bg-rose-50 text-rose-600 border-rose-200',
  Renamed: 'bg-purple-50 text-purple-600 border-purple-200',
  Untracked: 'bg-yellow-50 text-yellow-600 border-yellow-200',
  'Type change': 'bg-orange-50 text-orange-600 border-orange-200',
  Copied: 'bg-sky-50 text-sky-600 border-sky-200',
  Unmerged: 'bg-amber-50 text-amber-600 border-amber-200',
  Commit: 'bg-gray-100 text-gray-700 border-gray-300',
};

function formatStatusLabel(label?: string | null) {
  if (!label) return null;
  if (label.length <= 12) return label;
  return label.slice(0, 12);
}

const VirtuosoScroller = forwardRef<HTMLDivElement, ComponentPropsWithoutRef<'div'>>(
  ({ style, ...props }, ref) => (
    <div
      {...props}
      ref={ref}
      style={{ ...(style ?? {}), overflowY: 'visible' }}
    />
  ),
);
VirtuosoScroller.displayName = 'VirtuosoScroller';
const VirtuosoFooter = () => <div className="h-8" />;

export default function GitDiffList({
  entries,
  emptyMessage = 'No diff output available.',
  scrollContainerRef,
}: GitDiffListProps) {
  const [mode, setMode] = useState<DiffModeEnum>(() => readStoredDiffMode());
  const [wrap, setWrap] = useState<boolean>(() => readStoredWrapPreference());
  const [query, setQuery] = useState('');
  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const headerRef = useRef<HTMLDivElement>(null);
  const [customScrollParent, setCustomScrollParent] = useState<HTMLElement | null>(null);

  const filteredEntries = useMemo(() => {
    if (!query.trim()) return entries;
    const term = query.trim().toLowerCase();
    return entries.filter((entry) => {
      const haystack = `${entry.title} ${entry.groupLabel} ${entry.status ?? ''} ${entry.statusLabel ?? ''}`.toLowerCase();
      return haystack.includes(term);
    });
  }, [entries, query]);

  const [openMap, setOpenMap] = useState<Record<string, boolean>>({});

  useEffect(() => {
    setOpenMap((prev) => {
      const next: Record<string, boolean> = {};
      let hasOpen = false;
      filteredEntries.forEach((entry, index) => {
        if (prev[entry.key]) {
          next[entry.key] = true;
          hasOpen = true;
        }
        if (!hasOpen && index === 0) {
          next[entry.key] = true;
          hasOpen = true;
        }
      });
      return next;
    });
  }, [filteredEntries]);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return;
    }

    try {
      window.localStorage.setItem(DIFF_MODE_STORAGE_KEY, mode.toString());
    } catch (error) {
      console.warn('Failed to persist diff mode preference to localStorage', error);
    }
  }, [mode]);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return;
    }

    try {
      window.localStorage.setItem(DIFF_WRAP_STORAGE_KEY, wrap ? 'true' : 'false');
    } catch (error) {
      console.warn('Failed to persist diff wrap preference to localStorage', error);
    }
  }, [wrap]);

  const rows = useMemo<DiffListRow[]>(() => {
    if (!filteredEntries.length) {
      return [
        {
          kind: 'empty' as const,
          key: 'empty',
          message: query.trim() ? 'No files match your filter.' : emptyMessage,
        },
      ];
    }

    const groupMap = new Map<
      string,
      {
        label: string;
        items: GitDiffListEntry[];
        order: number;
      }
    >();
    let order = 0;
    filteredEntries.forEach((entry) => {
      if (!groupMap.has(entry.groupKey)) {
        groupMap.set(entry.groupKey, { label: entry.groupLabel, items: [], order: order++ });
      }
      groupMap.get(entry.groupKey)!.items.push(entry);
    });

    return [...groupMap.values()]
      .sort((a, b) => a.order - b.order)
      .flatMap<DiffListRow>((group) => [
        {
          kind: 'group',
          key: `group:${group.label}`,
          label: group.label,
          count: group.items.length,
        } as const,
        ...group.items.map<DiffListRow>((entry) => ({
          kind: 'diff',
          key: entry.key,
          entry,
        })),
      ]);
  }, [filteredEntries, emptyMessage, query]);

  const handleModeChange = useCallback((next: DiffModeEnum) => {
    setMode(next);
  }, []);

  const handleWrapToggle = useCallback(() => {
    setWrap((prev) => !prev);
  }, []);

  const toggleOpen = useCallback((key: string) => {
    setOpenMap((prev) => ({ ...prev, [key]: !prev[key] }));
  }, []);

  useEffect(() => {
    if (!scrollContainerRef) {
      setCustomScrollParent(null);
      return;
    }
    setCustomScrollParent(scrollContainerRef.current ?? null);
  }, [scrollContainerRef]);

  const openCount = useMemo(
    () => Object.values(openMap).filter(Boolean).length,
    [openMap],
  );

  return (
    <div className="relative">
      <div
        ref={headerRef}
        className="sticky top-[72px] z-20 flex flex-wrap items-center gap-3 rounded-lg border border-gray-200 bg-white/90 px-4 py-3 shadow-sm backdrop-blur supports-[backdrop-filter]:bg-white/70 dark:border-gray-700 dark:bg-gray-900/80"
      >
        <div className="flex flex-1 flex-wrap items-center gap-3">
          <h4 className="text-sm font-semibold uppercase tracking-wide text-gray-800 dark:text-gray-100">
            Diff Files
          </h4>
          <span className="rounded-full border border-blue-500/30 bg-blue-500/10 px-2 py-0.5 text-[10px] font-medium text-blue-500">
            {filteredEntries.length}
          </span>
          <span className="hidden items-center gap-1 rounded-full border border-emerald-500/30 bg-emerald-500/10 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide text-emerald-400 sm:inline-flex">
            Virtual Scroll
          </span>
          <div className="ml-auto flex items-center gap-2 sm:ml-0">
            <button
              type="button"
              onClick={() => handleModeChange(DiffModeEnum.SplitGitHub)}
              className={clsx(
                'rounded-md border px-2 py-0.5 text-[10px] transition',
                mode === DiffModeEnum.SplitGitHub
                  ? 'border-blue-500 bg-blue-500/10 text-blue-200'
                  : 'border-gray-300 text-gray-600 hover:border-gray-400 hover:text-gray-800 dark:border-gray-600 dark:text-gray-300 dark:hover:border-gray-500',
              )}
            >
              Split
            </button>
            <button
              type="button"
              onClick={() => handleModeChange(DiffModeEnum.Unified)}
              className={clsx(
                'rounded-md border px-2 py-0.5 text-[10px] transition',
                mode === DiffModeEnum.Unified
                  ? 'border-blue-500 bg-blue-500/10 text-blue-200'
                  : 'border-gray-300 text-gray-600 hover:border-gray-400 hover:text-gray-800 dark:border-gray-600 dark:text-gray-300 dark:hover:border-gray-500',
              )}
            >
              Inline
            </button>
            <button
              type="button"
              onClick={handleWrapToggle}
              className={clsx(
                'rounded-md border px-2 py-0.5 text-[10px] transition',
                wrap
                  ? 'border-blue-500 bg-blue-500/10 text-blue-200'
                  : 'border-gray-300 text-gray-600 hover:border-gray-400 hover:text-gray-800 dark:border-gray-600 dark:text-gray-300 dark:hover:border-gray-500',
              )}
            >
              {wrap ? 'No Wrap' : 'Wrap'}
            </button>
          </div>
        </div>
        <div className="flex w-full flex-col gap-2 sm:flex-row sm:items-center">
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Filter by file name or status…"
            className="w-full rounded-md border border-gray-300 bg-white px-3 py-2 text-sm text-gray-700 shadow-inner focus:border-blue-500 focus:outline-none focus:ring-2 focus:ring-blue-500/40 dark:border-gray-600 dark:bg-gray-950 dark:text-gray-200"
          />
          <span className="text-[10px] text-gray-500 dark:text-gray-400 sm:text-right">
            {openCount} {openCount === 1 ? 'panel open' : 'panels open'}
          </span>
        </div>
      </div>

      <div
        className={clsx('mt-4', !customScrollParent && 'h-[72vh]')}
        data-virt-scroll
      >
        <Virtuoso
          ref={virtuosoRef}
          data={rows}
          overscan={200}
          customScrollParent={customScrollParent ?? undefined}
          components={
            customScrollParent
              ? { Footer: VirtuosoFooter, Scroller: VirtuosoScroller }
              : { Footer: VirtuosoFooter }
          }
          itemContent={(index, item) => {
            if (item.kind === 'group') {
              return (
                <div
                  className="sticky top-[calc(72px+54px)] z-10 flex items-center justify-between bg-white/95 px-1 py-3 text-[10px] font-semibold uppercase tracking-wide text-gray-500 backdrop-blur supports-[backdrop-filter]:bg-white/70 dark:bg-gray-900/90 dark:text-gray-400"
                  data-diff-group={item.label}
                >
                  <span>{item.label}</span>
                  <span className="font-mono text-[9px] text-gray-400 dark:text-gray-500">{item.count}</span>
                </div>
              );
            }

            if (item.kind === 'empty') {
              return (
                <div className="flex h-40 items-center justify-center rounded-lg border border-dashed border-gray-300 bg-white/60 text-sm text-gray-500 dark:border-gray-700 dark:bg-gray-900/60 dark:text-gray-400">
                  {item.message}
                </div>
              );
            }

            return (
              <DiffListItem
                key={item.entry.key}
                entry={item.entry}
                isOpen={!!openMap[item.entry.key]}
                onToggle={() => toggleOpen(item.entry.key)}
                mode={mode}
                wrap={wrap}
              />
            );
          }}
        />
      </div>
    </div>
  );
}

function DiffListItem({
  entry,
  isOpen,
  onToggle,
  mode,
  wrap,
}: {
  entry: GitDiffListEntry;
  isOpen: boolean;
  onToggle: () => void;
  mode: DiffModeEnum;
  wrap: boolean;
}) {
  const viewerRef = useRef<GitDiffViewerHandle | null>(null);
  const [expandState, setExpandState] = useState<'expanded' | 'collapsed'>('expanded');

  useEffect(() => {
    if (!isOpen) return;
    viewerRef.current?.expandAll('both');
    setExpandState('expanded');
  }, [isOpen, entry.diffText, mode, wrap]);

  const handleContextToggle = useCallback(
    (event: React.MouseEvent) => {
      event.stopPropagation();
      if (!viewerRef.current) return;
      const activeMode = mode === DiffModeEnum.Unified ? 'unified' : 'split';
      if (expandState === 'expanded') {
        viewerRef.current.collapseAll(activeMode);
        setExpandState('collapsed');
      } else {
        viewerRef.current.expandAll(activeMode);
        setExpandState('expanded');
      }
    },
    [expandState, mode],
  );

  const handleCopy = useCallback(
    (event: React.MouseEvent) => {
      event.stopPropagation();
      navigator.clipboard
        .writeText(entry.diffText)
        .then(() => {
          // no-op
        })
        .catch((err) => {
          console.error('Failed to copy diff', err);
        });
    },
    [entry.diffText],
  );

  const statusLabel = entry.statusLabel ?? entry.status ?? null;
  const statusColor = statusLabel ? STATUS_COLORS[statusLabel] ?? 'bg-gray-800 text-gray-200 border-gray-700' : null;

  return (
    <div className="mb-4 rounded-lg border border-gray-200 bg-white shadow-sm transition hover:-translate-y-[1px] hover:shadow-md dark:border-gray-700 dark:bg-gray-900">
      <button
        type="button"
        onClick={onToggle}
        className="flex w-full flex-col gap-2 px-4 py-3 text-left"
        aria-expanded={isOpen}
      >
        <div className="flex flex-wrap items-center gap-2">
          {statusLabel && (
            <span
              className={clsx(
                'inline-flex min-w-[2rem] items-center justify-center rounded-full border px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide',
                statusColor,
              )}
            >
              {formatStatusLabel(statusLabel)}
            </span>
          )}
          <span className="font-mono text-sm text-gray-800 dark:text-gray-100">{entry.title}</span>
        </div>
        <div className="flex flex-wrap items-center gap-4 text-[10px] text-gray-500 dark:text-gray-400">
          <span className="inline-flex items-center gap-1">
            <span className="font-semibold text-emerald-400">+{entry.additions}</span>
            <span className="font-semibold text-rose-400">-{entry.deletions}</span>
          </span>
          <span>{entry.groupLabel}</span>
          <span className="ml-auto inline-flex items-center text-[10px] uppercase tracking-wide text-gray-400">
            {isOpen ? 'Click to collapse' : 'Click to expand'}
          </span>
        </div>
      </button>

      {isOpen && (
        <>
          <div className="flex flex-wrap items-center justify-between gap-2 border-t border-gray-200 bg-gray-50 px-4 py-2 text-[10px] text-gray-500 dark:border-gray-700 dark:bg-gray-950/40 dark:text-gray-400">
            <span>{expandState === 'expanded' ? 'Showing full context' : 'Collapsed context'}</span>
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={handleContextToggle}
                className="rounded-md border border-gray-300 px-2 py-0.5 text-[10px] text-gray-600 transition hover:border-gray-400 hover:text-gray-800 dark:border-gray-600 dark:text-gray-200 dark:hover:border-gray-500"
              >
                {expandState === 'expanded' ? 'Collapse Context' : 'Expand Context'}
              </button>
              <button
                type="button"
                onClick={handleCopy}
                className="rounded-md border border-gray-300 px-2 py-0.5 text-[10px] text-gray-600 transition hover:border-gray-400 hover:text-gray-800 dark:border-gray-600 dark:text-gray-200 dark:hover:border-gray-500"
              >
                Copy Patch
              </button>
            </div>
          </div>
          <div className="border-t border-gray-200 bg-gray-50 px-4 py-4 dark:border-gray-700">
            <GitDiffViewer
              ref={viewerRef}
              diffText={entry.diffText}
              showHeader={false}
              mode={mode}
              wrap={wrap}
              diffFontSize={12}
              theme="light"
            />
          </div>
        </>
      )}
    </div>
  );
}
