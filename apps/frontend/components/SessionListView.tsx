"use client";

import { ReactNode, useState } from "react";
import type { SessionEvent, SessionToolEvent } from "@/types";
import { ChevronDownIcon, ChevronRightIcon } from "@heroicons/react/24/outline";
import { SquareFunction } from "lucide-react";
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
  headline?: string;
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function coerceString(value: unknown): string | null {
  if (typeof value === "string") {
    const trimmed = value.trim();
    return trimmed.length > 0 ? trimmed : null;
  }
  return null;
}

function coerceNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string") {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

function extractToolTextOutput(output: unknown): string | null {
  if (!output) {
    return null;
  }

  if (typeof output === "string") {
    const trimmed = output.trim();
    return trimmed.length > 0 ? trimmed : null;
  }

  if (Array.isArray(output)) {
    const segments = output
      .map((entry) => {
        if (!entry || typeof entry !== "object") {
          return null;
        }
        const maybeText = (entry as Record<string, unknown>).text;
        return typeof maybeText === "string" ? maybeText.trim() : null;
      })
      .filter((segment): segment is string => Boolean(segment));

    if (segments.length > 0) {
      return segments.join("\n\n");
    }
  }

  if (isRecord(output) && typeof output.text === "string") {
    const trimmed = output.text.trim();
    return trimmed.length > 0 ? trimmed : null;
  }

  return null;
}

function renderRawDetails(label: string, value: unknown) {
  const formatted = formatStructuredValue(value);
  if (!formatted) {
    return null;
  }

  return (
    <details
      key={label}
      className="rounded border border-dashed border-gray-200 bg-white px-3 py-2 text-xs text-gray-600"
    >
      <summary className="cursor-pointer select-none text-xs font-semibold uppercase tracking-wide text-gray-600">
        {label}
      </summary>
      <pre className="mt-2 whitespace-pre-wrap break-all text-xs text-gray-700">
        {formatted}
      </pre>
    </details>
  );
}

function renderUsageSummary(detail: SessionEvent) {
  const data = detail.data;
  const tokenCount = isRecord(data) ? coerceNumber(data.token_count) : null;
  const cost = isRecord(data) ? coerceString(data.cost) : null;
  const duration = isRecord(data) ? coerceString(data.duration) : null;

  const items: Array<{ label: string; value: string }> = [];
  if (tokenCount !== null) {
    items.push({ label: "Tokens", value: tokenCount.toLocaleString() });
  }
  if (cost) {
    items.push({ label: "Cost", value: cost });
  }
  if (duration) {
    items.push({ label: "Duration", value: duration });
  }

  return items.length > 0 ? (
    <dl className="grid grid-cols-1 gap-2 text-xs text-gray-600 sm:grid-cols-3">
      {items.map((item) => (
        <div
          key={item.label}
          className="rounded border border-gray-200 bg-white/80 px-3 py-2"
        >
          <dt className="text-[11px] font-semibold uppercase tracking-wide text-gray-600">
            {item.label}
          </dt>
          <dd className="mt-1 font-mono text-xs text-gray-800">{item.value}</dd>
        </div>
      ))}
    </dl>
  ) : null;
}

function hasRenderableContent(value: unknown): boolean {
  if (value === null || value === undefined) {
    return false;
  }
  if (typeof value === "string") {
    return value.trim().length > 0;
  }
  if (Array.isArray(value)) {
    return value.length > 0;
  }
  if (typeof value === "object") {
    return Object.keys(value as Record<string, unknown>).length > 0;
  }
  return true;
}

