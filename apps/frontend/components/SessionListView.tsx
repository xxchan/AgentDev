"use client";

import { ReactNode, useState } from "react";
import type { SessionEvent } from "@/types";
import { ChevronDownIcon, ChevronRightIcon } from "@heroicons/react/24/outline";
import { cn } from "@/lib/utils";
import { getProviderBadgeClasses } from "@/lib/providers";
import { ScrollArea } from "@/components/ui/scroll-area";

type SpecialMessageType =
  | "user_instructions"
  | "environment_context"
  | "user_action";

interface TagEntry {
  key: string;
  label: string;
  value: string;
}

interface SpecialMessageBase {
  type: SpecialMessageType;
  title: string;
  collapsible: boolean;
  defaultCollapsed: boolean;
  accent: "indigo" | "emerald" | "blue";
}

interface UserInstructionsMessage extends SpecialMessageBase {
  type: "user_instructions";
  body: string;
}

interface EnvironmentContextMessage extends SpecialMessageBase {
  type: "environment_context";
  entries: TagEntry[];
}

interface UserActionMessage extends SpecialMessageBase {
  type: "user_action";
  sections: TagEntry[];
}

type SpecialMessage =
  | UserInstructionsMessage
  | EnvironmentContextMessage
  | UserActionMessage;

type ParsedUserMessage =
  | { kind: "special"; message: SpecialMessage }
  | { kind: "default"; text: string; shouldCollapse: boolean };

export interface SessionListMessage {
  key: string;
  detail: SessionEvent;
}

export interface SessionMessageRenderResult {
  header?: ReactNode;
  title?: ReactNode;
  subtitle?: ReactNode;
  content: ReactNode;
  collapsible: boolean;
  defaultCollapsed: boolean;
  containerClassName?: string;
  titleClassName?: string;
}

export interface SessionMessageRendererContext {
  index: number;
  session: SessionListItem;
  formatTimestamp: (value?: string | null) => string;
  defaultRender: () => SessionMessageRenderResult;
}

export type SessionMessageRenderer = (
  message: SessionListMessage,
  context: SessionMessageRendererContext,
) => SessionMessageRenderResult | null;

function shouldCollapsePlainMessage(message: string) {
  if (!message) {
    return false;
  }
  if (message.length > 320) {
    return true;
  }
  return message.split(/\r?\n/).length > 8;
}

function toStartCase(value: string) {
  return value
    .split(/[_\s-]+/)
    .filter(Boolean)
    .map((segment) => segment.charAt(0).toUpperCase() + segment.slice(1))
    .join(" ");
}

function parseTagEntries(body: string): TagEntry[] {
  const pattern = /<([a-z0-9_\-:]+)>([\s\S]*?)<\/\1>/gi;
  const entries: TagEntry[] = [];
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(body)) !== null) {
    const [, key, rawValue] = match;
    const value = rawValue.trim();
    if (value) {
      entries.push({
        key,
        label: toStartCase(key),
        value,
      });
    }
  }
  return entries;
}

function sortEntries(entries: TagEntry[], preferredOrder: string[]) {
  const orderMap = preferredOrder.reduce<Record<string, number>>((acc, key, index) => {
    acc[key] = index;
    return acc;
  }, {});

  entries.sort((a, b) => {
    const aRank = orderMap[a.key] ?? Number.MAX_SAFE_INTEGER;
    const bRank = orderMap[b.key] ?? Number.MAX_SAFE_INTEGER;
    if (aRank !== bRank) {
      return aRank - bRank;
    }
    return a.key.localeCompare(b.key);
  });
}

