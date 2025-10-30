'use client';

import { useEffect, useState } from 'react';
import MainLayout from '@/components/layout/MainLayout';
import WorktreeList from '@/components/WorktreeList';
import WorktreeDetails from '@/components/WorktreeDetails';
import WorktreeProcesses from '@/components/WorktreeProcesses';
import {
  type DiscoveryParams,
  useDiscoveredWorktrees,
} from '@/hooks/useDiscoveredWorktrees';
import { useWorktrees } from '@/hooks/useWorktrees';
import { WorktreeSummary } from '@/types';

export default function WorktreesPage() {
  const { worktrees, isLoading } = useWorktrees();
  const [discoveryParams, setDiscoveryParams] =
    useState<DiscoveryParams | null>(null);
  const {
    worktrees: discoveredWorktrees,
    isLoading: isDiscoveryLoading,
    isFetching: isDiscoveryFetching,
    error: discoveryError,
    refetch: refetchDiscovery,
    hasRequested: hasDiscoveryRun,
  } = useDiscoveredWorktrees(discoveryParams);
  const [selectedWorktreeId, setSelectedWorktreeId] = useState<string | null>(
    null,
  );
  const [isProcessPanelCollapsed, setIsProcessPanelCollapsed] =
    useState(true);

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
          discoveredWorktrees={discoveredWorktrees}
          isDiscoveryLoading={
            isDiscoveryLoading || isDiscoveryFetching
          }
          discoveryError={discoveryError}
          hasDiscoveryRun={hasDiscoveryRun}
          lastDiscoveryParams={discoveryParams}
          onRunDiscovery={handleRunDiscovery}
          onRefreshDiscovery={handleRefreshDiscovery}
        />
      }
      main={
        <WorktreeDetails worktree={selectedWorktree} isLoading={isLoading} />
      }
      bottom={
        <WorktreeProcesses
          worktreeId={selectedWorktreeId}
          worktreeName={selectedWorktree?.name ?? null}
          isCollapsed={isProcessPanelCollapsed}
          onToggleCollapsed={() =>
            setIsProcessPanelCollapsed((current) => !current)
          }
        />
      }
      isBottomCollapsed={isProcessPanelCollapsed}
    />
  );
}
  const handleRunDiscovery = (params: DiscoveryParams) => {
    setDiscoveryParams(params);
  };

  const handleRefreshDiscovery = () => {
    if (discoveryParams) {
      void refetchDiscovery();
    }
  };
