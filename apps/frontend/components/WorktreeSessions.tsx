"use client";

import { useEffect } from "react";
import SessionDetailModeToggle from "@/components/SessionDetailModeToggle";
import SessionListView, { SessionListItem, SessionListMessage } from "@/components/SessionListView";
import { useSessionDetailMode } from "@/hooks/useSessionDetailMode";
import { useSessionDetails } from "@/hooks/useSessionDetails";
import {
  buildUserOnlyMessages,
  getSessionKey,
  toSessionListMessages,
} from "@/lib/session-utils";
import { WorktreeSessionSummary } from "@/types";

interface WorktreeSessionsProps {
  sessions: WorktreeSessionSummary[];
  formatTimestamp: (value?: string | null) => string;
}

export default function WorktreeSessions({
  sessions,
  formatTimestamp,
}: WorktreeSessionsProps) {
  const [detailMode, setDetailMode] = useSessionDetailMode();
  const { getDetail, getError, requestDetail, isFetching } = useSessionDetails();

  useEffect(() => {
    if (sessions.length === 0 || detailMode === "user_only") {
      return;
    }

    sessions.forEach((session) => {
      const targetMode = detailMode;
      const args = {
        provider: session.provider,
        sessionId: session.session_id,
        mode: targetMode,
      } as const;

      if (getDetail(args) || isFetching(args)) {
        return;
      }

      void requestDetail({
        provider: session.provider,
        sessionId: session.session_id,
        mode: targetMode,
      });
    });
  }, [detailMode, getDetail, isFetching, requestDetail, sessions]);

  const sessionItems: SessionListItem[] = sessions.map((session) => {
    const sessionKey = getSessionKey(session);
    const baseArgs = {
      provider: session.provider,
      sessionId: session.session_id,
    } as const;

    const fullArgs = { ...baseArgs, mode: "full" } as const;
    const conversationArgs = { ...baseArgs, mode: "conversation" } as const;
    const userOnlyArgs = { ...baseArgs, mode: "user_only" } as const;

    const fullDetail = getDetail(fullArgs);
    const conversationDetail = getDetail(conversationArgs);
    const userOnlyDetail = getDetail(userOnlyArgs);

    const fullError = getError(fullArgs);
    const conversationError = getError(conversationArgs);
    const userOnlyError = getError(userOnlyArgs) ?? fullError;

    const fullFetching = isFetching(fullArgs);
    const conversationFetching = isFetching(conversationArgs);
    const userOnlyFetching = isFetching(userOnlyArgs);

    const previewTruncated =
      session.user_message_count > session.user_messages_preview.length;

    const activeDetail =
      detailMode === "full"
        ? fullDetail
        : detailMode === "conversation"
          ? conversationDetail
          : userOnlyDetail ?? fullDetail ?? null;

    let messages: SessionListMessage[] =
      detailMode === "full"
        ? toSessionListMessages(activeDetail?.events ?? [], sessionKey, "full")
        : detailMode === "conversation"
          ? toSessionListMessages(
              activeDetail?.events ?? [],
              sessionKey,
              "conversation",
            )
          : buildUserOnlyMessages(session, activeDetail ?? undefined);

    const needsFetch =
      detailMode === "full"
        ? !fullDetail
        : detailMode === "conversation"
          ? !conversationDetail
          : false;

    const detailError =
      detailMode === "full"
        ? fullError
        : detailMode === "conversation"
          ? conversationError
          : userOnlyError;

    const detailLoading =
      detailMode === "full"
        ? fullFetching && !fullDetail
        : detailMode === "conversation"
          ? conversationFetching && !conversationDetail
          : previewTruncated && !fullDetail
            ? userOnlyFetching
            : false;

    if (detailMode === "user_only") {
      const shownUserMessages = messages.filter(
        (entry) => (entry.detail.actor ?? "").toLowerCase() === "user",
      ).length;
      const isTruncated =
        previewTruncated && shownUserMessages < session.user_message_count;

      if (isTruncated) {
        messages = [
          ...messages,
          {
            key: `${sessionKey}-preview-note`,
            detail: {
              actor: "system",
              category: "session_meta",
              label: "Preview",
              text: `Showing ${shownUserMessages} of ${session.user_message_count} user messages.`,
              summary_text: "Showing limited user messages",
              data: null,
            },
          },
        ];
      }
    }

    const item: SessionListItem = {
      sessionKey,
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
    };

    if (detailMode !== "user_only") {
      if (detailError) {
        item.emptyState = (
          <div className="text-xs text-destructive">
            Failed to load transcript: {detailError}
          </div>
        );
      } else if (needsFetch || detailLoading) {
        item.emptyState = (
          <div className="flex items-center justify-center gap-2 text-xs text-muted-foreground">
            <span className="inline-block h-3 w-3 animate-spin rounded-full border-2 border-border border-t-primary" />
            Loading transcript…
          </div>
        );
      } else if (messages.length === 0) {
        item.emptyState =
          detailMode === "conversation"
            ? "No conversation messages found."
            : "No transcript entries found.";
      }
    }

    return item;
  });

  return (
    <SessionListView
      title="Sessions"
      description="Captured conversations scoped to this worktree"
      sessions={sessionItems}
      formatTimestamp={formatTimestamp}
      emptyState="No captured sessions yet for this worktree. Conversations launched via Codex or Claude will appear here automatically."
      toolbar={<SessionDetailModeToggle value={detailMode} onChange={setDetailMode} />}
    />
  );
}
