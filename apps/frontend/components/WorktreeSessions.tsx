"use client";

import { useEffect, useMemo } from "react";
import SessionDetailModeToggle from "@/components/SessionDetailModeToggle";
import SessionListView, { SessionListItem, SessionListMessage } from "@/components/SessionListView";
import { useSessionDetailMode } from "@/hooks/useSessionDetailMode";
import { useSessionDetails } from "@/hooks/useSessionDetails";
import {
  buildDetailCacheKey,
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
  const { detailCache, detailErrors, requestDetail, isLoading: isDetailLoading } =
    useSessionDetails();

  useEffect(() => {
    if (sessions.length === 0) {
      return;
    }

    sessions.forEach((session) => {
      const sessionKey = getSessionKey(session);

      if (detailMode === "full") {
        const key = buildDetailCacheKey(sessionKey, "full");
        if (!detailCache[key]) {
          void requestDetail({
            provider: session.provider,
            sessionId: session.session_id,
            mode: "full",
          });
        }
        return;
      }

      if (detailMode === "conversation") {
        const key = buildDetailCacheKey(sessionKey, "conversation");
        if (!detailCache[key]) {
          void requestDetail({
            provider: session.provider,
            sessionId: session.session_id,
            mode: "conversation",
          });
        }
        return;
      }

      const previewTruncated =
        session.user_message_count > session.user_messages_preview.length;
      if (!previewTruncated) {
        return;
      }

      const userOnlyKey = buildDetailCacheKey(sessionKey, "user_only");
      const fullKey = buildDetailCacheKey(sessionKey, "full");
      if (!detailCache[userOnlyKey] && !detailCache[fullKey]) {
        void requestDetail({
          provider: session.provider,
          sessionId: session.session_id,
          mode: "user_only",
        });
      }
    });
  }, [sessions, detailMode, detailCache, requestDetail]);

  const sessionItems = useMemo<SessionListItem[]>(() => {
    return sessions.map((session) => {
      const sessionKey = getSessionKey(session);
      const fullKey = buildDetailCacheKey(sessionKey, "full");
      const conversationKey = buildDetailCacheKey(sessionKey, "conversation");
      const userOnlyKey = buildDetailCacheKey(sessionKey, "user_only");

      const fullDetail = detailCache[fullKey];
      const conversationDetail = detailCache[conversationKey];
      const userOnlyDetail = detailCache[userOnlyKey];
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
            : previewTruncated && !userOnlyDetail && !fullDetail;

      const detailError =
        detailMode === "full"
          ? detailErrors[fullKey]
          : detailMode === "conversation"
            ? detailErrors[conversationKey]
            : detailErrors[userOnlyKey] ?? detailErrors[fullKey];

      const loadingKey =
        detailMode === "full"
          ? fullKey
          : detailMode === "conversation"
            ? conversationKey
            : previewTruncated && !fullDetail
              ? userOnlyKey
              : null;

      const detailLoading = loadingKey ? isDetailLoading(loadingKey) : false;

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

          if (needsFetch || detailLoading || !userOnlyDetail) {
            messages = [
              ...messages,
              {
                key: `${sessionKey}-${detailError ? "error" : "loading"}`,
                detail: {
                  actor: "system",
                  category: "session_meta",
                  label: detailError ? "Error" : "Loading",
                  text: detailError
                    ? `Failed to load transcript: ${detailError}`
                    : "Loading full user transcript…",
                  summary_text: detailError
                    ? `Failed to load transcript: ${detailError}`
                    : "Loading full transcript…",
                  data: null,
                },
              },
            ];
          }
        } else if (detailError && (needsFetch || detailLoading)) {
          messages = [
            ...messages,
            {
              key: `${sessionKey}-error`,
              detail: {
                actor: "system",
                category: "session_meta",
                label: "Error",
                text: `Failed to load transcript: ${detailError}`,
                summary_text: `Failed to load transcript: ${detailError}`,
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
        } else if (needsFetch) {
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
  }, [
    sessions,
    detailMode,
    detailCache,
    detailErrors,
    isDetailLoading,
  ]);

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