function parseUserMessage(message: string): ParsedUserMessage {
  const trimmed = message.trim();
  const tagMatch = trimmed.match(/^<([a-z_]+)>([\s\S]*?)<\/\1>\s*$/i);

  if (tagMatch) {
    const tag = tagMatch[1].toLowerCase() as SpecialMessageType | string;
    const body = tagMatch[2].trim();

    if (tag === "user_instructions") {
      return {
        kind: "special",
        message: {
          type: "user_instructions",
          title: "Codex AGENTS.md",
          collapsible: true,
          defaultCollapsed: true,
          accent: "indigo",
          body,
        },
      };
    }

    if (tag === "environment_context") {
      const entries = parseTagEntries(body);
      if (entries.length > 0) {
        sortEntries(entries, [
          "cwd",
          "approval_policy",
          "sandbox_mode",
          "network_access",
          "shell",
        ]);
        return {
          kind: "special",
          message: {
            type: "environment_context",
            title: "Codex environment context",
            collapsible: true,
            defaultCollapsed: true,
            accent: "emerald",
            entries,
          },
        };
      }
    }

    if (tag === "user_action") {
      const sections = parseTagEntries(body);
      if (sections.length > 0) {
        sortEntries(sections, ["context", "action", "results"]);
        return {
          kind: "special",
          message: {
            type: "user_action",
            title: "Codex user action",
            collapsible: sections.some((entry) =>
              shouldCollapsePlainMessage(entry.value),
            ),
            defaultCollapsed: false,
            accent: "blue",
            sections,
          },
        };
      }
    }
  }

  return {
    kind: "default",
    text: message,
    shouldCollapse: shouldCollapsePlainMessage(message),
  };
}

function getSpecialMessageContainerClasses(type: SpecialMessageType) {
  switch (type) {
    case "user_instructions":
      return "border-indigo-200 bg-indigo-50/70";
    case "environment_context":
      return "border-emerald-200 bg-emerald-50/70";
    case "user_action":
      return "border-blue-200 bg-blue-50/70";
    default:
      return "border-gray-200 bg-gray-50";
  }
}

function getSpecialMessageTitleClass(message: SpecialMessage): string {
  switch (message.accent) {
    case "indigo":
      return "text-indigo-700";
    case "emerald":
      return "text-emerald-700";
    case "blue":
      return "text-blue-700";
    default:
      return "text-gray-600";
  }
}

interface MessageAccent {
  container: string;
  title: string;
  defaultCollapsed?: boolean;
}

const DEFAULT_MESSAGE_ACCENT: MessageAccent = {
  container: "border-gray-200 bg-gray-50",
  title: "text-gray-600",
  defaultCollapsed: false,
};

function getMessageAccent(detail: SessionEvent): MessageAccent {
  const actor = detail.actor?.trim().toLowerCase();
  if (actor) {
    switch (actor) {
      case "user":
        return {
          container: "border-sky-200 bg-sky-50",
          title: "text-sky-700",
          defaultCollapsed: false,
        };
      case "assistant":
        return {
          container: "border-violet-200 bg-violet-50",
          title: "text-violet-700",
          defaultCollapsed: false,
        };
      case "system":
        return {
          container: "border-amber-200 bg-amber-50",
          title: "text-amber-700",
          defaultCollapsed: false,
        };
    }
  }

  const category = detail.category.trim().toLowerCase();
  switch (category) {
    case "session_meta":
      return {
        container: "border-slate-300 bg-slate-50",
        title: "text-slate-700",
        defaultCollapsed: true,
      };
    case "_checkpoint":
      return {
        container: "border-gray-200 bg-gray-50",
        title: "text-gray-600",
        defaultCollapsed: true,
      };
    case "_usage":
      return {
        container: "border-gray-200 bg-gray-50",
        title: "text-gray-600",
        defaultCollapsed: true,
      };
    case "tool_use":
      return {
        container: "border-blue-200 bg-blue-50",
        title: "text-blue-700",
        defaultCollapsed: true,
      };
    case "tool_result":
      return {
        container: "border-cyan-200 bg-cyan-50",
        title: "text-cyan-700",
        defaultCollapsed: true,
      };
    case "response_item":
      return {
        container: "border-purple-200 bg-purple-50",
        title: "text-purple-700",
        defaultCollapsed: false,
      };
    case "assistant_message":
      return {
        container: "border-violet-200 bg-violet-50",
        title: "text-violet-700",
        defaultCollapsed: false,
      };
    case "user_message":
      return {
        container: "border-sky-200 bg-sky-50",
        title: "text-sky-700",
        defaultCollapsed: false,
      };
    default:
      return DEFAULT_MESSAGE_ACCENT;
  }
}

