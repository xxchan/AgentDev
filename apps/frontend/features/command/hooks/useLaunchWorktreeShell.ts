'use client';

import { useMutation } from '@tanstack/react-query';
import { postJson } from '@/lib/apiClient';
import type { LaunchWorktreeShellResponse } from '@/types';

interface LaunchWorktreeShellRequest {
  command?: string;
}

interface LaunchDirectoryShellRequest extends LaunchWorktreeShellRequest {
  path: string;
}

export type LaunchWorktreeShellInput =
  | ({ type: 'worktree'; worktreeId: string } & LaunchWorktreeShellRequest)
  | ({ type: 'directory'; workingDir: string } & LaunchWorktreeShellRequest);

export function useLaunchWorktreeShell() {
  return useMutation({
    mutationFn: async (input: LaunchWorktreeShellInput) => {
      if (input.type === 'worktree') {
        const payload: LaunchWorktreeShellRequest = input.command ? { command: input.command } : {};
        return postJson<LaunchWorktreeShellResponse, LaunchWorktreeShellRequest>(
          `/api/worktrees/${encodeURIComponent(input.worktreeId)}/shell`,
          payload,
        );
      }

      const payload: LaunchDirectoryShellRequest = {
        path: input.workingDir,
        ...(input.command ? { command: input.command } : {}),
      };
      return postJson<LaunchWorktreeShellResponse, LaunchDirectoryShellRequest>(
        '/api/shell',
        payload,
      );
    },
  });
}
