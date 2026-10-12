# M9m: the About card moves onto the shared card component

- **ID**: MOE-0041
- **State**: done
- **Labels**: ready-for-agent, refactor, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Blocks**: MOE-0042, MOE-0043, MOE-0049
- **Commit**: eef09d4

Five card surfaces exist. Three use `createCard` — the panel's actions card (`actionsCard`,
`ui/src/main.ts`), the Side View's More Actions card (`actionsCard`, `ui/src/chat.ts`), and the model
switcher (`ensureCard`, `ui/src/models.ts`). Two were hand-rolled; this ticket took the **About card**,
which had its own row builder, renderer, filter, focus tracking and key handler (`aboutRowEl`,
`renderAboutCard`, `ui/src/main.ts`). ADR-0028 already said adopting `createCard` would "fulfil rather
than violate" ADR-0026's deferral.

> **Scope trimmed 2026-10-12:** the Side View's history card was the other half of this ticket and
> is now **MOE-0049**, because attempting it showed it needs capabilities only it would use. See the
> comments.

## Decision

ADR-0044 holds the reasoning. Two capabilities were needed, and both are named there:

- **`CardRow.keepOpen`** — the theme rows must stay open so a second theme can be tried immediately
  (ADR-0035 amendment). Row-level by intent: it is the *row* that knows ("System / Light / Dark stay
  open"; "Open Config File" closes), and a card-level predicate would push that knowledge back into
  the owner — the coupling `createCard` exists to remove.
- **`Card.refresh(sections?)`** — re-render in place, optionally with fresh sections, keeping the
  filter text and the focused row. The old hand-rolled renderer preserved both, so reopening the card
  would have been a visible regression.

## Delivery

- `ui/src/card.ts` — `keepOpen` on `CardRow`, `refresh()` on `Card`.
- `ui/src/main.ts` — one `aboutCard = createCard(...)` instance; `aboutSections()` returns
  `CardSection[]`; the About renderer, its focus tracking, its two input listeners and
  `cardSectionHeaderEl` are gone. No markup change: the About card was already built to the shared
  shape (card div > `ul` + `input` with `border-t`), and its input was already at the bottom edge.

## Acceptance

- [x] About: choosing System / Light / Dark keeps the card open and its check follows the new
      preference; every other About row closes it.
- [x] The About card's filter, ↑↓/⏎, Esc / empty-⌫ and click-outside all come from `createCard` —
      no hand-rolled handling remains for it.
- [x] `aboutCardOpen`'s four readers now ask `aboutCard.isOpen()`. The window-level Esc guard and the
      click-outside listener stay in `main.ts` until MOE-0042's layer stack takes them, which is why
      this ticket does not claim the panel's key routing is finished.
- [x] `tsc --noEmit` and `tsc + vite build` green; the bundle drops 37.85 kB → 35.66 kB.
- [x] **"No behavior change observable to a user"** — verified for the chip, the filter and Esc: the
      reporter's passes exercised the About chip's material (referenced directly as the visual
      reference for the Filter Control), the Filter Card end to end, and its Esc ordering through the
      shared card. **Not covered by any pass:** the About card's theme rows, which nobody has toggled
      since the migration.
- [x] `CardRow` gained no field that only one surface uses... except `keepOpen`, which is About-only
      today. ADR-0044 decided it explicitly and named "Filter-adjacent multi-select" as the second
      expected consumer; recorded here rather than hidden.

## Comments

- 2026-10-12 (agent): filed from the Raycast primitives grilling session; ADR-0044 records the
  decision and amends ADR-0026's deferral. Sequenced before MOE-0042 (the layer stack should order
  unified cards, not special cases) and MOE-0043 (the Filter Card is built on `createCard`, not as a
  sixth hand-rolled surface).
- 2026-10-12 (agent): **done** (eef09d4) for the About half. `main.ts` −168 net lines.
- 2026-10-12 (agent): **the history half moved to MOE-0049.** Migrating it revealed it needs two more
  capabilities that only it would use: `filteredEmptyText` (its filtered-empty copy is "No matching
  conversations", where `createCard` hardcodes "No matching actions") and row identity (`CardRow.id`
  + `Card.focusedId()`), because the Side View's ⌃[ / ⌃] stepping walks `conversations` by index and a
  `CardRow` carries no id to map back through. That is exactly the field growth ADR-0044's Cost
  warned about — "a field earns its place by being needed on at least two surfaces" — so the right
  move was to stop and put the choice in its own ticket rather than grow the API unilaterally.