function buildDefaultRender(
  message: SessionListMessage,
  index: number,
  formatTimestamp: (value?: string | null) => string,
): SessionMessageRenderResult {
  const detail = message.detail;
  const baseText =
    detail.text ??
    detail.summary_text ??
    (detail.data ? JSON.stringify(detail.data, null, 2) : "");
  const parsed = baseText
    ? parseUserMessage(baseText)
    : { kind: "default" as const, text: "", shouldCollapse: false };

  const subtitleParts: string[] = [];
  if (detail.timestamp) {
    const formatted = formatTimestamp(detail.timestamp);
    if (formatted !== "unknown") {
      subtitleParts.push(formatted);
    }
  }
  const subtitle = subtitleParts.join(" • ");

  if (parsed.kind === "special") {
    const special = parsed.message;
    const titlePrefix = `#${index + 1}`;
    const baseTitle = detail.label ?? special.title;
    const composedTitle = baseTitle ? `${titlePrefix} · ${baseTitle}` : titlePrefix;
    return {
      title: composedTitle,
      subtitle,
      content: (
        <>
          {special.type === "user_instructions" ? (
            <pre className="mt-2 max-h-96 overflow-auto whitespace-pre-wrap text-sm text-gray-800">
              {special.body}
            </pre>
          ) : null}
          {special.type === "environment_context" ? (
            <dl className="mt-3 grid grid-cols-1 gap-2 text-sm text-gray-700 sm:grid-cols-2">
              {special.entries.map((entry) => (
                <div
                  key={entry.key}
                  className="rounded border border-emerald-200/60 bg-white/70 px-3 py-2"
                >
                  <dt className="text-xs font-semibold uppercase tracking-wide text-emerald-700">
                    {entry.label}
                  </dt>
                  <dd className="mt-1 break-all font-mono text-xs text-gray-800">
                    {entry.value}
                  </dd>
                </div>
              ))}
            </dl>
          ) : null}
          {special.type === "user_action" ? (
            <div className="mt-2 space-y-3">
              {special.sections.map((section) => (
                <div
                  key={section.key}
                  className="rounded border border-blue-200 bg-white/80 px-3 py-2"
                >
                  <p className="text-xs font-semibold uppercase tracking-wide text-blue-700">
                    {section.label}
                  </p>
                  <p className="mt-1 whitespace-pre-wrap text-sm text-gray-800">
                    {section.value}
                  </p>
                </div>
              ))}
            </div>
          ) : null}
        </>
      ),
      collapsible: special.collapsible,
      defaultCollapsed: special.defaultCollapsed,
      containerClassName: getSpecialMessageContainerClasses(special.type),
      titleClassName: getSpecialMessageTitleClass(special),
    };
  }

  const titlePrefix = `#${index + 1}`;
  const baseTitle =
    detail.label ??
    (detail.actor && detail.actor.trim().length > 0
      ? toStartCase(detail.actor)
      : toStartCase(detail.category));
  const title = baseTitle ? `${titlePrefix} · ${baseTitle}` : titlePrefix;

  const isStructuredFallback =
    !detail.text && !detail.summary_text && Boolean(detail.data);

  const content = baseText ? (
    isStructuredFallback ? (
      <pre className="mt-2 max-h-96 overflow-auto whitespace-pre-wrap text-sm text-gray-800">
        {baseText}
      </pre>
    ) : (
      <p className="mt-2 whitespace-pre-wrap text-sm text-gray-700">{baseText}</p>
    )
  ) : (
    <p className="mt-2 text-xs italic text-gray-500">No message content.</p>
  );

  const accent = getMessageAccent(detail);
  const shouldCollapse = shouldCollapsePlainMessage(baseText);
  const accentDefaultCollapsed = accent.defaultCollapsed ?? false;
  const collapsible = shouldCollapse || accentDefaultCollapsed;
  const defaultCollapsed = accentDefaultCollapsed || shouldCollapse;

  return {
    title,
    subtitle,
    content,
    collapsible,
    defaultCollapsed,
    containerClassName: accent.container,
    titleClassName: accent.title,
  };
}
export interface SessionListItem {
  sessionKey: string;
  provider: string;
  sessionId: string;
  lastTimestamp?: string | null;
  messages: SessionListMessage[];
  metadata?: ReactNode;
  headerActions?: ReactNode;
  emptyState?: ReactNode;
}

