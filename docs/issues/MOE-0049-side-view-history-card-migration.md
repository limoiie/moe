# M9u: the Side View's history card — decide whether `createCard` can host it

- **ID**: MOE-0049
- **State**: triage
- **Labels**: needs-triage, refactor, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Blocked by**: MOE-0041

The Side View's history card is the last hand-rolled card (`openHistoryCard`, `closeHistoryCard`,
`conversationRow`, `ui/src/chat.ts`). ADR-0044 decided it migrates onto `createCard`, and
MOE-0041 — which did the About card — was split to leave this half here, because the attempt showed
the migration needs capabilities only this surface would use.

This is a **decision, not an implementation**: it is the point at which ADR-0044's own discipline
either holds or gets amended, so it needs an answer before code.

## What blocks a straight migration

Attempting it produced two required additions, both single-surface:

- **`filteredEmptyText`.** The card shows "No matching conversations" when a filter excludes
  everything and "No chat history yet: type below to start a new conversation" when there is no
  history at all. `createCard` already distinguishes those two cases but hardcodes the filtered one as
  "No matching actions" — right for the actions card, wrong for conversations.
- **Row identity.** The Side View's ⌃[ / ⌃] stepping walks the `conversations` array by index to find
  the neighbour of the focused conversation. Once the card owns the focus, nothing maps a `CardRow`
  back to a conversation: the row has no id. This needs `CardRow.id` plus `Card.focusedId()` — or a
  different way to reach the focused conversation.

Also resolved already, by MOE-0041: `Card.refresh(sections?)` exists, so the card can re-render in
place when its data changes without reopening.

ADR-0044's Cost predicted precisely this: *"`CardRow`'s optional fields will keep growing; the
discipline is that a field earns its place by being needed on at least two surfaces."*

## Options

1. **Add both capabilities and migrate.** `CardRow.id` earns its place (the model switcher's rows are
   models with ids, and ADR-0043's Filter Card values are identified too — so it is ≥2 surfaces in
   waiting); `filteredEmptyText` is a string option rather than a behaviour flag. The gain is real:
   it removes the last duplicated copy of ADR-0031's input discipline, which that ADR's Cost warns
   "must be copied into every new list-like overlay".
2. **Keep the history card hand-rolled and amend ADR-0044** to say so, with this evidence. Cheapest,
   and honest — but it leaves two implementations of Esc/empty-⌫ and of the rows/sections rendering,
   which is what ADR-0044 was written to end.
3. **Extract the shared parts differently** — e.g. a lower-level row/section renderer that both the
   card and the history list use, with the history card keeping its own key handling. More design
   work, but avoids adding single-surface fields to `CardRow`.

## Recommendation

Option 1, on the strength of `CardRow.id`. `filteredEmptyText` is a string, not a behaviour, and the
id has real second consumers queued up. If `CardRow.id` cannot be justified on a second surface once
the model switcher is looked at properly, option 2 is the honest fallback — and ADR-0044's Cost should
be amended rather than the field added anyway.

## Delivery

Not yet specified — the answer decides it. If option 1: `CardRow.id` + `Card.focusedId()` +
`CardOptions.filteredEmptyText`, then `ui/src/chat.ts` loses its renderer, its focus tracking and its
Esc/empty-⌫ handler, and `ui/chat.html`'s history markup joins the shared shape.

## Acceptance

- [ ] A decision recorded here, and ADR-0044 amended either to record the new capabilities or to
      record that the history card stays hand-rolled.
- [ ] If it migrates: ⌃[ / ⌃] still step per the *filtered* list, the two empty messages stay
      distinct, and the recency groups still render as non-focusable headers.
- [ ] If it migrates: no hand-rolled Esc / empty-⌫ remains for it.
- [ ] Either way, `pnpm -C ui typecheck` and `just ui-build` green.

## Comments

- 2026-10-12 (agent): split out of MOE-0041 when the migration was attempted. Labelled `needs-triage`
  because the choice is a design call about the shared component's boundary, not an implementation
  detail — and because ADR-0044 already anticipated the pressure this ticket is feeling.