'use client';

import { useMutation } from '@tanstack/react-query';
import { postJson } from '@/lib/apiClient';
import type { LaunchWorktreeShellResponse } from '@/types';

interface LaunchWorktreeShellRequest {
  command?: string;
}

export interface LaunchWorktreeShellInput extends LaunchWorktreeShellRequest {
  worktreeId: string;
}

export function useLaunchWorktreeShell() {
  return useMutation({
    mutationFn: async ({ worktreeId, command }: LaunchWorktreeShellInput) => {
      const payload: LaunchWorktreeShellRequest = command
        ? { command }
        : {};
      return postJson<LaunchWorktreeShellResponse, LaunchWorktreeShellRequest>(
        `/api/worktrees/${encodeURIComponent(worktreeId)}/shell`,
        payload,
      );
    },
  });
}

