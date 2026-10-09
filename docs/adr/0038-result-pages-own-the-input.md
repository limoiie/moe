# Result pages: the input is not their state, and Back leaves in one step

A result page (an AI Command's transform card, a record list) is a *page*, but two legacy habits made it
behave like a stack of layers. The search text that opened the command stayed in the Input Bar, so the
bar still belonged to the palette search while a page was up: typing re-ran the search and replaced the
page under a live stream (orphaning it), and `⌫` edited the search text instead of backing out — the
edit re-ran the search and landed on the search's one-row results, which read as "Back quit to a list
with one item". And Back consumed the full-screen detail first (`dismissDetail()`), exposing the single
result row as an intermediate list before the root — and that root still showed the stale search
sections from the way in. With the Quick Ask page asking before a Back kills a stream (ADR-0036), the
result pages needed the same dialect.

## Decision

- **Opening a page clears the Input Bar unless the row consumed the text.** Only `Query`-declaring rows
  (the capturing no-match ask) own the text; every other Apply and every entry (⌘P / ⌘N) clears it —
  the search text is never the page's state. From then on the bar is a plain search box at the root,
  which is what the placeholder and the empty-`⌫` layering already assume.
- **While a result streams, the page owns the input**: an input change does not re-run the palette
  search (it would replace the results layer and orphan the stream). The guard lifts when the stream
  settles; until then Enter / Esc / `⌫` belong to the page.
- **Back asks before stopping on every streaming surface.** The stop-confirmation dialog (⏎ Stop
  Generation / Esc Keep Generating) is one generic component; Enter stops directly and the pill's Stop
  row stops immediately, exactly as on the Quick Ask page (ADR-0036 amendment). An accidental Back must
  not silently kill a generation on any surface.
- **A result page backs out in one step, to the fresh root.** The full-screen detail *is* the page —
  the row list behind it is not a stop on the way back — and the parent is the root with an empty query
  (fresh Suggestions), never the stale search sections from the way in. `dismissDetail()` and
  `previewDismissed` are removed: after a full-screen detail there is no collapsed intermediate state
  to keep (ADR-0018's shapes are unchanged; only Back's path through them is).
- **Stream frames render on a coalesced cadence.** `ui/src/stream.ts` keeps the newest frame per key
  and applies at most every ~50 ms; the final frame is a frame like any other, so the settled answer
  always lands. `scrollToEnd` dedupes its corrections the same way. No React/TanStack dependency is
  pulled into the view layer (ADR-0007).

## Cost

- Leaving a result page loses the search that found the command (no row but the capture ever saw it as
  content); going back to a search means typing it again.
- A full-screen detail no longer has a "peek at the row" step; the row list on screen is the page
  behind a multi-result split view.
- Streamed text lands in ~20 updates a second rather than per token; that is above the reading
  threshold and below the delta rate, so rendering cost stays flat as answers grow.