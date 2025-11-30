# Diff Viewer Rebuild Plan

## Objectives
- Replace the legacy `@git-diff-view/react`-based viewer with the shadcn/Fredrika diff stack for improved readability (merged modified rows, inline char edits, better syntax highlighting).
- Preserve higher-level UX (grouped diff entries, virtualization, session/worktree integrations) while simplifying secondary controls (no split view/wrap toggles unless we rebuild them explicitly).
- Ensure diff data remains sourced from git outputs (commit divergence, staged/unstaged/untracked) without regressions in grouping, status metadata, or counts.

## Success Criteria
1. All diff displays (worktree “Diff Breakdown”, commit diff sections, any standalone viewers) render via the new `parseDiff` + `<Diff>/<Hunk>` components.
2. Removing `@git-diff-view/react`, `apps/frontend/components/GitDiffViewer.tsx`, and `apps/frontend/lib/diffPreferences.ts` introduces no TypeScript errors or runtime crashes.
3. Large diff lists remain responsive thanks to `react-virtuoso`; rendering a single entry with large hunks keeps scroll smooth (<100 ms frame budget when virtualized).
4. UX parity or improvement: users can still copy patches, collapse/expand context, and identify file status/groups at a glance.
5. QA: `pnpm run lint`, targeted UI smoke via `pnpm run dev:ui` + manual diff inspection, and snapshot/regression checks for helper utilities where applicable.

## Constraints & Considerations
- **Unified-only output:** the new component does not support split mode; plan assumes we intentionally drop split view & wrap toggles. Document this in release notes.
- **Bundle size:** `refractor/all` imports all prism languages; consider lazy-loading or trimming languages in a follow-up if bundle impact is high.
- **Large patches:** parser currently takes a single diff string; we must ensure multi-file patches (commit diff) render every file—not just the first element of `parseDiff`.
- **Accessibility:** `<table>` layout already provides semantics, but we should double-check focus order and color contrast for inserted/deleted lines.
- **Feature flags:** since rewrite is wholesale, no incremental flag unless requested; ensure staging verification before merging.

## Deliverables
- Updated UI components under `apps/frontend/components/ui/diff` wired into the dashboard.
- Refactored `GitDiffList` + `WorktreeGitSection` using the new renderer and removing obsolete state/prefs.
- Documentation of user-facing changes (loss of split view, new inline highlighting) in repo notes or release summary.
- Tests or stories covering core parsing helpers (e.g., unit tests for `mergeModifiedLines` edge cases, snapshot for multi-file diffs).

## Implementation Phases

### Phase 1 – Cleanup & Dependency Pruning
1. Remove `@git-diff-view/react` from `apps/frontend/package.json` and delete `components/GitDiffViewer.tsx` plus `lib/diffPreferences.ts`.
2. Search for `GitDiffViewer` usage (primarily `GitDiffList`, any other custom pages) and note integration points.
3. Run `pnpm install` (frontend) to sync lockfile and ensure no residual deps.
4. Baseline lint/tests to confirm repo clean prior to deeper work.

### Phase 2 – Data Layer Refactor
1. Extend `WorktreeGitSection`’s diff aggregation so each entry holds parsed metadata:
   - On creation, call `parseDiff` per diff text.
   - Store resulting `File[]` (with `hunks`, `type`, `oldPath/newPath`) alongside additions/deletions and status info.
2. Update any helper calculations (e.g., `computeDiffStats`, label extraction) to use parsed output when available, falling back to raw text only for metrics not exposed by the parser.
3. Ensure commit divergence diffs (multi-file patch strings) split cleanly—possibly by persisting raw text + parsed files per entry to avoid re-parsing on every render (memoize via `useMemo` in `GitDiffList`).

### Phase 3 – UI Integration
1. Rebuild the card for each diff entry:
   - Replace `<GitDiffViewer>` with the new `<Diff>` component, iterating over each parsed file and its hunks.
   - Use `CollapsibleCard`/`CopyButton` to recreate header actions (status badge, additions/deletions, copy patch, collapse context).
   - For multi-file entries, display a nested accordion or sequential sections with filenames and file-type badges.
2. Replace expand/collapse context buttons with logic tied to skip blocks:
   - Option A: adjust `ParseOptions.maxDiffDistance` to control how much context is shown.
   - Option B: keep a simple “Show/hide full context” toggle that re-runs `parseDiff` with different options per entry.
3. Keep `react-virtuoso` scaffolding but drop mode/wrap state. Instead, provide quick filters/search + counts as today.
4. Ensure styling matches existing typography (Tailwind classes) and respects dark mode if applicable (audit colors in `theme.css`).

### Phase 4 – Verification & Polish
1. Manual QA: run `pnpm run dev:ui`, open a worktree with representative diffs (added, deleted, renamed, large modifications) and capture screenshots for review.
2. Automated checks:
   - `pnpm run lint` (frontend) and any relevant tests. Consider adding unit tests for `parseDiff` edge cases (e.g., `mergeModifiedLines` distances) under `apps/frontend/components/ui/diff/utils/__tests__`.
   - Optional: write Storybook-like fixtures (if available) or snapshot tests using sample patches to guard against regressions.
3. Performance sanity: inspect React DevTools Profiler or use `performance.mark` logging while scrolling large diff lists to ensure virtualization still works.
4. Documentation: update README/impl notes (e.g., `impl-ui.md`) explaining the new diff pipeline and any removed features (split view, wrap toggle).

## Open Questions / Follow-ups
- Should we expose parser options (e.g., toggle for inline char edits or context distance) in the UI later?
- Do we need a dark theme variant for syntax highlighting? If so, extend `theme.css` with CSS variables bound to Tailwind theme colors.
- If bundle size is a concern, evaluate tree-shaking `refractor` languages or replacing with a lighter highlighter.

## Timeline Estimate
1. **Phase 1:** 0.5 day (cleanup + dependency removal).
2. **Phase 2:** 1 day (data refactor & memoization for parsed outputs).
3. **Phase 3:** 1–1.5 days (UI rebuild, multi-file handling, controls).
4. **Phase 4:** 0.5 day (QA, docs, polish). 

Total: ~3 days concentrated effort (adjust based on review cycles and QA depth).
