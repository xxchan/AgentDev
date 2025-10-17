"use client";

import { WorktreeSessionSummary } from "@/types";
import SessionListView, { SessionListItem, SessionListMessage } from "./SessionListView";

interface WorktreeSessionsProps {
  sessions: WorktreeSessionSummary[];
  formatTimestamp: (value?: string | null) => string;
}

export default function WorktreeSessions({
  sessions,
  formatTimestamp,
}: WorktreeSessionsProps) {
  const sessionItems: SessionListItem[] = sessions.map((session) => {
    const messages: SessionListMessage[] = session.user_messages.map((text, index) => ({
      key: `${session.provider}-${session.session_id}-${index}`,
      detail: {
        actor: "user",
        category: "user",
        label: "User",
        text,
        summary_text: text,
        data: null,
      },
    }));

    return {
      sessionKey: `${session.provider}-${session.session_id}`,
      provider: session.provider,
      sessionId: session.session_id,
      lastTimestamp: session.last_timestamp,
      messages,
      headerActions: (
        <button
          type="button"
          disabled
          title="Resume session coming soon"
          className="rounded-md border border-gray-200 px-2 py-1 text-xs text-gray-400"
        >
          Resume (soon)
        </button>
      ),
    } satisfies SessionListItem;
  });

  return (
    <SessionListView
      title="Sessions"
      description="Captured conversations scoped to this worktree"
      sessions={sessionItems}
      formatTimestamp={formatTimestamp}
      emptyState="No captured sessions yet for this worktree. Conversations launched via Codex or Claude will appear here automatically."
    />
  );
}
