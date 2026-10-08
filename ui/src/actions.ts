// The actions card (Show All Actions, ⌘K) is assembled from **sources**: one registration per
// section, loaded per page — no call site merges rows by hand (ADR-0037).
//
// The card answers three questions, in this order:
//   1. Actions — what can run on the object the page is about: the focused item of a result page
//      (list / split / detail alike — the shape never changes the object) or the conversation of
//      a chat page;
//   2. Command — the page's command's own rows (ADR-0029: Open Command on the root, the favorite
//      toggle, Remove from Suggestions, Configure Extension);
//   3. General — the platform's general slots as they are live on this page (ADR-0014/0022:
//      Browse ⌘P / New ⌘N as the Extension declares them, Delete ⌃X / DeleteAll ⌃⇧X where the
//      page carries records).
// A surface registers its sources once; `sections(ctx)` then loads the right card for every page
// kind, and an empty source hides its section. Both windows use the pipeline (ADR-0028), each with
// its own landing spots for the shared semantics.

import type { CardRow, CardSection } from "./card";
import type { Item } from "./types";

/** The page the card is being loaded for. */
export interface PageContext {
  /** Which window's card this is — each surface registers its own sources (ADR-0028). */
  surface: "panel" | "side";
  /** The page kind: the command layer, a result page (any shape), or a conversation page. */
  mode: "commands" | "items" | "chat";
  /** The object a result page is about: the focused item, whatever the shape (list / split / detail). */
  item?: Item;
  /** The command whose page this is: the focused command on the root, the page's command inside. */
  commandId?: string;
  /** The focused root row sits in Suggestions (⌃X forgets it instead of deleting a record, ADR-0025). */
  suggestion?: boolean;
}

/** One section of the card: which pages it joins, and the rows it contributes there. */
export interface ActionSource {
  /** Section header; the registration order is the card's section order. */
  title: string;
  /** Pages this source contributes to (content-based hiding: return no rows instead). */
  when(ctx: PageContext): boolean;
  /** The section's rows; empty hides the section. Async sources resolve declared entries (⌘P/⌘N). */
  rows(ctx: PageContext): CardRow[] | Promise<CardRow[]>;
}

/** The registered sources of one surface, loaded into the card. */
export function createActionPlan(sources: ActionSource[]) {
  return {
    /** The card's sections for this page: sources in registration order, empty sections dropped. */
    async sections(ctx: PageContext): Promise<CardSection[]> {
      const sections: CardSection[] = [];
      for (const source of sources) {
        if (!source.when(ctx)) continue;
        const rows = await source.rows(ctx);
        if (rows.length > 0) sections.push({ title: source.title, rows });
      }
      return sections;
    },
  };
}