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
   * dot animation is never restarted.
   */
  updateStreaming(text: string, pending: boolean): void;
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
  body: HTMLElement;
  indicator: HTMLElement;
}

export function createConversationView(container: HTMLElement): ConversationView {
  /** The answer bubble being streamed into (created on the first event). */
  let streaming: StreamingBubble | null = null;
  let last: string | null = null;

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
      const body = document.createElement("div");
      const indicator = generatingEl();
      root.append(body, indicator);
      container.append(root);
      streaming = { body, indicator };
    }
    return streaming;
  }

  return {
    clear() {
      container.replaceChildren();
      streaming = null;
      last = null;
    },
    append,
    beginAnswer() {
      streaming = null;
    },
    updateStreaming(text, pending) {
      const bubble = ensureStreamingBubble();
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
      scrollToBottom();
      requestAnimationFrame(scrollToBottom);
      setTimeout(scrollToBottom, 120);
    },
  };
}