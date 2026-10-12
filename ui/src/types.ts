// Contract mirror: matches `moe-core`'s serde camelCase shapes one-to-one (ADR-0006).
// The command palette (index.html) and the Side View (chat.html) share this file,
// so the two windows never drift apart.

export type ActionKind = "primary" | "secondary";

/**
 * One trailing accessory on a result row (ADR-0041), mirroring `moe-core`'s internally-tagged
 * `Accessory`: `kind` picks the renderer. `color` is a semantic name from the bounded palette
 * (blue/green/magenta/orange/purple/red/yellow/primary/secondary); unknown names fall back, so it
 * stays a plain string rather than a union — the Rust side does not validate it into an enum, and a
 * union over an unvalidated wire value is a type that lies. `date.value` is unix seconds.
 */
export type Accessory =
  | { kind: "tag"; value: string; color?: string; tooltip?: string }
  | { kind: "text"; value: string; color?: string; tooltip?: string }
  | { kind: "date"; value: number; color?: string; tooltip?: string }
  | { kind: "icon"; value: string; color?: string; tooltip?: string };

export interface Action {
  id: string;
  title: string;
  kind: ActionKind;
  keybinding?: string | null;
}

/**
 * The platform's placeholder for a list with no rows (ADR-0043, ADR-0040's ticket): declared by the
 * Extension so the copy matches what the list is about, rendered non-focusable here.
 * Suppressed while a load is in flight on an empty query - Raycast's rule, which keeps a first
 * paint from flashing "no results".
 */
export interface EmptyView {
  title: string;
  /** One line saying why the list is empty. Prose, not Markdown. */
  description?: string;
  /** Semantic icon name (ADR-0012). */
  icon?: string;
  // No `actions`, though Raycast's EmptyView has one: Moe's only dispatch path is `run_item_action`,
  // which needs an Item. ADR-0043's amendment records the narrowing.
}

export interface Item {
  id: string;
  title: string;
  subtitle?: string;
  /** Icon semantic name (ADR-0012); falls back to the source Command's icon when absent. */
  icon?: string;
  /** The first action is the Apply semantics; ⌘K expands all actions. */
  actions: Action[];
  payload: unknown;
  /** Optional group label (ADR-0018 amendment): the list–detail template renders a non-focusable
   *  header before the first item of each group; navigation and actions count items only. */
  group?: string;
  /**
   * Values of this list's filter dimension (ADR-0043). Meaningless without a matching `filter` on the
   * result; omitted when the item carries none.
   */
  tags?: string[];
  /**
   * Trailing accessories (ADR-0041): at most three render, none while the detail pane shows (`split`),
   * and the title truncates before any is dropped. Omitted when the item carries none.
   */
  accessories?: Accessory[];
  /** Detail content (Markdown); the split page renders it in the right pane (ADR-0018). */
  detail?: string | null;
  /** Still producing (streaming placeholder): Esc prioritizes stopping the generation. */
  pending?: boolean;
}

/**
 * One filter dimension a list supports (ADR-0043). The Extension states which axis matters and what
 * the values are; the trigger, the Filter Card and the focus rules are the platform's.
 */
export interface Filter {
  /** Stable id within the Extension (the platform keys a remembered value by it). */
  id: string;
  /** The dimension's name: shown at rest in the Input Bar ("Tags") and in the reset row ("All Tags"). */
  label: string;
  /** Semantic icon name (ADR-0012); also the Filter Card's leading glyph. */
  icon?: string;
  /**
   * Explicit values in display order. Empty (or absent: the Rust side omits an empty vector) =
   * derived from the returned items' `tags`, which is why a declared list cannot drift from the tags
   * actually present.
   */
  values?: string[];
}

export interface CommandMeta {
  id: string;
  extensionId: string;
  /** The command name without the extension prefix (ADR-0030): rows show the extension name separately. */
  title: string;
  subtitle?: string;
  /** Icon semantic name (ADR-0012). */
  icon?: string;
  input: "none" | "query" | "selection";
  /** Live list: once entered, every input change re-runs the list (e.g. history search). */
  live: boolean;
  /** Invocation shortcut display string (e.g. "⌘,"), when the command declares one (ADR-0030). */
  keybinding?: string | null;
  /** The owning extension's display name, filled by the Registry (ADR-0030). */
  extensionTitle?: string | null;
  /** The kind of thing this row represents ("AI Command", later "File"…); absent renders as "Command". */
  kind?: string | null;
}

/** One group in the command palette results (ADR-0020): source = section, header = extension title. */
export interface CommandSection {
  title: string;
  items: CommandMeta[];
}

// Externally tagged enum: unit variants (silent) serialize to a bare string;
// payload variants like openSideView/writeBack become objects (the backend opens the Side View).
export type ActionResult =
  | string
  | { writeBack: { text: string } }
  | { rerun: { text: string } }
  | { list: { items: Item[]; detailFull?: boolean; empty?: EmptyView; filter?: Filter } }
  | { conversation: { conversationId: string | null } }
  | { openSideView: { payload: unknown } }
  | { openSettings: Record<string, never> };

/** Streaming increment: updates the item in place by item id. */
export interface CommandEventPayload {
  itemUpdated?: { commandId: string; item: Item };
}

// ---- AI conversations (`ai` Namespace) ----

export interface AttachmentRef {
  name: string;
  path: string;
}

// Tool Call row state (MOE-0026): "running" while the call executes, then one settled state.
export type ToolCallStatus = "running" | "ok" | "error" | "stopped";

/**
 * One Tool Call (MOE-0026). Stream frames carry the full view (`status`, and `result` once
 * settled); the stored record carries the call alone — its result lives in the following `tool`
 * message, and the row derives the settled status from it when the record re-renders.
 */
export interface ToolCall {
  id: string;
  name: string;
  arguments: string;
  status?: ToolCallStatus;
  result?: string;
}

export interface Message {
  role: "user" | "assistant" | "tool";
  content: string;
  attachments?: AttachmentRef[];
  /** The model's thinking (replayed to the model; the Thinking row re-renders it on reopen). */
  reasoning?: string;
  /** Assistant-round tool calls of an Agentic Conversation (absent when empty). */
  toolCalls?: ToolCall[];
  /** On `tool` messages: the call this result answers. */
  toolCallId?: string;
}

export interface Conversation {
  id: string;
  namespace: string;
  title: string;
  updatedUnix: number;
  /** Agentic Conversations advertise tools and replay their record (fixed at creation, ADR-0039). */
  agentic?: boolean;
}

/** Payload for opening the Side View with a conversation from the panel (⌘M) or the tray (empty = new conversation). */
export interface SideOpenPayload {
  conversationId?: string | null;
}
