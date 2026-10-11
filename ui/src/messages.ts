// The conversation message stream — user bubbles + Markdown answers (streamed in place) — shared by
// the Command Panel's Quick Ask page and the Side View, so the two surfaces cannot drift apart.
// The owner supplies the scroll container; this module renders into it and tracks the last answer
// (the copy / write-back target).

import DOMPurify from "dompurify";
import { marked } from "marked";
import { generatingEl } from "./generating";
import { iconEl } from "./icons";
import type { Message, ToolCall, ToolCallStatus } from "./types";

export interface ConversationView {
  /** Remove every message (loading another conversation / starting a new chat). */
  clear(): void;
  /** Append a finished message (loaded history or a just-sent user bubble); returns its row for rollback. */
  append(message: Message): HTMLElement;
  /** A new answer begins: the next frames render into a fresh bubble (the finished one stays in place). */
  beginAnswer(): void;
  /**
   * Streaming frame: the full accumulated answer text + whether it is still being produced.
   * The body and the indicator are separate elements, so each frame only replaces the body and the
   * dot animation is never restarted. `reasoning` (a model's thinking block) renders as the muted
   * ChatGPT-style row above the answer, never inside the body (MOE-0008/0011). `toolCalls` (an
   * agentic turn's activity stack) replaces the rendered rows when present — frames carry the full
   * list in arrival order — and leaves them untouched when absent (MOE-0026).
   */
  updateStreaming(
    text: string,
    pending: boolean,
    reasoning?: string,
    toolCalls?: ToolCall[],
  ): void;
  /** Hide the generation indicator without touching the text (a user-requested stop); later frames still land here. */
  settleStreaming(): void;
  /** Drop a streaming bubble that never received text (a failed send): no empty bubble is left. */
  discardAnswer(): void;
  /** The last answer's text (null when there is none yet): the copy / write-back target. */
  lastAnswer(): string | null;
  /** Whether no message is shown (the blank new-chat state). */
  isEmpty(): boolean;
  /** Pin the view to the bottom, correcting once after async layout (markdown/code/images expand, IIE4AD-369). */
  scrollToEnd(): void;
}

/** Title rule shared by the panel page and the Side View: first line of the first user message. */
export function conversationTitle(messages: Message[]): string | null {
  const first = messages.find((message) => message.role === "user" && message.content.trim());
  if (!first) return null;
  const line = first.content.trim().split("\n")[0];
  return line.length > 24 ? `${line.slice(0, 24)}…` : line;
}

function markdown(text: string): string {
  return DOMPurify.sanitize(marked.parse(text, { async: false }));
}

interface StreamingBubble {
  reasoning: HTMLDetailsElement;
  reasoningLabel: HTMLElement;
  reasoningBody: HTMLElement;
  /** The Tool Call activity stack: one row per call, above the answer (MOE-0026). */
  activity: HTMLElement;
  body: HTMLElement;
  indicator: HTMLElement;
}

/**
 * The Thinking row (MOE-0012): ChatGPT's language — borderless, muted, collapsed by default; the
 * label shimmers while the model reasons and swaps to "Thought for N seconds" once it settles; the
 * trailing chevron turns on expand. The row leads with a lightning icon and its expanded content
 * reads as an excerpt (MOE-0036). One shared builder for the chat bubbles here and the panel's
 * result cards (`reasoningEl`). See docs/research/chatgpt-thinking-block.md.
 */
function reasoningParts(): {
  details: HTMLDetailsElement;
  label: HTMLElement;
  body: HTMLElement;
} {
  const details = document.createElement("details");
  details.className = "moe-thinking not-prose mb-2 text-xs text-fg-subtle";
  const summary = document.createElement("summary");
  summary.className =
    "-mx-1.5 flex cursor-pointer select-none items-center gap-1 rounded-md px-1.5 py-0.5 hover:bg-surface-float";
  const label = document.createElement("span");
  const chevron = iconEl("chevron-right", {
    size: 12,
    className: "moe-thinking-chevron ml-auto text-fg-faint",
  });
  summary.append(
    iconEl("zap", { size: 12, className: "text-fg-faint" }),
    label,
    chevron,
  );
  const body = document.createElement("div");
  body.className = "moe-thinking-body md mt-1.5 text-sm";
  details.append(summary, body);
  return { details, label, body };
}

/**
 * ChatGPT's label states (docs/research/chatgpt-thinking-block.md): "Thinking" while the model
 * reasons; "Thought for N seconds" once done (minutes abbreviated, "a few seconds" under one).
 */
