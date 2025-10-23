This is a [Next.js](https://nextjs.org) project bootstrapped with [`create-next-app`](https://nextjs.org/docs/app/api-reference/cli/create-next-app).

## Getting Started

First, run the development server:

```bash
npm run dev
# or
yarn dev
# or
pnpm dev
# or
bun dev
```

Open [http://localhost:3000](http://localhost:3000) with your browser to see the result.

You can start editing the page by modifying `app/page.tsx`. The page auto-updates as you edit the file.

This project uses [`next/font`](https://nextjs.org/docs/app/building-your-application/optimizing/fonts) to automatically optimize and load [Geist](https://vercel.com/font), a new font family for Vercel.

## Data Fetching Architecture

- The UI uses [TanStack React Query](https://tanstack.com/query/latest) for all API access. The query client factory lives in `lib/queryClient.ts`, and the provider is mounted from `app/query-provider.tsx`.
- Query keys are centralised in `lib/queryKeys.ts` to keep cache boundaries consistent between hooks and components.
- Use the strongly typed helpers in `lib/apiClient.ts` (`getJson`, `postJson`) so that React Query can propagate abort signals and surface structured errors.
- Domain hooks such as `useWorktrees`, `useWorktreeProcesses`, `useSessions`, `useSessionDetails`, and `useLaunchWorktreeCommand` wrap React Query primitives with polling intervals, mutations, and cache lookups that match the dashboard UX.
- When adding new queries, prefer composing existing query keys and helpers to benefit from shared retry/stale-time defaults and predictable invalidation behaviour.

## Local Verification

After making changes to the frontend:

1. Install dependencies with `pnpm install` (from the repository root).
2. Run `pnpm --filter frontend lint` followed by `pnpm --filter frontend build` to ensure the bundle compiles cleanly.
3. Start the integrated UI (`pnpm run dev:ui` from the repository root) to exercise worktree views and session transcripts; React Query Devtools are available in development builds for cache inspection.
