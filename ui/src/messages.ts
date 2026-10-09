// The conversation message stream — user bubbles + Markdown answers (streamed in place) — shared by
// the Command Panel's Quick Ask page and the Side View, so the two surfaces cannot drift apart.
// The owner supplies the scroll container; this module renders into it and tracks the last answer
// (the copy / write-back target).

import DOMPurify from "dompurify";
import { marked } from "marked";
import { generatingEl } from "./generating";
import type { Message } from "./types";

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
   * dot animation is never restarted. `reasoning` (models that think in `…` blocks) renders in a
   * collapsible Thinking block above the answer — it is never part of the body (MOE-0008).
   */
  updateStreaming(text: string, pending: boolean, reasoning?: string): void;
  /** Hide the generation indicator without touching the text (a user-requested stop); later frames still land here. */
  settleStreaming(): void;
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
  body: HTMLElement;
  indicator: HTMLElement;
}

/**
 * The Thinking block's parts (MOE-0008): a muted collapsible block above the answer — one shared
 * builder for the chat bubbles here and the panel's result cards (`reasoningEl`).
 */
function reasoningParts(): {
  details: HTMLDetailsElement;
  label: HTMLElement;
  body: HTMLElement;
} {
  const details = document.createElement("details");
  details.className =
    "not-prose mb-2 rounded-xl border border-line bg-surface-float px-3 py-2 text-xs text-fg-subtle";
  const label = document.createElement("summary");
  label.className = "cursor-pointer select-none";
  const body = document.createElement("div");
  body.className = "mt-1.5 max-h-64 overflow-y-auto whitespace-pre-wrap leading-relaxed";
  details.append(label, body);
  return { details, label, body };
}

/** A settled Thinking block (open while `pending`): the panel's result card renders this directly. */
export function reasoningEl(reasoning: string, pending: boolean): HTMLElement {
  const { details, label, body } = reasoningParts();
  label.textContent = pending ? "Thinking…" : "Thinking";
  body.textContent = reasoning;
  details.open = pending;
  return details;
}

export function createConversationView(container: HTMLElement): ConversationView {
  /** The answer bubble being streamed into (created on the first event). */
  let streaming: StreamingBubble | null = null;
  let last: string | null = null;
  /** Whether an rAF correction is already queued (stream bursts must not queue one per frame). */
  let correcting = false;
  let lateCorrection: ReturnType<typeof setTimeout> | undefined;
  /** The previous frame's pending flag: a settled answer folds the Thinking block once. */
  let lastPending = false;

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
    } else {
      row.className = "md text-sm text-fg";
      row.innerHTML = markdown(message.content);
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
      const body = document.createElement("div");
      const indicator = generatingEl();
      root.append(details, body, indicator);
      container.append(root);
      streaming = { reasoning: details, reasoningLabel: label, reasoningBody, body, indicator };
    }
    return streaming;
  }

  return {
    clear() {
      container.replaceChildren();
      streaming = null;
      last = null;
      lastPending = false;
    },
    append,
    beginAnswer() {
      streaming = null;
      lastPending = false;
    },
    updateStreaming(text, pending, reasoning = "") {
      const bubble = ensureStreamingBubble();
      const hasReasoning = reasoning.trim().length > 0;
      bubble.reasoning.classList.toggle("hidden", !hasReasoning);
      if (hasReasoning) {
        bubble.reasoningLabel.textContent = pending ? "Thinking…" : "Thinking";
        bubble.reasoningBody.textContent = reasoning;
        // Watch the thinking live; fold it when the answer settles (the user can reopen it)
        if (pending) bubble.reasoning.open = true;
        else if (lastPending) bubble.reasoning.open = false;
      }
      lastPending = pending;
      bubble.body.innerHTML = markdown(text);
      bubble.indicator.classList.toggle("hidden", !pending);
      if (text.trim()) last = text;
    },
    settleStreaming() {
      streaming?.indicator.classList.add("hidden");
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