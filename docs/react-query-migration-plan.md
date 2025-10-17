# React Query Migration Plan

This document captures the end-to-end plan for migrating the AgentDev frontend to [TanStack React Query](https://tanstack.com/query/latest) while maintaining current functionality and type-safety guarantees.

## 1. Inventory & Behaviour Requirements

- **Locate every `fetch` usage** and capture current behaviour:
  - `hooks/useWorktrees.ts`, `hooks/useWorktreeProcesses.ts`, `hooks/useSessions.ts`.
  - `components/WorktreeGitSection.tsx`, `components/WorktreeDetails.tsx`, `components/WorktreeProcesses.tsx`.
  - `app/sessions/page.tsx` (list polling + transcript detail fetch + manual caches/AbortControllers).
- **Preserve existing UX**: polling intervals, manual refetch buttons, error messaging, abort semantics, VSCode command feedback, and transcript mode switching.
- **Note domain-specific invariants**: worktree-derived selection logic, command trigger effects on process lists, and transcript truncation handling.

## 2. React Query Infrastructure

- Add dependencies: `@tanstack/react-query` globally, `@tanstack/react-query-devtools` for development-only use.
- Create `lib/queryClient.ts` exporting a factory/helper to instantiate `QueryClient` with defaults (retry strategy, stale times, refetch on window focus).
- Add `app/query-provider.tsx` to wrap children with `QueryClientProvider`; integrate in `app/layout.tsx`. Ensure compatibility with Next.js App Router (client component wrapper).
- Mount Devtools conditionally in development builds.

## 3. Query Keys & HTTP Utilities

- Introduce `lib/queryKeys.ts` defining typed key helpers, e.g.:
  - `worktrees.list`, `worktrees.detail(id)`, `worktrees.processes(id)`, `worktrees.git(id)`.
  - `sessions.list`, `sessions.detail(provider, id, mode)`.
- Create `lib/apiClient.ts` with strongly typed helpers (`getJson<T>`, `postJson<T>`) that:
  - Accept `AbortSignal` from React Query.
  - Use existing `apiUrl` builder.
  - Throw enriched errors (status, message) for consistent UI handling.

## 4. Convert Data Hooks (Polling & Lists)

- Rewrite `useWorktrees`, `useWorktreeProcesses`, `useSessions` to `useQuery` wrappers:
  - Preserve polling via `refetchInterval` (currently 5s).
  - Gate `useWorktreeProcesses` with `enabled: Boolean(worktreeId)`.
  - Return typed data structures (`WorktreeSummary[]`, etc.) and query metadata (loading, error, refetch).
  - Remove custom interval + abort logic; defer to React Query.

## 5. Lazy Panels & Detail Views

- Update `WorktreeGitSection` to use `useQuery`:
  - `enabled` when expanded and `worktreeId` exists.
  - Keep cache per `worktreeId`; reset expansion state as needed.
  - Use query status for loading/error UI.
- Adjust `WorktreeDetails` to consume new hooks and manage selection state using query data (`data ?? []`).
- Ensure worktree selection logic in `app/worktrees/page.tsx` still handles empty/loading states.

## 6. Mutations for Commands

- Implement `useLaunchWorktreeCommand` (React Query `useMutation`) shared by `WorktreeDetails` and `WorktreeProcesses`.
  - POST to `/api/worktrees/{id}/commands`.
  - On success, invalidate `worktrees.processes`, maybe `worktrees.list` for status updates.
  - Surface loading/error states to drive existing UI feedback.

## 7. Worktree Processes

- Consume new `useWorktreeProcesses` query within `WorktreeProcesses` component.
- Replace manual `refetch` + `setState` combos with React Query helpers (`refetch`, `invalidateQueries`).
- Keep UI state (log expansion) only where necessary.

## 8. Sessions Console Migration

- Swap `useSessions` usage to React Query data; update derived memoized structures to read from `query.data ?? []`.
- Replace manual transcript fetching with React Query:
  - Query key `queryKeys.sessions.detail(provider, sessionId, mode)`.
  - `enabled` when a session is selected and mode requires fetching (`user_only` only when preview truncated).
  - Remove `detailCache`, `detailErrors`, `detailLoadingKey`, and AbortControllers; rely on query cache and state.
  - Handle mode switching with `queryClient.invalidateQueries` or by selecting from cache.
- Ensure UI handles loading/error states via query flags (e.g., spinner when transcript is loading).

## 9. Clean-Up & Consistency

- Remove redundant utilities (manual polling intervals, abort refs).
- Ensure TypeScript remains strict (no `any`/`unknown`).
- Document new patterns inline with concise comments where necessary.

## 10. Verification Checklist

1. `pnpm install` to update lockfile with new deps.
2. `pnpm lint` and `pnpm run build:frontend`.
3. Start integrated dev environment (`pnpm run dev:ui` → tmux session):
   - Validate sessions dashboard (provider filters, group switching, transcript loading/cancellation).
   - Validate worktrees view (polling, Git section expansion, process list auto-refresh).
   - Trigger VSCode launch to confirm mutation invalidates process list and shows feedback.
4. Observe logs for errors; inspect React Query Devtools (dev mode).
5. Shut down the tmux session when finished (`tmux kill-session -t agentdev_dev`).

## 11. Documentation & Follow-Up

- Update README or relevant internal docs to mention React Query architecture and helper modules.
- Capture lessons learned or edge cases for future contributors.
- Plan incremental rollout (feature flag or branch) if we want to merge in phases.

This plan should keep the migration safe, incremental, and fully validated before delivery.
