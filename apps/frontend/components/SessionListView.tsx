"use client";

import { ReactNode, useState } from "react";
import { ChevronDownIcon, ChevronRightIcon } from "@heroicons/react/24/outline";

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
export interface SessionListItem {
  sessionKey: string;
  provider: string;
  sessionId: string;
  lastTimestamp?: string | null;
  userMessages: string[];
  metadata?: ReactNode;
  headerActions?: ReactNode;
}

interface SessionListViewProps {
  title: string;
  description?: string;
  sessions: SessionListItem[];
  formatTimestamp: (value?: string | null) => string;
  emptyState?: ReactNode;
}

export default function SessionListView({
  title,
  description,
  sessions,
  formatTimestamp,
  emptyState,
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
    <section className="rounded-lg border border-gray-200 bg-white">
      <header className="flex items-center justify-between border-b border-gray-200 px-4 py-3">
        <div>
          <h3 className="text-sm font-semibold uppercase tracking-wide text-gray-900">
            {title}
          </h3>
          {description ? (
            <p className="text-xs text-gray-500">{description}</p>
          ) : null}
        </div>
        <span className="text-xs text-gray-400">{sessions.length} total</span>
      </header>
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
                    <span className="rounded-full bg-blue-50 px-2 py-0.5 text-xs font-medium uppercase tracking-wide text-blue-700">
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
                    {session.userMessages.length > 0 ? (
                      <ol className="space-y-2">
                        {session.userMessages.map((message, messageIdx) => {
                          const messageKey = `${session.sessionKey}-${messageIdx}`;
                          const parsed = parseUserMessage(message);
                          const defaultExpanded =
                            parsed.kind === "special"
                              ? !parsed.message.defaultCollapsed
                              : !parsed.shouldCollapse;
                          const showToggle =
                            parsed.kind === "special"
                              ? parsed.message.collapsible
                              : parsed.shouldCollapse;
                          const storedExpansion =
                            expandedSessionMessages[messageKey];
                          const isExpanded = showToggle
                            ? storedExpansion ?? defaultExpanded
                            : true;
                          const handleToggle = () => {
                            toggleSessionMessage(messageKey, defaultExpanded);
                          };

                          const headerContent = (
                            <div className="flex w-full items-center justify-between gap-2">
                              <div className="flex flex-col">
                                <span className="text-xs font-semibold uppercase tracking-wide text-gray-600">
                                  {parsed.kind === "special"
                                    ? parsed.message.title
                                    : "User Message"}
                                </span>
                                <span className="text-xs text-gray-500">
                                  Message {messageIdx + 1}
                                </span>
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
                              key={messageKey}
                              className={`overflow-hidden rounded-md border ${
                                parsed.kind === "special"
                                  ? getSpecialMessageContainerClasses(
                                      parsed.message.type,
                                    )
                                  : "border-gray-200 bg-gray-50"
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
                                <div className="px-3 pb-3">
                                  {parsed.kind === "special" ? (
                                    <>
                                      {parsed.message.type === "user_instructions" ? (
                                        <pre className="mt-2 max-h-96 overflow-auto whitespace-pre-wrap text-sm text-gray-800">
                                          {parsed.message.body}
                                        </pre>
                                      ) : null}
                                      {parsed.message.type === "environment_context" ? (
                                        <dl className="mt-3 grid grid-cols-1 gap-2 text-sm text-gray-700 sm:grid-cols-2">
                                          {parsed.message.entries.map((entry) => (
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
                                      {parsed.message.type === "user_action" ? (
                                        <div className="mt-2 space-y-3">
                                          {parsed.message.sections.map((section) => (
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
                                  ) : (
                                    <p className="mt-2 whitespace-pre-wrap text-sm text-gray-700">
                                      {message}
                                    </p>
                                  )}
                                </div>
                              )}
                            </li>
                          );
                        })}
                      </ol>
                    ) : (
                      <p className="text-sm text-gray-500">
                        No user messages recorded
                      </p>
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
    </section>
  );
}
