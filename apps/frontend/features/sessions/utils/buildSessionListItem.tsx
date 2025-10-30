import { ReactNode } from 'react';
import type {
  BaseSessionSummary,
  SessionDetailMode,
  SessionDetailResponse,
} from '@/types';
import {
  buildUserOnlyMessages,
  getSessionKey,
  toSessionListMessages,
} from '@/lib/session-utils';
import type { SessionListItem, SessionListMessage } from '@/components/SessionListView';

interface BuildSessionListItemArgs<T extends BaseSessionSummary> {
  session: T;
  detailMode: SessionDetailMode;
  detailResponse: SessionDetailResponse | null;
  detailError: string | null;
  showDetailLoading: boolean;
  previewTruncated: boolean;
  showUserOnlyLoading: boolean;
  metadata?: ReactNode;
  headerActions?: ReactNode;
}

function buildLoadingMessage(sessionKey: string): SessionListMessage {
  return {
    key: `${sessionKey}-loading`,
    detail: {
      actor: 'system',
      category: 'session_meta',
      label: 'Loading',
      text: 'Loading full user transcript…',
      summary_text: 'Loading full transcript…',
      data: null,
    },
  };
}

function buildErrorMessage(sessionKey: string, error: string): SessionListMessage {
  const message = `Failed to load transcript: ${error}`;
  return {
    key: `${sessionKey}-error`,
    detail: {
      actor: 'system',
      category: 'session_meta',
      label: 'Error',
      text: message,
      summary_text: message,
      data: null,
    },
  };
}

function buildPreviewNote(sessionKey: string, shown: number, total: number): SessionListMessage {
  return {
    key: `${sessionKey}-preview-note`,
    detail: {
      actor: 'system',
      category: 'session_meta',
      label: 'Preview',
      text: `Showing ${shown} of ${total} user messages.`,
      summary_text: 'Showing limited user messages',
      data: null,
    },
  };
}

export function buildSessionListItem<T extends BaseSessionSummary>(
  args: BuildSessionListItemArgs<T>,
): SessionListItem {
  const {
    session,
    detailMode,
    detailResponse,
    detailError,
    showDetailLoading,
    previewTruncated,
    showUserOnlyLoading,
    metadata,
    headerActions,
  } = args;

  const sessionKey = getSessionKey(session);

  let messages: SessionListMessage[] =
    detailMode === 'full'
      ? toSessionListMessages(detailResponse?.events ?? [], sessionKey, 'full')
      : detailMode === 'conversation'
        ? toSessionListMessages(detailResponse?.events ?? [], sessionKey, 'conversation')
        : buildUserOnlyMessages(session, detailResponse);

  if (detailMode === 'user_only') {
    const userMessagesShown = messages.filter(
      (entry) => (entry.detail.actor ?? '').toLowerCase() === 'user',
    ).length;

    if (previewTruncated && userMessagesShown < session.user_message_count) {
      messages = [...messages, buildPreviewNote(sessionKey, userMessagesShown, session.user_message_count)];
    }

    if (detailError) {
      messages = [...messages, buildErrorMessage(sessionKey, detailError)];
    } else if (showUserOnlyLoading) {
      messages = [...messages, buildLoadingMessage(sessionKey)];
    }
  }

  const item: SessionListItem = {
    sessionKey,
    provider: session.provider,
    sessionId: session.session_id,
    lastTimestamp: session.last_timestamp,
    messages,
    metadata,
    headerActions,
  };

  if (detailMode === 'full' || detailMode === 'conversation') {
    if (detailError) {
      item.emptyState = (
        <div className="text-xs text-destructive">
          Failed to load transcript: {detailError}
        </div>
      );
    } else if (showDetailLoading) {
      item.emptyState = (
        <div className="flex items-center justify-center gap-2 text-xs text-muted-foreground">
          <span className="inline-block h-3 w-3 animate-spin rounded-full border-2 border-border border-t-primary" />
          Loading transcript…
        </div>
      );
    } else if (messages.length === 0) {
      item.emptyState =
        detailMode === 'conversation'
          ? 'No conversation messages found.'
          : 'No transcript entries found.';
    }
  } else if (!detailError && !showUserOnlyLoading) {
    const userMessageCount = messages.filter(
      (entry) => (entry.detail.actor ?? '').toLowerCase() === 'user',
    ).length;
    if (userMessageCount === 0) {
      item.emptyState = 'No user messages recorded.';
    }
  }

  return item;
}
