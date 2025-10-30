"use client";

import { useEffect } from "react";
import SessionDetailModeToggle from "@/components/SessionDetailModeToggle";
import SessionListView, { SessionListItem } from "@/components/SessionListView";
import ResumeCommandButton from "@/features/command/components/ResumeCommandButton";
import { useSessionDetailMode } from "@/features/sessions/hooks/useSessionDetailMode";
import { useSessionDetails } from "@/features/sessions/hooks/useSessionDetails";
import { buildSessionListItem } from "@/features/sessions/utils/buildSessionListItem";
import { WorktreeSessionSummary } from "@/types";

interface WorktreeSessionsProps {
  sessions: WorktreeSessionSummary[];
  formatTimestamp: (value?: string | null) => string;
  worktreeId: string;
}

export default function WorktreeSessions({
  sessions,
  formatTimestamp,
  worktreeId,
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

    const detailResponse =
      detailMode === "full"
        ? fullDetail
        : detailMode === "conversation"
          ? conversationDetail
          : userOnlyDetail ?? fullDetail ?? null;

    const detailError =
      detailMode === "full"
        ? fullError
        : detailMode === "conversation"
          ? conversationError
          : userOnlyError;

    const needsFetch =
      detailMode === "full"
        ? !fullDetail
        : detailMode === "conversation"
          ? !conversationDetail
          : false;

    const detailLoading =
      detailMode === "full"
        ? fullFetching && !fullDetail
        : detailMode === "conversation"
          ? conversationFetching && !conversationDetail
          : previewTruncated && !fullDetail
            ? userOnlyFetching
            : false;

    const showDetailLoading =
      detailMode === "user_only" ? false : needsFetch || detailLoading;

    const showUserOnlyLoading =
      detailMode === "user_only"
        ? previewTruncated && !fullDetail
          ? userOnlyFetching
          : false
        : false;

    return buildSessionListItem({
      session,
      detailMode,
      detailResponse,
      detailError,
      showDetailLoading,
      previewTruncated,
      showUserOnlyLoading,
      headerActions: (
        <ResumeCommandButton
          provider={session.provider}
          sessionId={session.session_id}
          worktreeId={worktreeId}
        />
      ),
    });
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