function formatStructuredValue(value: unknown): string {
  if (value === null || value === undefined) {
    return "";
  }
  if (typeof value === "string") {
    return value;
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function buildToolRender(
  detail: SessionEvent,
  tool: SessionToolEvent,
  index: number,
  formatTimestamp: (value?: string | null) => string,
): SessionMessageRenderResult {
  const phaseLabel = tool.phase === "use" ? "Tool Use" : "Tool Result";
  const titlePrefix = `#${index + 1}`;
  const titleParts = [phaseLabel];
  if (tool.name && tool.name.trim().length > 0) {
    titleParts.push(tool.name);
  }
  const baseTitle = titleParts.join(" · ");
  const title = baseTitle ? `${titlePrefix} · ${baseTitle}` : titlePrefix;

  const subtitleParts: string[] = [];
  if (tool.identifier && tool.identifier.trim().length > 0) {
    subtitleParts.push(`ID ${tool.identifier}`);
  }
  if (detail.timestamp) {
    const formatted = formatTimestamp(detail.timestamp);
    if (formatted !== "unknown") {
      subtitleParts.push(formatted);
    }
  }
  const subtitle = subtitleParts.join(" • ");

  const metadataItems: Array<{ label: string; value: string }> = [];
  if (tool.working_dir && tool.working_dir.trim().length > 0) {
    metadataItems.push({ label: "Working dir", value: tool.working_dir });
  }

  const summaryBlocks: ReactNode[] = [];
  const rawBlocks: ReactNode[] = [];
  const collapseCandidates: string[] = [];
  const appendRaw = (label: string, value: unknown) => {
    const rendered = renderRawDetails(label, value);
    if (rendered) {
      rawBlocks.push(rendered);
    }
  };
  let outputHandled = false;

  if (tool.phase === "use" && (tool.name ?? "").trim().toLowerCase() === "task") {
    if (isRecord(tool.input)) {
      const description = coerceString(tool.input.description);
      const subagent = coerceString(tool.input.subagent_name);
      const prompt = coerceString(tool.input.prompt);

      if (description || subagent) {
        summaryBlocks.push(
          <div
            key="task-summary"
            className="rounded border border-zinc-200 bg-white/80 px-3 py-2 text-sm text-gray-700"
          >
            <div className="flex flex-wrap items-center gap-2">
              <p className="font-medium text-gray-800">{description ?? "Task orchestration"}</p>
              {subagent ? (
                <span className="rounded-full border border-zinc-200 bg-zinc-50 px-2 py-0.5 text-[11px] font-semibold uppercase tracking-wide text-zinc-600">
                  {subagent}
                </span>
              ) : null}
            </div>
          </div>,
        );
        if (description) {
          collapseCandidates.push(description);
        }
      }

      if (prompt) {
        summaryBlocks.push(
          <div key="task-preview" className="rounded border border-zinc-200 bg-white px-3 py-2">
            <p className="text-[11px] font-semibold uppercase tracking-wide text-zinc-600">
              Prompt preview
            </p>
            <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap text-xs text-gray-800">
              {prompt}
            </pre>
          </div>,
        );
        collapseCandidates.push(prompt);
      }

      appendRaw("Raw tool input", tool.input);
    } else if (hasRenderableContent(tool.input)) {
      const formatted = formatStructuredValue(tool.input);
      summaryBlocks.push(
        <div key="input" className="rounded border border-zinc-200 bg-white px-3 py-2">
          <p className="text-[11px] font-semibold uppercase tracking-wide text-zinc-600">Input</p>
          <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap text-xs text-gray-800">
            {formatted}
          </pre>
        </div>,
      );
      collapseCandidates.push(formatted);
    }
  } else if (hasRenderableContent(tool.input)) {
    const formatted = formatStructuredValue(tool.input);
    if (formatted) {
      summaryBlocks.push(
        <div key="input" className="rounded border border-zinc-200 bg-white px-3 py-2">
          <p className="text-[11px] font-semibold uppercase tracking-wide text-zinc-600">Input</p>
          <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap text-xs text-gray-800">
            {formatted}
          </pre>
        </div>,
      );
      collapseCandidates.push(formatted);
    }
    appendRaw("Raw tool input", tool.input);
  }

  if (tool.phase === "result") {
    const textOutput = extractToolTextOutput(tool.output);
    if (textOutput) {
      summaryBlocks.push(
        <div key="output" className="rounded border border-zinc-200 bg-white px-3 py-2">
          <p className="text-[11px] font-semibold uppercase tracking-wide text-zinc-600">
            Output
          </p>
          <pre className="mt-2 max-h-96 overflow-auto whitespace-pre-wrap text-xs text-gray-800">
            {textOutput}
          </pre>
        </div>,
      );
      collapseCandidates.push(textOutput);
      appendRaw("Raw tool output", tool.output);
      outputHandled = true;
    }
  }

  if (!outputHandled && hasRenderableContent(tool.output)) {
    const formatted = formatStructuredValue(tool.output);
    if (formatted) {
      summaryBlocks.push(
        <div key="output" className="rounded border border-zinc-200 bg-white px-3 py-2">
          <p className="text-[11px] font-semibold uppercase tracking-wide text-zinc-600">
            Output
          </p>
          <pre className="mt-2 max-h-96 overflow-auto whitespace-pre-wrap text-xs text-gray-800">
            {formatted}
          </pre>
        </div>,
      );
      collapseCandidates.push(formatted);
    }
    appendRaw("Raw tool output", tool.output);
    outputHandled = true;
  }

  if (hasRenderableContent(tool.extras)) {
    const formatted = formatStructuredValue(tool.extras);
    if (formatted) {
      summaryBlocks.push(
        <div key="extras" className="rounded border border-zinc-200 bg-white px-3 py-2">
          <p className="text-[11px] font-semibold uppercase tracking-wide text-zinc-600">
            Extras
          </p>
          <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap text-xs text-gray-800">
            {formatted}
          </pre>
        </div>,
      );
      collapseCandidates.push(formatted);
    }
    appendRaw("Raw extras", tool.extras);
  }

  if (summaryBlocks.length === 0) {
    const text = detail.text?.trim();
    if (text) {
      summaryBlocks.push(
        <div key="message" className="rounded border border-zinc-200 bg-white px-3 py-2">
          <p className="text-[11px] font-semibold uppercase tracking-wide text-zinc-600">
            Message
          </p>
          <pre className="mt-2 whitespace-pre-wrap text-xs text-gray-800">{text}</pre>
        </div>,
      );
      collapseCandidates.push(text);
    }
  }

  const accent = getMessageAccent(detail);
  const badgeTone = accent.badge ?? "border-zinc-200 bg-zinc-50 text-zinc-600";
  const shouldCollapse = collapseCandidates.some((value) => shouldCollapsePlainMessage(value));
  const accentDefault = accent.defaultCollapsed ?? false;
  const collapsible = true;
  const defaultCollapsed = accentDefault || shouldCollapse;
  const header = (
    <div className="flex w-full flex-col gap-1">
      <span
        className={cn(
          "text-xs font-semibold uppercase tracking-wide",
          accent.title ?? "text-gray-600",
        )}
      >
        {title}
      </span>
      <div className="flex flex-wrap items-center gap-2 text-xs text-zinc-500">
        {subtitle ? <span>{subtitle}</span> : null}
        <span
          className={cn(
            "inline-flex items-center rounded-full border px-1.5 py-0.5 font-semibold leading-none",
            badgeTone,
          )}
        >
          <SquareFunction className="h-3.5 w-3.5" aria-hidden="true" />
        </span>
        <span className="font-medium text-zinc-500">{phaseLabel}</span>
      </div>
    </div>
  );

  const content = (
    <div className="space-y-3">
      {metadataItems.length > 0 ? (
        <dl className="grid grid-cols-1 gap-2 text-xs text-gray-600 sm:grid-cols-2">
          {metadataItems.map((item) => (
            <div
              key={`${item.label}:${item.value}`}
              className="rounded border border-zinc-200 bg-white/80 px-3 py-2"
            >
              <dt className="text-xs font-semibold uppercase tracking-wide text-gray-600">
                {item.label}
              </dt>
              <dd className="mt-1 break-all font-mono text-xs text-gray-800">{item.value}</dd>
            </div>
          ))}
        </dl>
      ) : null}
      {summaryBlocks}
      {rawBlocks}
      {summaryBlocks.length === 0 && rawBlocks.length === 0 ? (
        <p className="text-xs italic text-gray-500">No structured tool payload available.</p>
      ) : null}
    </div>
  );

  return {
    header,
    title,
    subtitle,
    content,
    collapsible,
    defaultCollapsed,
    containerClassName: accent.container,
    titleClassName: accent.title,
  };
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

function deriveInstructionTitle(body: string): string | null {
  const docMatch = body.match(/\b([A-Za-z0-9._-]+\.md)\b/);
  if (docMatch) {
    return docMatch[1];
  }
  const headingMatch = body.match(/^#{1,6}\s+(.+)$/m);
  if (headingMatch) {
    return headingMatch[1].trim();
  }
  return null;
}

function parseUserMessage(message: string): ParsedUserMessage {
  const trimmed = message.trim();
  const tagMatch = trimmed.match(/^<([a-z_]+)>([\s\S]*?)<\/\1>\s*$/i);

  if (tagMatch) {
    const tag = tagMatch[1].toLowerCase() as SpecialMessageType | string;
    const body = tagMatch[2].trim();

    if (tag === "user_instructions") {
      const derivedTitle = deriveInstructionTitle(body);

      return {
        kind: "special",
        message: {
          type: "user_instructions",
          title: "Codex AGENTS.md",
          headline: derivedTitle ?? undefined,
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
  badge?: string;
  defaultCollapsed?: boolean;
}

const DEFAULT_MESSAGE_ACCENT: MessageAccent = {
  container: "border-gray-200 bg-gray-50",
  title: "text-gray-600",
  badge: "border-gray-200 bg-gray-50 text-gray-600",
  defaultCollapsed: false,
};

function getMessageAccent(detail: SessionEvent): MessageAccent {
  const category = detail.category.trim().toLowerCase();
  if (category === "tool_use" || category === "tool_result") {
    return {
      container: "border-stone-300 bg-stone-100",
      title: "text-stone-800",
      badge: "border-stone-300 bg-stone-100 text-stone-700",
      defaultCollapsed: true,
    };
  }

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
  const normalizedCategory = detail.category.trim().toLowerCase();
  if (detail.tool) {
    return buildToolRender(detail, detail.tool, index, formatTimestamp);
  }
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
  if (normalizedCategory === "_usage") {
    if (isRecord(detail.data)) {
      const totalTokens = coerceNumber(detail.data.token_count);
      if (totalTokens !== null) {
        subtitleParts.push(`Tokens ${totalTokens.toLocaleString()}`);
      }
    }
    if (subtitleParts.length === 0 && detail.text) {
      subtitleParts.push(detail.text);
    }
  }
  const subtitle = subtitleParts.join(" • ");

  if (parsed.kind === "special") {
    const special = parsed.message;
    const titlePrefix = `#${index + 1}`;
    const titleSegments: string[] = [];
    const addTitleSegment = (value?: string | null) => {
      if (!value) {
        return;
      }
      const trimmed = value.trim();
      if (!trimmed) {
        return;
      }
      const lower = trimmed.toLowerCase();
      if (!titleSegments.some((segment) => segment.toLowerCase() === lower)) {
        titleSegments.push(trimmed);
      }
    };
    addTitleSegment(detail.label ?? null);
    addTitleSegment(special.title);
    addTitleSegment(special.headline ?? null);

    const headline = special.headline?.trim();
    const headlineLower = headline?.toLowerCase();
    const baseSegments =
      headlineLower !== undefined
        ? titleSegments.filter((segment) => segment.toLowerCase() !== headlineLower)
        : titleSegments;
    const baseTitle =
      baseSegments.length > 0 ? `${titlePrefix} · ${baseSegments.join(" · ")}` : titlePrefix;
    const titleNode = headline ? (
      <span className="inline-flex items-center gap-2">
        <span>{baseTitle}</span>
        <span className="normal-case rounded border border-indigo-200 bg-indigo-50 px-1.5 py-0.5 text-[10px] font-medium text-indigo-600">
          {headline}
        </span>
      </span>
    ) : (
      baseTitle
    );
    return {
      title: titleNode,
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

  if (normalizedCategory === "_usage") {
    const usageSummary = renderUsageSummary(detail);
    const rawBlocks: ReactNode[] = [];
    const dataBlock = renderRawDetails("Raw usage payload", detail.data ?? null);
    if (dataBlock) {
      rawBlocks.push(dataBlock);
    }
    if (detail.text) {
      const textBlock = renderRawDetails("Raw usage text", detail.text);
      if (textBlock) {
        rawBlocks.push(textBlock);
      }
    }
    const accent = getMessageAccent(detail);

    return {
      title,
      subtitle,
      content: (
        <div className="space-y-3">
          {usageSummary ?? (
            <p className="text-xs text-gray-500">No usage metrics available.</p>
          )}
          {rawBlocks}
        </div>
      ),
      collapsible: true,
      defaultCollapsed: true,
      containerClassName: accent.container,
      titleClassName: accent.title,
    };
  }

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
      <ScrollArea className="flex-1 min-h-0">
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
