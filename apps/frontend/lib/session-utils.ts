import type {
  BaseSessionSummary,
  SessionDetailMode,
  SessionDetailResponse,
  SessionEvent,
} from '@/types';
import type { SessionListMessage } from '@/components/SessionListView';

export const SESSION_DETAIL_MODE_STORAGE_KEY = 'agentdev.sessions.detail-mode';

export function getSessionKey(
  session: Pick<BaseSessionSummary, 'provider' | 'session_id'>,
): string {
  return `${session.provider}-${session.session_id}`;
}

export function buildDetailCacheKey(
  sessionKey: string,
  mode: SessionDetailMode,
): string {
  return `${sessionKey}|${mode}`;
}

export function toSessionListMessages(
  events: SessionEvent[],
  sessionKey: string,
  scope: string,
): SessionListMessage[] {
  return events.map((detail, index) => ({
    key: `${sessionKey}-${scope}-${index}`,
    detail,
  }));
}

export function buildUserOnlyMessages(
  session: BaseSessionSummary,
  detail?: SessionDetailResponse | null,
): SessionListMessage[] {
  const sessionKey = getSessionKey(session);
  const previewMessages = session.user_messages_preview ?? [];
  const baseEvents =
    detail?.events ??
    previewMessages.map<SessionEvent>((text) => ({
      actor: 'user',
      category: 'user',
      label: 'User',
      text,
      summary_text: text,
      data: null,
    }));
  const events =
    detail && detail.mode === 'user_only'
      ? baseEvents
      : baseEvents.filter(
          (event) => (event.actor ?? '').toLowerCase() === 'user',
        );
  return toSessionListMessages(events, sessionKey, 'user');
}
