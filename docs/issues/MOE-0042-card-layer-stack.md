# M9n: an explicit layer stack replaces the card if-chain

- **ID**: MOE-0042
- **State**: todo
- **Labels**: ready-for-human, refactor, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Blocked by**: MOE-0041
- **Blocks**: MOE-0043

Overlay precedence is a chain of early-returning if-statements maintained in three places at once:
the `cardOpen` guard `actionsCard.isOpen() || aboutCardOpen || modelSwitcher.isOpen()`
(`main.ts:2386`), the `back()` chain (`main.ts:2057-2113`), and a window-level Esc check
(`main.ts:2301-2307`). Adding ADR-0043's Filter Card means editing all three correctly, every time —
and getting one wrong is the class of bug IIE4AD-406 was (a card's Backspace reaching the window
handler and closing the card mid-edit).

## Decision

- **An ordered stack with a `top()`**, so `cardOpen` becomes derived and `back()` becomes a single
  pop. A new overlay registers once instead of being threaded through three conditionals.
- **Hard constraint — precedence above the cards is unchanged.** The stop-confirm dialog owns every
  key while open (`main.ts:2267-2277`), attach mode comes next (`:2290-2300`), and a focused
  `pending` item's stop-confirm sits above the cards in `back()` (`:2085-2088`). Those are not cards
  and do **not** join the stack; the stack orders cards only. ADR-0044 names this as the change's main
  regression risk and the reason it gets its own ADR and issue rather than riding along.
- **`createCard` keeps owning key discipline** — `stopPropagation` for every consumed key, empty-⌫
  forwarded as dismiss (`card.ts:282-290`) — so the stack never has to reason about which element saw
  a key first.

## Delivery

- `ui/src` — a small layers module (beside `card.ts`), then `main.ts`'s `keydown` handler and
  `back()`, and `chat.ts`'s equivalents.

## Acceptance

- Behavior is **identical** for every existing surface — this is a refactor, not a change. Verified
  by key, per surface, in both windows.
- Precedence holds: confirm dialog > attach mode > pending stop-confirm > cards > page.
- Empty-⌫ inside a card's filter still dismisses the card only, and never reaches the window-level
  "empty Backspace = Back" (IIE4AD-406 stays fixed).
- Adding a hypothetical seventh card requires one registration and no edit to `cardOpen` or `back()`.
- `just ui-build` green.

## Comments

- 2026-10-12 (agent): filed from the Raycast primitives grilling session; ADR-0044 records the
  decision. Labelled `ready-for-human` rather than `ready-for-agent`: it rewrites the panel's whole
  key-routing path, and the acceptance criteria are behavioral equivalences that need a person
  pressing keys across six surfaces to confirm. Blocked by MOE-0041 so the stack is built over
  unified cards rather than encoding two hand-rolled surfaces as special cases.
