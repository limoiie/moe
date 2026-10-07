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