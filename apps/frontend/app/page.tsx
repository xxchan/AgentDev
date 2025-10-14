'use client';

import { useEffect, useState } from 'react';
import MainLayout from '@/components/layout/MainLayout';
import WorktreeList from '@/components/WorktreeList';
import WorktreeDetails from '@/components/WorktreeDetails';
import { useWorktrees } from '@/hooks/useWorktrees';
import { WorktreeSummary } from '@/types';

export default function Home() {
  const { worktrees, isLoading } = useWorktrees();
  const [selectedWorktreeId, setSelectedWorktreeId] = useState<string | null>(
    null,
  );

  useEffect(() => {
    if (worktrees.length === 0) {
      setSelectedWorktreeId(null);
      return;
    }
    if (
      !selectedWorktreeId ||
      !worktrees.some((tree) => tree.id === selectedWorktreeId)
    ) {
      setSelectedWorktreeId(worktrees[0].id);
    }
  }, [worktrees, selectedWorktreeId]);

  const selectedWorktree: WorktreeSummary | null =
    worktrees.find((tree) => tree.id === selectedWorktreeId) ?? null;

  return (
    <MainLayout
      sidebar={
        <WorktreeList
          worktrees={worktrees}
          isLoading={isLoading}
          selectedId={selectedWorktreeId}
          onSelect={setSelectedWorktreeId}
        />
      }
      main={
        <WorktreeDetails worktree={selectedWorktree} isLoading={isLoading} />
      }
      bottom={
        <div className="flex h-full flex-col items-center justify-center gap-2 text-center text-sm text-gray-500">
          <p className="font-medium text-gray-600">Process runner coming soon</p>
          <p className="max-w-md text-xs leading-relaxed text-gray-400">
            We&apos;re wiring up <code className="rounded bg-gray-100 px-1 py-0.5">agentdev worktree exec</code>{' '}
            so you can launch and monitor commands without leaving the dashboard.
          </p>
        </div>
      }
    />
  );
}
