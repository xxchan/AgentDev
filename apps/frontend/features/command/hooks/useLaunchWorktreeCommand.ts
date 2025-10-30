'use client';

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { postJson } from '@/lib/apiClient';
import { queryKeys } from '@/lib/queryKeys';
import type { LaunchWorktreeCommandResponse } from '@/types';

interface LaunchWorktreeCommandRequest {
  command: string;
  description?: string;
}

export interface LaunchWorktreeCommandInput extends LaunchWorktreeCommandRequest {
  worktreeId: string;
}

export function useLaunchWorktreeCommand() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async ({ worktreeId, command, description }: LaunchWorktreeCommandInput) =>
      postJson<LaunchWorktreeCommandResponse, LaunchWorktreeCommandRequest>(
        `/api/worktrees/${encodeURIComponent(worktreeId)}/commands`,
        {
          command,
          description,
        },
      ),
    onSuccess: (_response, variables) => {
      void queryClient.invalidateQueries({
        queryKey: queryKeys.worktrees.processes(variables.worktreeId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.worktrees.list,
      });
    },
  });
}
