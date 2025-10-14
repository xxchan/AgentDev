# Worktree-Centric Dashboard Design

## Motivation
- Terminal panes do not scale once more than a couple of worktrees are active; context about diffs, prompts, and running commands scatters across shells.
- Existing UI skeleton focuses on “tasks and agents” for horse-race evaluations, a workflow we rarely use today. The new goal is to streamline parallel feature development across multiple worktrees.
- Acceptance is painful: reopening every worktree, recalling the latest prompt, rerunning `pnpm dev` or other commands, and finding the right diff wastes time.

## Guiding Principles
- **Worktree first:** The dashboard should list real git worktrees that agentdev manages, not abstract task wrappers.
- **At-a-glance state:** One glance should answer “what branches exist, what changed, what is running, what was the last instruction”.
- **Bridge, don’t replace, the CLI:** Buttons should trigger the same primitives we rely on today (`agentdev worktree …`, `agentdev x exec`, `agentdev x merge`).
- **Short session summaries:** Show the intent and latest status of each AI conversation without replaying the entire log.
- **Launch pads for verification:** Quickly spin up dev/test commands, surface their logs, and know whether they are still running.

## Primary Flows
1. **Overview worktrees:** View all active worktrees with status rollups, diffs, and last activity time.
2. **Review and accept:** Drill into a worktree to inspect staged/untracked changes or recent commits, then decide to merge/delete.
3. **Recall context:** Skim prompts, user intent, and short summaries for each conversation that happened in a worktree.
4. **Run / resume commands:** Start or resume dev servers/tests in that worktree and monitor their output from the dashboard.

## Current Progress (`feat/ui`)
- Worktree list sidebar now ships with repository grouping, compact spacing, and clearer status/relative time badges for at-a-glance scanning.
- Worktree detail view protects against missing commit ids and extends relative time formatting to cover week-level granularity.
- Hooks migrated to `useWorktrees()` polling the worktree registry API; page wiring selects the first active worktree by default.
- Process pane now surfaces metadata but still lacks live log streaming while backend wiring is in progress.
- Process pane queries `/api/worktrees/:id/processes`, backed by the CLI process registry, to surface running/completed commands with status and exit codes.
- Dashboard launch dialog calls `/api/worktrees/:id/commands`, optimistically adding pending rows so new commands appear immediately.
- Backend/CLI capture stdout and stderr for each finished command, persist them in the shared registry, and return them via the processes API so the UI can show historical logs.
- Process cards render a collapsible log viewer that exposes the persisted stdout/stderr payloads per command.

## Next Focus
- Flesh out automated coverage: backend integration tests for `/api/worktrees/:id/processes` and `/commands`, plus frontend/component smoke tests around the launch dialog and log viewer.
- Decide on log delivery improvements (e.g. streaming) once persistence tests land; not immediately blocking but should stay on the roadmap.
- Revisit error-handling UX for failed launches (surface stderr snippet inline, offer retry).

## Information Architecture
- **Sidebar – Worktree List**
  - Rows: `repo-name/worktree-name`
  - Badges: git status (clean/dirty), warning for conflicts, indicator if background commands are running.
  - Secondary text: last updated timestamp, primary prompt snippet or note.
  - Filters: Active (default), Idle/Archived (collapsed section).
- **Main Panel – Tabs per Worktree**
  - **Overview:** Quick summary (prompt, manual notes, latest commit message, default agent alias). Quick actions (`Open shell`, `Run command`, `Merge`, `Delete`, `Mark archived`).
- **Diff:** Two columns (“Staged”, “Unstaged/Untracked”) with per-file list; toggle to include last N commits. Needs inline file preview.
  - **Sessions:** Cards listing conversations found under `.agentdev`, `.claude`, `.codex`, etc. Each shows created time, last user message, optional model-generated summary. Buttons for `Resume` or `Open in terminal`.
  - **Processes:** Real-time logs of commands launched from the dashboard (e.g. `pnpm dev`). Show start time, status, controls to stop/restart, and attach to tmux if necessary.

## Backend Additions
- **Worktree registry API**
  - `GET /api/worktrees`: return array of worktrees with metadata (id, repo, branch, path, prompt, alias, created_at, updated_at, git summary).
  - `GET /api/worktrees/{id}`: detailed view including staged/untracked file summary and running processes.
  - `PATCH /api/worktrees/{id}`: update notes/labels (optional stretch).
  - `POST /api/worktrees/{id}/commands`: launch a command (`agentdev x exec`, `pnpm dev`, etc.), returning command id for log streaming.
  - `GET /api/worktrees/{id}/sessions`: list session metadata discovered from known providers.
- **Git helpers**
  - ✅ `collect_worktree_diff_breakdown()` exposes staged/unstaged/untracked file diffs plus upstream divergence in a single pass.
  - Async functions to compute staged vs unstaged summaries and produce diffs without blocking.
  - Utility to fetch “last N commits” metadata for the worktree.
  - Surface actionable diagnostics when git commands fail (full command, exit code, stderr) and skip inspection gracefully when `.git` metadata is missing to avoid log spam.
- **Session discovery**
  - Scan for session logs in `.agentdev/`, `.claude/`, `.codex/` directories inside the worktree.
  - Extract prompt/user message and optionally call summarizer for longer transcripts (future optimization).
