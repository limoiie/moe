# M9u: the Side View's history card — decide whether `createCard` can host it

- **ID**: MOE-0049
- **State**: done
- **Labels**: wontfix, refactor, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Blocked by**: MOE-0041 (done)

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

## Decision: option 2 — the card stays hand-rolled, and ADR-0044's Cost is amended

This ticket made its own recommendation conditional — option 1 *"on the strength of `CardRow.id`"*, and
option 2 as "the honest fallback" if that id "cannot be justified on a second surface once the model
switcher is looked at properly — and ADR-0044's Cost should be amended rather than the field added
anyway". Looking at the model switcher settles it against option 1:

- **`CardRow.id` has no second consumer.** The switcher's rows are rebuilt per query (`onQuery`) and it
  marks the active model with `checked`, not by identity; nothing there would read an id.
- **The migration needs a capability the ticket did not foresee.** ⌃[ / ⌃] move the *highlight inside
  the card* (`moveHistory` → `conversations[historyIndex]`, `ui/src/chat.ts`), and `Card` can only
  *report* focus (`focused()`), not set it. Migrating therefore needs `CardRow.id` **plus a focus
  setter** — two new capabilities for one surface, where ADR-0044's Cost says a field earns its place
  "by being needed on at least two surfaces".
- `filteredEmptyText`, the third capability the body above lists, **already landed** — the Filter Card
  needed it in MOE-0041, so only the two above remain.

So ADR-0044's discipline holds as written, and the history card stays hand-rolled. What that costs is
named rather than hidden (ADR-0044's amendment records it): the Side View keeps its own Esc/empty-⌫ and
its own rows/sections rendering, which is the duplication ADR-0044 was written to end.

**The revisit trigger:** a second consumer for `CardRow.id`, or any card that needs programmatic focus.
Either makes option 1 correct again, and the migration would then be its own ticket carrying the three
capabilities and the behavioral criteria below.

## Acceptance

- [x] A decision recorded here, **and** ADR-0044 amended — to record that the history card stays
      hand-rolled and why, which is what that ADR's Cost clause prescribed for this outcome.
- [x] "If it migrates" criteria — not applicable; recorded here so the ticks are not read as having
      verified them.
- [x] `pnpm -C ui typecheck` and `just ui-build` green (no code changed).

## Comments

- 2026-10-12 (agent): split out of MOE-0041 when the migration was attempted. Labelled `needs-triage`
  because the choice is a design call about the shared component's boundary, not an implementation
  detail — and because ADR-0044 already anticipated the pressure this ticket is feeling.
- 2026-10-12 (agent): **decided — option 2**, on this ticket's own condition rather than against it.
  The recommendation was conditional on `CardRow.id` earning its place, and the model switcher does not
  read one (`checked` marks the active model; rows are rebuilt per `onQuery`). Reading the migration
  properly also turned up a capability the ticket missed: **⌃[/⌃] set the card's highlight**, and
  `Card` can only report focus — so option 1 would cost two single-surface capabilities, not one. The
  third it listed, `filteredEmptyText`, already shipped with the Filter Card in MOE-0041. ADR-0044 now
  carries the amendment its Cost clause asked for, with the revisit trigger named.