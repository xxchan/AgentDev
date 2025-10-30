'use client';

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { postJson } from '@/lib/apiClient';
import { queryKeys } from '@/lib/queryKeys';
import type {
  DeleteWorktreeRequest,
  DeleteWorktreeResponse,
} from '@/types';

export interface DeleteWorktreeInput extends DeleteWorktreeRequest {
  worktreeId: string;
}

export function useDeleteWorktree() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async ({ worktreeId, ...body }: DeleteWorktreeInput) =>
      postJson<DeleteWorktreeResponse, DeleteWorktreeRequest>(
        `/api/worktrees/${encodeURIComponent(worktreeId)}/delete`,
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