export function thinkingLabel(pending: boolean, seconds: number): string {
  if (pending) return "Thinking";
  if (seconds < 1) return "Thought for a few seconds";
  if (seconds < 60) return `Thought for ${seconds} second${seconds === 1 ? "" : "s"}`;
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return rest === 0 ? `Thought for ${minutes}m` : `Thought for ${minutes}m ${rest}s`;
}

/** A settled Thinking row (collapsed by default): the panel's result card renders this directly. */
export function reasoningEl(
  reasoning: string,
  pending: boolean,
  seconds = 0,
): HTMLDetailsElement {
  const { details, label, body } = reasoningParts();
  label.textContent = thinkingLabel(pending, seconds);
  label.classList.toggle("moe-thinking-live", pending);
  body.innerHTML = markdown(reasoning);
  return details;
}

// ---- Tool Call rows (MOE-0026): one row per call, chronological, never aggregated ----

/** The runner persists stopped results with this exact text and prefixes failures with `Error: `
 * (`crates/moe-extensions/src/agent.rs` / `tools.rs`); a record-derived row reads its state from it. */
const STOPPED_RESULT = "(stopped by the user)";
const ERROR_RESULT_PREFIX = "Error: ";

/** A Tool Call row's parts: the details element plus the slots that change in place. */
interface ToolRow {
  details: HTMLDetailsElement;
  label: HTMLElement;
  /** The trailing slot before the chevron: a spinner while running, a word when failed/stopped. */
  status: HTMLElement;
  body: HTMLElement;
}

/** `arguments` arrive as a JSON string; fragments of an in-flight call may not parse yet. */
function toolArguments(raw: string): Record<string, unknown> {
  try {
    const value: unknown = JSON.parse(raw);
    return value !== null && typeof value === "object" ? (value as Record<string, unknown>) : {};
  } catch {
    return {};
  }
}

/** `https://example.com/path?q=1` → `example.com/path`: the label shows where it went, not the scheme. */
function shortUrl(raw: string): string {
  try {
    const url = new URL(raw);
    return url.pathname && url.pathname !== "/" ? `${url.host}${url.pathname}` : url.host;
  } catch {
    return raw;
  }
}

/** Row copy (MOE-0026): `Searched «query»`, `Fetched example.com/path`, `Read ~/x.log`. */
function toolCallLabel(call: ToolCall): { icon: string; label: string } {
  const args = toolArguments(call.arguments);
  switch (call.name) {
    case "web_search": {
      const query = typeof args.query === "string" ? args.query.replace(/\s+/g, " ").trim() : "";
      return { icon: "search", label: query ? `Searched «${query}»` : "Searched the web" };
    }
    case "web_fetch": {
      const url = typeof args.url === "string" ? shortUrl(args.url) : "";
      return { icon: "globe", label: url ? `Fetched ${url}` : "Fetched a page" };
    }
    case "read_file": {
      const path = typeof args.path === "string" ? args.path : "";
      return { icon: "file-text", label: path ? `Read ${path}` : "Read a file" };
    }
    default:
      return { icon: "wand-2", label: `Used ${call.name}` };
  }
}

/** The trailing status slot: spinner while running; failed/stopped calls say so; ok settles quietly. */
function setToolRowStatus(row: ToolRow, status: ToolCallStatus | undefined) {
  row.label.classList.toggle("moe-thinking-live", status === "running");
  row.status.replaceChildren();
  if (status === "running") {
    row.status.append(iconEl("rotate-cw", { size: 12, className: "moe-tool-spin text-fg-faint" }));
  } else if (status === "error" || status === "stopped") {
    const word = document.createElement("span");
    word.className = status === "error" ? "text-danger" : "text-fg-faint";
    word.textContent = status === "error" ? "failed" : "stopped";
    row.status.append(word);
  }
}

/**
 * One Tool Call row: the Thinking row's anatomy (borderless, muted, collapsed, chevron) with the
 * tool's icon + plain label; expanding shows the truncated result as a muted-markdown excerpt
 * (the left rule under the icon, MOE-0036). One row per
 * call — a "used N tools" aggregate is rejected (observability over magic, MOE-0026).
 */