interface SessionListViewProps {
  title: string;
  description?: string;
  sessions: SessionListItem[];
  formatTimestamp: (value?: string | null) => string;
  emptyState?: ReactNode;
  toolbar?: ReactNode;
  renderMessage?: SessionMessageRenderer;
}

export default function SessionListView({
  title,
  description,
  sessions,
  formatTimestamp,
  emptyState,
  toolbar,
  renderMessage,
}: SessionListViewProps) {
  const [expandedSessionMessages, setExpandedSessionMessages] = useState<
    Record<string, boolean>
  >({});
  const [collapsedSessions, setCollapsedSessions] = useState<
    Record<string, boolean>
  >({});

  const toggleSessionMessage = (key: string, defaultExpanded: boolean) => {
    setExpandedSessionMessages((prev) => {
      const current = prev[key];
      const isExpanded = current ?? defaultExpanded;
      return {
        ...prev,
        [key]: !isExpanded,
      };
    });
  };

  return (
    <section className="flex h-full min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-gray-200 bg-white">
      <header className="flex shrink-0 flex-col gap-2 border-b border-gray-200 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h3 className="text-sm font-semibold uppercase tracking-wide text-gray-900">
            {title}
          </h3>
          {description ? (
            <p className="text-xs text-gray-500">{description}</p>
          ) : null}
        </div>
        <div className="flex flex-col items-start gap-2 sm:flex-row sm:items-center sm:gap-3">
          {toolbar ? <div className="flex items-center gap-2">{toolbar}</div> : null}
          <span className="text-xs text-gray-400">{sessions.length} total</span>
        </div>
      </header>
      <ScrollArea className="flex-1 min-h-0" viewportClassName="pr-4">
        {sessions.length > 0 ? (
          <ul className="divide-y divide-gray-100">
          {sessions.map((session) => {
            const sessionKey = session.sessionKey;
            const isCollapsed = collapsedSessions[sessionKey] ?? false;
            const contentId = `${sessionKey}-content`;
            const ChevronIcon = isCollapsed ? ChevronRightIcon : ChevronDownIcon;

            const toggleSessionCollapsed = () => {
              setCollapsedSessions((prev) => ({
                ...prev,
                [sessionKey]: !isCollapsed,
              }));
            };

            return (
              <li key={sessionKey} className="px-4 py-4 text-sm">
                <div
                  className={`flex flex-wrap items-center justify-between gap-3 rounded-md px-3 py-2 transition-colors ${
                    isCollapsed
                      ? "border border-gray-200 bg-gray-50"
                      : "border border-transparent bg-white"
                  }`}
                >
                  <div className="flex items-center gap-2">
                    <button
                      type="button"
                      onClick={toggleSessionCollapsed}
                      aria-expanded={!isCollapsed}
                      aria-controls={contentId}
                      className="flex items-center gap-1 rounded-md border border-gray-200 bg-white/80 p-1 text-gray-500 transition hover:bg-white focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-200"
                    >
                      <ChevronIcon className="h-4 w-4" />
                      <span className="sr-only">
                        {isCollapsed ? "Expand session" : "Collapse session"}
                      </span>
                    </button>
                    <span
                      className={cn(
                        "rounded-full px-2 py-0.5 text-xs font-medium uppercase tracking-wide",
                        getProviderBadgeClasses(session.provider),
                      )}
                    >
                      {session.provider}
                    </span>
                    <span className="text-xs text-gray-500">
                      {formatTimestamp(session.lastTimestamp)}
                    </span>
                  </div>
                  <div className="flex flex-wrap items-center gap-2">
                    <span
                      className="rounded bg-gray-100 px-2 py-0.5 font-mono text-xs text-gray-700"
                      title={session.sessionId}
                    >
                      {session.sessionId}
                    </span>
                    {session.headerActions}
                  </div>
                </div>
                {!isCollapsed ? (
                  <div id={contentId} className="mt-3 space-y-3">
                    {session.metadata ? (
                      <div className="rounded-md border border-gray-200 bg-gray-50 px-3 py-2 text-xs text-gray-600">
                        {session.metadata}
                      </div>
                    ) : null}
                    {session.messages.length > 0 ? (
                      <ol className="space-y-2">
                        {session.messages.map((message, messageIdx) => {
                          const messageStateKey = `${session.sessionKey}:${message.key}`;
                          const baseRender = buildDefaultRender(
                            message,
                            messageIdx,
                            formatTimestamp,
                          );
                          const customRender = renderMessage
                            ? renderMessage(message, {
                                index: messageIdx,
                                session,
                                formatTimestamp,
                                defaultRender: () => baseRender,
                              })
                            : null;
                          const rendered = customRender ?? baseRender;
                          const showToggle = rendered.collapsible;
                          const defaultExpanded = !rendered.defaultCollapsed;
                          const storedExpansion = expandedSessionMessages[messageStateKey];
                          const isExpanded = showToggle
                            ? storedExpansion ?? defaultExpanded
                            : true;
                          const handleToggle = () => {
                            toggleSessionMessage(messageStateKey, defaultExpanded);
                          };

                          const headerContent = rendered.header ?? (
                            <div className="flex w-full items-center justify-between gap-2">
                              <div className="flex flex-col">
                                {rendered.title ? (
                                  <span
                                    className={cn(
                                      "text-xs font-semibold uppercase tracking-wide",
                                      rendered.titleClassName ?? "text-gray-600",
                                    )}
                                  >
                                    {rendered.title}
                                  </span>
                                ) : null}
                                {rendered.subtitle ? (
                                  <span className="text-xs text-gray-500">
                                    {rendered.subtitle}
                                  </span>
                                ) : null}
                              </div>
                              {showToggle ? (
                                <span className="text-xs text-gray-400">
                                  {isExpanded ? "Collapse" : "Expand"}
                                </span>
                              ) : null}
                            </div>
                          );

                          return (
                            <li
                              key={message.key}
                              className={`overflow-hidden rounded-md border ${
                                rendered.containerClassName ??
                                "border-gray-200 bg-gray-50"
                              }`}
                            >
                              {showToggle ? (
                                <button
                                  type="button"
                                  onClick={handleToggle}
                                  className="flex w-full items-center justify-between gap-2 px-3 py-2 text-left hover:bg-gray-900/5 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-200"
                                  aria-expanded={isExpanded}
                                >
                                  {headerContent}
                                </button>
                              ) : (
                                <div className="flex w-full items-center justify-between gap-2 px-3 py-2">
                                  {headerContent}
                                </div>
                              )}
                              {(!showToggle || isExpanded) && (
                                <div className="px-3 pb-3">{rendered.content}</div>
                              )}
                            </li>
                          );
                        })}
                      </ol>
                    ) : (
                      <div className="rounded-md border border-dashed border-gray-200 bg-gray-50 px-3 py-6 text-center text-xs text-gray-500">
                        {session.emptyState ?? "No messages captured yet."}
                      </div>
                    )}
                  </div>
                ) : null}
              </li>
            );
          })}
        </ul>
      ) : (
        <div className="px-4 py-6 text-sm text-gray-500">
          {emptyState ?? "No sessions available."}
        </div>
        )}
      </ScrollArea>
    </section>
  );
}
