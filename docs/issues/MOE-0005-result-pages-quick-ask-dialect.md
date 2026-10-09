# M7m: result pages share the Quick Ask dialect — Back confirms, the page owns the input, Back leaves in one step

- **ID**: MOE-0005
- **State**: in-progress
- **Labels**: bug, ux
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent
- **Commit**: (pending)

User feedback after Quick Ask v2 (ADR-0036) and Enter-stops (MOE-0003): the AI Commands result pages
still behaved differently from the panel's conversation page, and Quick Ask had its own scroll bug and
unsmooth streaming.

## Requirement (user feedback)

1. During generation, Back (empty ⌫ / Esc) requests the user's confirmation — as the Quick Ask page does.
2. In the idle state, Back quits to the parent in one step. It currently landed on "a list containing
   exactly one ai-command-result action"; the user asked **why**. Two stacked causes:
   1. the search text that opened the command stayed in the Input Bar (`Selection`-declaring rows are
      invoked without a query but were never cleared), so ⌫ was a plain text edit: deleting one
      character re-ran the palette search (the input listener) and flipped the layer back to the
      command layer showing the search's one-row Results section — the AI command row looked like a
      "list containing exactly one ai-command-result action";
   2. even with an empty input, `back()` consumed the full-screen detail first (`dismissDetail()`),
      exposing the items layer's single result row, and only the next Back reached the command layer
      — which still showed the stale search sections from the way in.
3. Quick Ask did not auto scroll to end: `createConversationView` was given the inner log element, but
   the scroll container was `detailEl` (`CONVERSATION_PANE_CLASS` owns `overflow-y-auto`), so
   `scrollToEnd()` set `scrollTop` on a non-scrolling div.
4. The generation was not smooth: every SSE delta re-rendered the whole detail (marked parse +
   DOMPurify + `replaceChildren`) and thrashed the scroll container three times per frame.

## Decision (ADR-0038)

- **Back asks first on every streaming surface.** The stop-confirmation dialog is one generic
  component; Enter stops directly, the pill's Stop row is immediate, Back only asks.
- **A result page owns the Input Bar.** Applying a command whose row never consumed the text clears
  the Input Bar (the search text is never the page's state); while a result streams, the input is not
  a search box — typing does not re-run the palette search and orphan the stream.
- **A result page backs out in one step.** The full-screen detail *is* the page; the row list behind
  it is not a stop on the way back, and the parent is the root page (fresh, empty query) — never the
  stale search sections from the way in. `dismissDetail()` / `previewDismissed` are removed.
- **Stream frames render on a coalesced cadence** (`ui/src/stream.ts`): the newest frame per key,
  applied at most every ~50 ms; the DOM sees ~20 updates a second regardless of the model's rate.

## Delivery

- `ui/src/stream.ts` (new): `streamCoalescer`.
- `ui/src/main.ts`: generic `openConfirm(options)` + `openStopConfirm`; Back on a pending result asks;
  `applyCommand` / `openEntry` clear the bar when the row did not consume it; the input listener
  stands down while a result streams; one-step Back from result pages (`refresh("")`);
  `dismissDetail` / `previewDismissed` removed; the chat log becomes the scroll container.
- `ui/src/layout.ts`: `CONVERSATION_PANE_CLASS` becomes the column; the log inside scrolls.
- `ui/src/messages.ts`: `scrollToEnd` dedupes its corrections (one rAF + one trailing timer).
- `ui/src/chat.ts`: the Side View streams through the same coalescer.
- Docs: ADR-0038; ADR-0036 amendment addendum; CONTEXT (`Stop`); README (`Esc` / `⌫` rows, AI Commands
  and Quick Ask bullets).

## Acceptance

- AI Command streaming: ⌫ (empty input) or Esc pops "Stop generating?"; ⏎ in the dialog stops, Esc
  cancels; ⏎ alone still stops immediately.
- After an AI Command opens, the Input Bar is empty (the search text is gone); Back leaves the result
  page in one step to a fresh root.
- Quick Ask: sending scrolls to the bottom and keeps following the answer.
- Streaming: the answer updates at a steady cadence (no per-delta re-parse); completion still lands,
  the final frame included.
- `pnpm build` green.

## Comments

- 2026-10-09 (agent): filed from user feedback; implementing.