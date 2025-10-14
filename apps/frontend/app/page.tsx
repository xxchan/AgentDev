'use client';

import { useEffect, useState } from 'react';
import MainLayout from '@/components/layout/MainLayout';
import WorktreeList from '@/components/WorktreeList';
import WorktreeDetails from '@/components/WorktreeDetails';
import WorktreeProcesses from '@/components/WorktreeProcesses';
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
        <WorktreeProcesses
          worktreeId={selectedWorktreeId}
          worktreeName={selectedWorktree?.name ?? null}
        />
      }
    />
  );
}
