// Contract mirror: matches `moe-core`'s serde camelCase shapes one-to-one (ADR-0006).
// The command palette (index.html) and the Side View (chat.html) share this file,
// so the two windows never drift apart.

export type ActionKind = "primary" | "secondary";

export interface Action {
  id: string;
  title: string;
  kind: ActionKind;
  keybinding?: string | null;
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
  /** Detail content (Markdown); the split page renders it in the right pane (ADR-0018). */
  detail?: string | null;
  /** Still producing (streaming placeholder): Esc prioritizes stopping the generation. */
  pending?: boolean;
}

export interface CommandMeta {
  id: string;
  extensionId: string;
  title: string;
  subtitle?: string;
  /** Icon semantic name (ADR-0012). */
  icon?: string;
  input: "none" | "query" | "selection";
  /** Live list: once entered, every input change re-runs the list (e.g. history search). */
  live: boolean;
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
  | { list: { items: Item[]; detailFull?: boolean } }
  | { openSideView: { payload: unknown } };

/** Streaming increment: updates the item in place by item id. */
export interface CommandEventPayload {
  itemUpdated?: { commandId: string; item: Item };
}

// ---- AI conversations (`ai` Namespace) ----

export interface AttachmentRef {
  name: string;
  path: string;
}

export interface Message {
  role: "user" | "assistant";
  content: string;
  attachments?: AttachmentRef[];
}

export interface Conversation {
  id: string;
  namespace: string;
  title: string;
  updatedUnix: number;
}

/** Payload for opening the Side View with a conversation from the panel (⌘M) or the tray (empty = new conversation). */
export interface SideOpenPayload {
  conversationId?: string | null;
}