function toolRowEl(call: ToolCall): ToolRow {
  const details = document.createElement("details");
  details.className = "moe-thinking not-prose text-xs text-fg-subtle";
  details.dataset.toolCallId = call.id;
  const summary = document.createElement("summary");
  summary.className =
    "-mx-1.5 flex cursor-pointer select-none items-center gap-1 rounded-md px-1.5 py-0.5 hover:bg-surface-float";
  const { icon, label } = toolCallLabel(call);
  const labelEl = document.createElement("span");
  labelEl.className = "min-w-0 truncate";
  labelEl.textContent = label;
  const status = document.createElement("span");
  status.className = "flex shrink-0 items-center";
  summary.append(
    iconEl(icon, { size: 12, className: "text-fg-faint" }),
    labelEl,
    status,
    iconEl("chevron-right", {
      size: 12,
      className: "moe-thinking-chevron ml-auto text-fg-faint",
    }),
  );
  const body = document.createElement("div");
  body.className = "moe-thinking-body md mt-1.5 text-xs text-fg-muted";
  details.append(summary, body);
  const row: ToolRow = { details, label: labelEl, status, body };
  setToolRowStatus(row, call.status);
  if (call.result) body.innerHTML = markdown(call.result);
  return row;
}

/** The settled state a stored result implies (the runner's stable strings; see agent.rs/tools.rs). */
function toolResultStatus(result: string): ToolCallStatus {
  if (result === STOPPED_RESULT) return "stopped";
  if (result.startsWith(ERROR_RESULT_PREFIX)) return "error";
  return "ok";
}

/** Fold a stored `tool` result into its call's row (reopen path): the row settles with its result. */
function applyToolResult(row: ToolRow, result: string) {
  row.body.innerHTML = markdown(result);
  setToolRowStatus(row, toolResultStatus(result));
}

/**
 * Rebuild the activity stack in place (every frame of an agentic turn carries the full call list).
 * Each row's expand toggle is carried across the rebuild, keyed by call id (the MOE-0012 carry-over
 * idea); rows already re-rendered from the record (a page opened mid-turn) never duplicate here.
 */
function renderActivity(container: HTMLElement, activity: HTMLElement, calls: ToolCall[]) {
  const open = new Set<string>();
  for (const el of Array.from(
    activity.querySelectorAll<HTMLDetailsElement>("details[data-tool-call-id]"),
  )) {
    if (el.open && el.dataset.toolCallId) open.add(el.dataset.toolCallId);
  }
  const recorded = new Set<string>();
  for (const el of Array.from(container.querySelectorAll<HTMLElement>("[data-tool-call-id]"))) {
    if (!activity.contains(el) && el.dataset.toolCallId) recorded.add(el.dataset.toolCallId);
  }
  activity.replaceChildren(
    ...calls
      .filter((call) => !recorded.has(call.id))
      .map((call) => {
        const row = toolRowEl(call);
        row.details.open = open.has(call.id);
        return row.details;
      }),
  );
  // An empty stack takes no space: the stack sits above the answer only when it has rows.
  activity.classList.toggle("mb-2", activity.childElementCount > 0);
}

