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
  MouseEvent,
} from 'react';
import clsx from 'clsx';
import { Virtuoso, VirtuosoHandle } from 'react-virtuoso';

import { Diff } from '@/components/ui/diff';
import type { File } from '@/components/ui/diff/utils';

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
  files: File[];
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

const FILE_TYPE_LABELS: Record<string, string> = {
  add: 'Added',
  delete: 'Deleted',
  modify: 'Modified',
  rename: 'Renamed',
  copy: 'Copied',
};

const FILE_TYPE_COLORS: Record<string, string> = {
  add: 'bg-emerald-500/10 text-emerald-600 border-emerald-300/70',
  delete: 'bg-rose-500/10 text-rose-600 border-rose-300/70',
  modify: 'bg-blue-500/10 text-blue-600 border-blue-300/70',
  rename: 'bg-purple-500/10 text-purple-600 border-purple-300/70',
  copy: 'bg-sky-500/10 text-sky-600 border-sky-300/70',
};

function formatStatusLabel(label?: string | null) {
  if (!label) return null;
  if (label.length <= 12) return label;
  return label.slice(0, 12);
}

const VirtuosoScroller = forwardRef<HTMLDivElement, ComponentPropsWithoutRef<'div'>>(function VirtuosoScroller(
  { style, ...props },
  ref,
) {
  return (
    <div
      {...props}
      ref={ref}
      style={{ ...(style ?? {}), overflowY: 'visible' }}
    />
  );
});
const VirtuosoFooter = () => <div className="h-8" />;

export default function GitDiffList({
  entries,
  emptyMessage = 'No diff output available.',
  scrollContainerRef,
}: GitDiffListProps) {
  const [query, setQuery] = useState('');
  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const [customScrollParent, setCustomScrollParent] = useState<HTMLElement | null>(null);
  const [openMap, setOpenMap] = useState<Record<string, boolean>>({});

  const filteredEntries = useMemo(() => {
    if (!query.trim()) return entries;
    const term = query.trim().toLowerCase();
    return entries.filter((entry) => {
      const fileNames = entry.files
        .map((file) => `${file.newPath ?? ''} ${file.oldPath ?? ''}`)
        .join(' ');
      const haystack = `${entry.title} ${entry.groupLabel} ${entry.status ?? ''} ${entry.statusLabel ?? ''} ${fileNames}`.toLowerCase();
      return haystack.includes(term);
    });
  }, [entries, query]);

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
    if (!scrollContainerRef) {
      setCustomScrollParent(null);
      return;
    }
    setCustomScrollParent(scrollContainerRef.current ?? null);
  }, [scrollContainerRef]);

  const totalVisibleFiles = useMemo(
    () =>
      filteredEntries.reduce(
        (sum, entry) => sum + Math.max(entry.files.length, entry.diffText.trim() ? 1 : 0),
        0,
      ),
    [filteredEntries],
  );

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
        fileCount: number;
      }
    >();
    let order = 0;

    filteredEntries.forEach((entry) => {
      if (!groupMap.has(entry.groupKey)) {
        groupMap.set(entry.groupKey, {
          label: entry.groupLabel,
          items: [],
          order: order++,
          fileCount: 0,
        });
      }
      const group = groupMap.get(entry.groupKey)!;
      group.items.push(entry);
      const weight = entry.files.length > 0 ? entry.files.length : entry.diffText.trim() ? 1 : 0;
      group.fileCount += weight;
    });

    return [...groupMap.values()]
      .sort((a, b) => a.order - b.order)
      .flatMap<DiffListRow>((group) => [
        {
          kind: 'group',
          key: `group:${group.label}`,
          label: group.label,
          count: group.fileCount,
        },
        ...group.items.map<DiffListRow>((entry) => ({ kind: 'diff', key: entry.key, entry })),
      ]);
  }, [filteredEntries, emptyMessage, query]);

  const toggleOpen = useCallback((key: string) => {
    setOpenMap((prev) => ({ ...prev, [key]: !prev[key] }));
  }, []);

  const openCount = useMemo(() => Object.values(openMap).filter(Boolean).length, [openMap]);

  return (
    <div className="relative">
      <div
        className="sticky top-[72px] z-20 flex flex-wrap items-center gap-3 rounded-lg border border-gray-200 bg-white/90 px-4 py-3 shadow-sm backdrop-blur supports-[backdrop-filter]:bg-white/70 dark:border-gray-700 dark:bg-gray-900/80"
      >
        <div className="flex flex-1 flex-wrap items-center gap-3">
          <h4 className="text-sm font-semibold uppercase tracking-wide text-gray-800 dark:text-gray-100">
            Diff Files
          </h4>
          <span className="rounded-full border border-blue-500/30 bg-blue-500/10 px-2 py-0.5 text-[10px] font-medium text-blue-500">
            {totalVisibleFiles}
          </span>
          <span className="hidden items-center gap-1 rounded-full border border-emerald-500/30 bg-emerald-500/10 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide text-emerald-400 sm:inline-flex">
            Virtual Scroll
          </span>
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

      <div className={clsx('mt-4', !customScrollParent && 'h-[72vh]')} data-virt-scroll>
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
}: {
  entry: GitDiffListEntry;
  isOpen: boolean;
  onToggle: () => void;
}) {
  const [showContext, setShowContext] = useState(true);

  useEffect(() => {
    if (!isOpen) return;
    setShowContext(true);
  }, [isOpen, entry.diffText, entry.key]);

  const handleContextToggle = useCallback(
    (event: MouseEvent<HTMLButtonElement>) => {
      event.stopPropagation();
      setShowContext((prev) => !prev);
    },
    [],
  );

  const handleCopy = useCallback(
    (event: MouseEvent<HTMLButtonElement>) => {
      event.stopPropagation();
      navigator.clipboard
        .writeText(entry.diffText)
        .catch((error) => console.error('Failed to copy diff', error));
    },
    [entry.diffText],
  );

  const statusLabel = entry.statusLabel ?? entry.status ?? null;
  const statusColor = statusLabel ? STATUS_COLORS[statusLabel] ?? 'bg-gray-800 text-gray-200 border-gray-700' : null;
  const fileCount = entry.files.length > 0 ? entry.files.length : entry.diffText.trim() ? 1 : 0;

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
          <span className="rounded-full border border-gray-200 px-2 py-0.5 text-[9px] uppercase tracking-wide text-gray-400 dark:border-gray-600">
            {fileCount} {fileCount === 1 ? 'file' : 'files'}
          </span>
          <span className="ml-auto inline-flex items-center text-[10px] uppercase tracking-wide text-gray-400">
            {isOpen ? 'Click to collapse' : 'Click to expand'}
          </span>
        </div>
      </button>

      {isOpen && (
        <>
          <div className="flex flex-wrap items-center justify-between gap-2 border-t border-gray-200 bg-gray-50 px-4 py-2 text-[10px] text-gray-500 dark:border-gray-700 dark:bg-gray-950/40 dark:text-gray-400">
            <span>{showContext ? 'Showing full context' : 'Context collapsed'}</span>
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={handleContextToggle}
                className="rounded-md border border-gray-300 px-2 py-0.5 text-[10px] text-gray-600 transition hover:border-gray-400 hover:text-gray-800 dark:border-gray-600 dark:text-gray-200 dark:hover:border-gray-500"
              >
                {showContext ? 'Collapse Context' : 'Expand Context'}
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
            {entry.files.length > 0 ? (
              <div className="space-y-4">
                {entry.files.map((file, index) => (
                  <FileDiffViewer
                    key={`${entry.key}:file:${file.newPath ?? file.oldPath ?? index}`}
                    file={file}
                    collapseContext={!showContext}
                    index={index}
                  />
                ))}
              </div>
            ) : entry.diffText.trim() ? (
              <pre className="overflow-x-auto rounded-md border border-gray-200 bg-white p-3 text-xs text-gray-700 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200">
                {entry.diffText}
              </pre>
            ) : (
              <p className="text-xs text-gray-500 dark:text-gray-400">No diff content available.</p>
            )}
          </div>
        </>
      )}
    </div>
  );
}

