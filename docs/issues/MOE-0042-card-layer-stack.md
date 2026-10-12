# M9n: an explicit layer stack replaces the card if-chain

- **ID**: MOE-0042
- **State**: done
- **Labels**: ready-for-agent, refactor, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Blocked by**: MOE-0041
- **Blocks**: MOE-0043
- **Commit**: 6df9baa

Overlay precedence is a chain of early-returning if-statements maintained in three places at once:
the `cardOpen` guard `actionsCard.isOpen() || aboutCardOpen || modelSwitcher.isOpen()`
(`cardOpen`, `ui/src/main.ts`), the `back()` chain (`back()`, `ui/src/main.ts`), and a window-level
Esc check (`aboutCardOpen`, `ui/src/main.ts`). Adding ADR-0043's Filter Card means editing all three
correctly, every time —
and getting one wrong is the class of bug IIE4AD-406 was (a card's Backspace reaching the window
handler and closing the card mid-edit).

## Decision

- **An ordered stack with a `top()`**, so `cardOpen` becomes derived and `back()` becomes a single
  pop. A new overlay registers once instead of being threaded through three conditionals.
- **Hard constraint — precedence above the cards is unchanged.** The stop-confirm dialog owns every
  key while open, attach mode comes next, and a focused `pending` item's stop-confirm sits above the
  cards in `back()`. Those are not cards and do **not** join the stack; the stack orders cards only.
  ADR-0044 names this as the change's main regression risk and the reason it gets its own ADR and
  issue rather than riding along.
- **`createCard` keeps owning key discipline** — `stopPropagation` for every consumed key, empty-⌫
  forwarded as dismiss — so the stack never has to reason about which element saw a key first.

## Delivery

- `ui/src/layers.ts` — `createLayers()`: `register` / `top()` / `anyOpen()` / `closeTop()`. Pure
  logic, no DOM: registration order is front-most-LAST, which is also the order the anchors stack.
- `ui/src/main.ts` — the panel registers its actions card, about card and model switcher; `back()`
  is one `closeTop()`; the key guards are `anyOpen()`; the About-specific window Esc check is gone.
- `ui/src/chat.ts` — the Side View registers the same three (history card included, still
  hand-rolled — MOE-0049 decides whether it joins `createCard`; a Layer only needs `isOpen`/`close`).

## Acceptance

- [x] `back()` is one `closeTop()` in each window, and `cardOpen` is derived — no hand-maintained
      precedence chain survives in either key guard.
- [x] Precedence above the cards is untouched: confirm dialog → attach mode → pending stop-confirm →
      cards → page. Only the card section of `back()` was replaced.
- [x] Empty-⌫ inside a card's filter still dismisses the card only, and never reaches the
      window-level "empty Backspace = Back" — `createCard`'s `stopPropagation` plus the `anyOpen()`
      guard, neither of which changed.
- [x] Adding a card requires one registration and no edit to `back()` or the key guard.
- [x] `tsc --noEmit` and `tsc + vite build` green.
- [ ] **Behavioral equivalence not hand-verified.** The refactor is a refactor *except* for one
      deliberate fix (see the comment), and equivalence across two windows and six surfaces is a
      claim only key-pressing can settle. The Side View's close order is its old order by
      construction; the panel's gained the switcher.

## Comments

- 2026-10-12 (agent): filed from the Raycast primitives grilling session; ADR-0044 records the
  decision. Labelled `ready-for-human` rather than `ready-for-agent`: it rewrites the panel's whole
  key-routing path, and the acceptance criteria are behavioral equivalences that need a person
  pressing keys across six surfaces to confirm. Blocked by MOE-0041 so the stack is built over
  unified cards rather than encoding two hand-rolled surfaces as special cases.
- 2026-10-12 (agent): **done** (6df9baa). One drift found and fixed, which the if-chain had hidden:
  the Side View's `back()` closed the model switcher and the panel's did not, so in the panel Esc
  with the switcher open and the focus elsewhere backed out of the page instead of closing it, while
  the key guard still refused to let empty Backspace through. The stack closes it because it is
  registered — which is the whole argument for the change, demonstrated on the first day.
- 2026-10-12 (agent): **the click-outside handler was deliberately not moved.** ADR-0044 named three
  key-routing sites and click-outside is neither; each card is dismissed by a different owning
  element (the action pill for the actions card, the chip for About, the history button for
  history), which a uniform stack op would have to model as a predicate. Recorded so the acceptance
  criterion above ("one registration, no edit to `back()` or the key guard") stays literally true
  rather than quietly overreaching.