export function createConversationView(container: HTMLElement): ConversationView {
  /** The answer bubble being streamed into (created on the first event). */
  let streaming: StreamingBubble | null = null;
  let last: string | null = null;
  /** Whether an rAF correction is already queued (stream bursts must not queue one per frame). */
  let correcting = false;
  let lateCorrection: ReturnType<typeof setTimeout> | undefined;
  /** When reasoning first appeared: the settled label reports "Thought for N seconds" from it. */
  let thinkingStartedAt: number | null = null;
  /** Tool rows re-rendered from the record, by call id: `tool` messages fold their result in. */
  const recordRows = new Map<string, ToolRow>();

  function scrollToBottom() {
    container.scrollTop = container.scrollHeight;
  }

  function append(message: Message): HTMLElement {
    const row = document.createElement("div");
    if (message.role === "user") {
      row.className = "flex flex-col items-end gap-1";
      const card = document.createElement("div");
      card.className =
        "moe-bubble max-w-[85%] whitespace-pre-wrap rounded-2xl rounded-br-sm bg-bubble-user px-3 py-2 text-sm text-fg";
      card.textContent = message.content;
      row.append(card);
      if (message.attachments?.length) {
        const chips = document.createElement("div");
        chips.className = "flex max-w-[85%] flex-wrap justify-end gap-1";
        for (const reference of message.attachments) {
          const chip = document.createElement("span");
          chip.className =
            "moe-rim max-w-full truncate rounded-full bg-surface-float px-2 py-0.5 text-[11px] text-fg-muted";
          chip.textContent = `📎 ${reference.name}`;
          chip.title = reference.path;
          chips.append(chip);
        }
        row.append(chips);
      }
    } else if (message.role === "tool") {
      // A tool result folds into its call's row (reopened record): it never renders as a bubble of
      // its own. The record always stores the call before its results; an orphan is ignored.
      const callRow = message.toolCallId ? recordRows.get(message.toolCallId) : undefined;
      if (callRow) applyToolResult(callRow, message.content);
      return callRow?.details ?? document.createElement("div");
    } else {
      row.className = "md text-sm text-fg";
      // Reasoning + Tool Call rows re-render from the record (MOE-0026): Thinking first, then the
      // turn's calls in arrival order, above the round's answer text.
      if (message.reasoning?.trim()) {
        row.append(reasoningEl(message.reasoning, false));
      }
      if (message.toolCalls?.length) {
        const activity = document.createElement("div");
        activity.className = "mb-2 flex flex-col gap-0.5";
        for (const call of message.toolCalls) {
          const toolRow = toolRowEl(call);
          activity.append(toolRow.details);
          recordRows.set(call.id, toolRow);
        }
        row.append(activity);
      }
      const content = document.createElement("div");
      content.innerHTML = markdown(message.content);
      row.append(content);
      if (message.content.trim()) last = message.content;
    }
    container.append(row);
    return row;
  }

  function ensureStreamingBubble(): StreamingBubble {
    if (!streaming) {
      const root = document.createElement("div");
      root.className = "md text-sm text-fg";
      const { details, label, body: reasoningBody } = reasoningParts();
      details.classList.add("hidden");
      const activity = document.createElement("div");
      activity.className = "flex flex-col gap-0.5";
      const body = document.createElement("div");
      const indicator = generatingEl();
      root.append(details, activity, body, indicator);
      // Attach the bubble to the log: without this the whole answer (and its indicator) renders
      // into a detached node — persisted and streamed correctly, but never visible (MOE-0030 r10).
      container.append(root);
      streaming = { reasoning: details, reasoningLabel: label, reasoningBody, activity, body, indicator };
    }
    return streaming;
  }

  return {
    clear() {
      container.replaceChildren();
      streaming = null;
      last = null;
      thinkingStartedAt = null;
      recordRows.clear();
    },
    append,
    beginAnswer() {
      streaming = null;
      thinkingStartedAt = null;
    },
    updateStreaming(text, pending, reasoning = "", toolCalls) {
      const bubble = ensureStreamingBubble();
      const hasReasoning = reasoning.trim().length > 0;
      bubble.reasoning.classList.toggle("hidden", !hasReasoning);
      if (hasReasoning) {
        thinkingStartedAt ??= Date.now();
        const seconds = pending ? 0 : Math.round((Date.now() - thinkingStartedAt) / 1000);
        bubble.reasoningLabel.textContent = thinkingLabel(pending, seconds);
        bubble.reasoningLabel.classList.toggle("moe-thinking-live", pending);
        bubble.reasoningBody.innerHTML = markdown(reasoning);
      }
      if (toolCalls) renderActivity(container, bubble.activity, toolCalls);
      bubble.body.innerHTML = markdown(text);
      bubble.indicator.classList.toggle("hidden", !pending);
      if (text.trim()) last = text;
    },
    settleStreaming() {
      streaming?.indicator.classList.add("hidden");
      // A user-requested stop settles the row too: the label must not keep shimmering
      if (streaming && thinkingStartedAt !== null) {
        const seconds = Math.round((Date.now() - thinkingStartedAt) / 1000);
        streaming.reasoningLabel.textContent = thinkingLabel(false, seconds);
        streaming.reasoningLabel.classList.remove("moe-thinking-live");
      }
    },
    discardAnswer() {
      if (streaming && streaming.body.textContent.trim() === "") {
        streaming.body.parentElement?.remove();
        streaming = null;
        thinkingStartedAt = null;
      }
    },
    lastAnswer() {
      return last;
    },
    isEmpty() {
      return container.childElementCount === 0;
    },
    scrollToEnd() {
      // Pin immediately, then correct once after async layout (markdown/code/images expand, IIE4AD-369).
      // Stream frames call this in bursts: the rAF correction is deduped and the delayed one follows
      // the last frame, so the container is not thrashed three times per frame.
      scrollToBottom();
      if (!correcting) {
        correcting = true;
        requestAnimationFrame(() => {
          correcting = false;
          scrollToBottom();
        });
      }
      clearTimeout(lateCorrection);
      lateCorrection = setTimeout(scrollToBottom, 120);
    },
  };
}