- **Session provider abstraction**
  - Introduce a `SessionProvider` trait/enums in the backend that encapsulate discovery, summary extraction, and resume mechanics per agent family (Codex, Claude, custom).
  - Providers register themselves with metadata (storage path patterns, transcript format, resume command) so the UI can render heterogeneous sessions uniformly.
  - Allow worktree metadata to record which provider produced a session, enabling provider-specific actions (e.g., resume via REST call vs. launching CLI).
- **Sessions CLI groundwork**
  - Add an `agentdev sessions` command group mirroring `agentdev worktree`, starting with `agentdev sessions list [--worktree <name>]` to expose provider, summary, timestamps, and associated worktree.
  - Implement the Codex provider first by inspecting `~/.codex/**` session manifests; ensure the abstraction cleanly maps to its storage layout.
  - Prototype a Claude Code provider (likely reading `~/.claude/**` or IDE-specific stores) to validate the provider trait spans multiple ecosystems before expanding UI support.
- **Command runtime abstraction**
  - Replace direct tmux coupling with a process registry that can handle `spawn`, `stream logs`, `stop`.
  - Keep tmux integration available for legacy flows but do not require it for UI-initiated commands.
- **State persistence**
  - Extend persisted `WorktreeInfo` metadata to include git status snapshot, last prompt, last command summary, and cached session details.
  - Rehydrate this data from `state.json` on startup and reconcile against real git worktrees.

## Frontend Changes
- ✅ Remove legacy task tree; only `useWorktrees()` remains as the source of truth for dashboard data.
- Rework layout:
  - ✅ `WorktreeList` groups worktrees by repository with repo headers, status badges, and refreshed typography for density (task tree deleted).
  - ✅ `DiffPanel` surfaces staged, unstaged, untracked files with inline unified diffs and default expansion of the first change.
  - `SessionList` reading metadata and lazy-loading details.
  - `ProcessPane` showing streaming command output (WebSocket).
- Add “Launch command” dialog allowing template commands (e.g., `pnpm dev`, `pnpm test`, custom).
- Integrate optimistic updates for quick actions (`archive`, `delete`, `merge`).

## Migration Plan
1. **Backend schema:** Standardize on worktree-focused structs/endpoints; legacy `/api/tasks` routes are removed. ✅
2. **Frontend toggle:** Add experimental flag (`NEXT_PUBLIC_ENABLE_WORKTREE_DASHBOARD`) to develop the new layout alongside the old one. ✅ (flag removed after cut-over)
3. **Cut-over:** Remove task-centric components/endpoints, update docs, and clean unused code. ✅ Worktree UI is now the only supported flow.
4. **Follow-ups:** Optional settings page for default commands, integration with summarizer service, search/filter for large worktree lists.

## Delivery Milestones (E2E Features)
1. **Worktree visibility (CLI + Dashboard)**
   - Implement `agentdev worktree list --json` enhancements to emit git status, last activity, and prompt metadata.
   - ✅ Expose `GET /api/worktrees`, `GET /api/worktrees/{id}`, and `GET /api/worktrees/{id}/git`; make the worktree-first dashboard the default homepage (feature flag removed).
   - ✅ Acceptance: a freshly created worktree appears in both CLI and dashboard with accurate git summaries and per-file diffs.
2. **Session surfacing**
   - ✅ Ship `agentdev sessions list` (Codex provider first); dashboard currently consumes summaries via `/api/worktrees`.
   - 🔄 Sessions tab scaffolded with provider badges, last message, and resume placeholder. Still need detailed transcript fetching and resume wiring.
   - Acceptance: running an agent via Codex updates both CLI and UI session listings without manual refresh.
3. **Ad-hoc command runner**
   - Land process registry + `agentdev x exec` integration, plus API (`POST /api/worktrees/{id}/commands`, log streaming).
   - Add dashboard Processes tab with start/stop controls and log viewer.
   - Acceptance: launching `pnpm dev` from the dashboard shows live logs and status, and the CLI reports the process in `agentdev worktree status`.
4. **Conversation control**
   - Extend session providers with resume hooks (CLI: `agentdev sessions resume <id>`; API endpoint).
   - Introduce UI affordance to resume or start new sessions, streaming output via WebSocket (tmux optional).
   - Acceptance: resuming a Codex/Claude session from the dashboard opens an interactive stream, and CLI resume provides the same behavior.

## Open Questions
- How do we store human-authored notes per worktree (commit message, manual summary)? Possibly extend `state.json` with free-form text.
- Session summary source: rely on existing first user message, or introduce background summarization job?
- Should `agentdev x exec` output continue through tmux under the hood for resilience, or can we rely on plain processes per command?
- What signals mark a worktree as “idle/archived”? (e.g., merged into main, no commits in 48h, or manual toggle).

## Verification Strategy
- Backend unit tests for git status/diff helpers and session discovery.
- Integration test to ensure persisted `WorktreeInfo` rehydrates correctly and mismatched entries are pruned.
- Frontend `pnpm run build:frontend` and playwright/cypress smoke test exercising worktree list and diff tabs.
- Manual flow: spawn two worktrees via CLI, run dashboard, confirm UI shows accurate diffs, sessions, and command logs.
