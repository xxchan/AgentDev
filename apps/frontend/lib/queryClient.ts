'use client';

import { QueryClient, type QueryClientConfig } from '@tanstack/react-query';

const queryClientConfig: QueryClientConfig = {
  defaultOptions: {
    queries: {
      retry: (failureCount, error) => {
        if (failureCount > 2) {
          return false;
        }
        if (error instanceof Error && 'status' in error) {
          const status = (error as { status?: number }).status;
          if (typeof status === 'number' && status >= 400 && status < 500) {
            return false;
          }
        }
        return true;
      },
      staleTime: 0,
      refetchOnWindowFocus: false,
    },
    mutations: {
      retry: 0,
    },
  },
};

export function createQueryClient() {
  return new QueryClient(queryClientConfig);
}
