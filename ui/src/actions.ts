// The actions card (Show All Actions, ⌘K) is assembled from **sources**: one registration per
// surface, loaded per page — no call site merges rows by hand (ADR-0037).
//
// The card answers three questions:
//   1. Actions — what can run on the object the page is about: the focused item of a result page
//      (list / split / detail alike — the shape never changes the object) or the conversation of
//      a chat page;
//   2. Command — the page's command's own rows (ADR-0029: Open Command on the root, the favorite
//      toggle, Remove from Suggestions, Configure Extension);
//   3. General — the platform's general slots as they are live on this page (ADR-0014/0022:
//      Browse ⌘P / New ⌘N as the Extension declares them, Delete ⌃X / DeleteAll ⌃⇧X where the
//      page carries records).
// A surface registers its sources once; `sections(ctx)` then loads the right card for every page
// kind, and an empty source contributes nothing. Both windows use the pipeline (ADR-0028), each
// with its own landing spots for the shared semantics.
//
// **One canonical order** (ADR-0037 amendment): every row declares what it *is* (`rank`), never
// where it goes, and the plan sorts by that vocabulary — so the same action lands in the same slot
// on every page of every surface, whichever source declared it:
//
//   primary ⏎ → secondary ⌥⏎ → the object's / page's own actions → destructive (delete / delete
//   all) → the extension's settings → the app's global rows (Settings window, config file, window).
//
// The source sections stay: they order by their topmost row's slot (ties keep registration order),
// and inside a section the rows sort by rank — a hairline separates adjacent rank groups
// (`separator`), so the canonical order is visible without losing the sections' labels.

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

/** The canonical action order (ADR-0037 amendment) — the card's slots, top to bottom. */
export type ActionRank =
  /** The page's ⏎ action: Apply / Open Command, or Stop while a stream runs (ADR-0036). */
  | "primary"
  /** The page's ⌥⏎ action (copy). */
  | "secondary"
  /** The object's / page's own actions: New, Browse, Set AI Model, Open in Side View, re-run… */
  | "action"
  /** Irreversible record mutations: Delete ⌃X / DeleteAll ⌃⇧X. */
  | "destructive"
  /** The extension's own configuration. */
  | "settings"
  /** The app's escape hatches: the Settings window, the config file, window rows. */
  | "global";

/** A card row with its canonical slot; a row that declares nothing is a plain `action`. */
export interface ActionRow extends CardRow {
  rank?: ActionRank;
}

/** One source of the card: which pages it joins, and the rows it contributes there. */
export interface ActionSource {
  /** Section header; the section's canonical position comes from its topmost row (see `sections`). */
  title: string;
  /** Pages this source contributes to (content-based hiding: return no rows instead). */
  when(ctx: PageContext): boolean;
  /** The source's rows; empty contributes nothing. Async sources resolve declared entries (⌘P/⌘N). */
  rows(ctx: PageContext): ActionRow[] | Promise<ActionRow[]>;
}

/** The registered sources of one surface, loaded into the card. */
export function createActionPlan(sources: ActionSource[]) {
  return {
    /** The card's sections for this page: each source's rows sorted by rank (stable: the source's
     *  own order decides ties) with rank groups hairline-separated, and the sections themselves
     *  ordered by their topmost row's slot (ties keep registration order). */
    async sections(ctx: PageContext): Promise<CardSection[]> {
      const sections: { section: CardSection; slot: number }[] = [];
      for (const source of sources) {
        if (!source.when(ctx)) continue;
        const rows = sortByRank(await source.rows(ctx));
        if (rows.length === 0) continue;
        markGroups(rows);
        sections.push({ section: { title: source.title, rows }, slot: slotOf(rows[0]) });
      }
      return sections
        .map((entry, index) => ({ ...entry, index }))
        .sort((a, b) => a.slot - b.slot || a.index - b.index)
        .map(({ section }) => section);
    },
  };
}

/** The slots in order; the index is the sort key. */
const RANKS: ActionRank[] = ["primary", "secondary", "action", "destructive", "settings", "global"];

/** A row's slot index; an unknown/absent rank reads as `action` (the safe middle). */
function slotOf(row: ActionRow): number {
  const slot = RANKS.indexOf(row.rank ?? "action");
  return slot === -1 ? RANKS.indexOf("action") : slot;
}

/** Stable sort by slot: rows keep their declared order inside a rank. */
function sortByRank(rows: ActionRow[]): ActionRow[] {
  return rows
    .map((row, index) => ({ row, index }))
    .sort((a, b) => slotOf(a.row) - slotOf(b.row) || a.index - b.index)
    .map(({ row }) => row);
}

/** Marks the first row of every rank group after the first: the card draws a hairline above it. */
function markGroups(rows: ActionRow[]): void {
  let previous: number | null = null;
  for (const row of rows) {
    const slot = slotOf(row);
    if (previous !== null && slot !== previous) row.separator = true;
    previous = slot;
  }
}
