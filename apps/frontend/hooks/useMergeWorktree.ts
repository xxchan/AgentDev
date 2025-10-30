'use client';

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { postJson } from '@/lib/apiClient';
import { queryKeys } from '@/lib/queryKeys';
import type { MergeWorktreeRequest, MergeWorktreeResponse } from '@/types';

export interface MergeWorktreeInput extends MergeWorktreeRequest {
  worktreeId: string;
}

export function useMergeWorktree() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async ({ worktreeId, ...body }: MergeWorktreeInput) =>
      postJson<MergeWorktreeResponse, MergeWorktreeRequest>(
        `/api/worktrees/${encodeURIComponent(worktreeId)}/merge`,
        body,
      ),
    onSuccess: (_data, variables) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.worktrees.list });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.worktrees.processes(variables.worktreeId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.worktrees.git(variables.worktreeId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.worktrees.detail(variables.worktreeId),
      });
    },
  });
}