function FileDiffViewer({
  file,
  collapseContext,
  index,
}: {
  file: File;
  collapseContext: boolean;
  index: number;
}) {
  const title = file.newPath ?? file.oldPath ?? `File ${index + 1}`;
  const typeLabel = FILE_TYPE_LABELS[file.type] ?? 'Changed';
  const typeColor = FILE_TYPE_COLORS[file.type] ?? 'bg-gray-100 text-gray-600 border-gray-200';
  const renamed = file.oldPath && file.newPath && file.oldPath !== file.newPath;

  return (
    <div className="rounded-md border border-gray-200 bg-white shadow-sm dark:border-gray-700 dark:bg-gray-900">
      <div className="flex flex-wrap items-center gap-2 border-b border-gray-200 bg-gray-100 px-3 py-2 text-[11px] font-medium text-gray-600 dark:border-gray-700 dark:bg-gray-950">
        <span
          className={clsx(
            'inline-flex items-center rounded-full border px-2 py-0.5 text-[10px] uppercase tracking-wide',
            typeColor,
          )}
        >
          {typeLabel}
        </span>
        <span className="font-mono text-xs text-gray-800 dark:text-gray-100">{title}</span>
        {renamed && (
          <span className="text-[10px] text-gray-400">{file.oldPath} → {file.newPath}</span>
        )}
      </div>
      <div className="overflow-x-auto">
        <Diff
          hunks={file.hunks}
          type={file.type}
          fileName={title}
          collapseContext={collapseContext}
          className="min-w-full"
        />
      </div>
    </div>
  );
}
