# One actions card in both windows; only the anchor differs

The panel's actions card (⌘K / ⌘⇧P, ADR-0015) and the Side View's More Actions menu (ADR-0014's
landing spot) are the same surface semantically — "the current context's actions" — but they had
drifted into two different things: a filterable, sectioned, keyboard-driven card on one side, and
a bare three-button menu without a filter or keyboard selection on the other.

## Decision

- Both surfaces are now **one component** (`ui/src/card.ts`, `createCard`) with one behavior set:
  rows grouped into Raycast-style sections (headers never focusable), a filter input, ↑↓ select,
  ⏎ run, `Esc` / empty `⌫` dismiss, click-outside closes, and Raycast's flatten-while-filtering.
  Each surface supplies its own rows and receives the same UI/UX in return.
- Only the **anchor** differs, and it is HTML structure, not behavior:

  | | Panel (`index.html`) | Side View (`chat.html`) |
  |---|---|---|
  | Position | bottom-right corner, above the action pill | horizontally centered, near the top (like the history card) |
  | Input edge | bottom (`border-t`) | top (`border-b`) |

- The panel card keeps its pill sync through the component's `onFocusChange` hook: the pill's
  primary action follows the highlighted row; `runFocused()` is the pill's click path.
- Rows: the panel's card shows the focused item's **Actions** plus the Extension-declared general
  entries under **General** (ADR-0014); the Side View's shows its local commands as **Chat** /
  **App** / **Window** (New Chat / Open Config File / Hide Side View).
- Closing returns focus to the window's own input bar (panel: Input Bar; Side View: composer), so
  the card stays one layer in the same layered-back order (`Esc` → card → … → window).

## Cost

- The Side View's More Actions is a full card now: three rows gain a filter they do not need and
  a header band above each group — accepted, identical UI/UX is the decision, not density.
- Dismissing the Side View card focuses the composer even when the clicked action pointed
  elsewhere (e.g. Hide Side View); harmless there, and consistent with the panel's return-to-input.
- The About card keeps its own renderer (ADR-0026: duplicated by intent); if it adopts
  `createCard`, that ADR's "wait for a third card" is fulfilled rather than violated.
## Amendment: the filter input sits at the bottom on every menu card (MOE-0030 revision)

The Side View's cards used to anchor their filter input at the top edge (the history card's
shape); the panel's cards kept it at the bottom. The model switcher's arrival made the mismatch
visible — two menu cards, two input edges, one window. The bottom edge wins everywhere: rows
first, filter last, on the panel's and the Side View's actions card, the history card, the about
card and the switcher card alike. The card component itself is order-agnostic (the owner's markup
places the input); the convention is this amendment. The switcher additionally filters
server-side (`createCard`'s `onQuery`): its rows are ranked and capped backend-side, so a client
title filter would hide exactly the capability matches the backend found — with `onQuery` the
card replaces its sections per query instead of filtering what it loaded.

## Amendment: the anchor table's input-edge row is superseded

The table above gives the Side View "Input edge: top (`border-b`)". The amendment below it already
reversed that — "The bottom edge wins everywhere … on the panel's and the Side View's actions card,
the history card, the about card and the switcher card alike" — and the DOM agrees with the
reversal: in `ui/chat.html` the `<ul id="action-list">` precedes the `<input id="action-search">`,
which carries `border-t`. The table row is therefore stale, and the stale claim had propagated into
that file's own comment (now fixed). Read the table's **Position** row — that one still holds, and it
remains the only difference between the two windows.

Worth recording because it is now load-bearing: **a card anchors near its trigger.** Each card sits
next to the control that opens it — the action pill's card bottom-right, the avatar chip's
bottom-left, the Side View toolbar's top-center. ADR-0043's Filter Card inherits the rule and anchors
top-right beneath the Filter Control: a fourth anchor for the same component, not a new component.
