# M9q: metadata tags are pointer-only — decide the keyboard path

- **ID**: MOE-0045
- **State**: done
- **Labels**: wontfix, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Blocked by**: MOE-0048 (done)

ADR-0042 makes detail-metadata tags clickable, but only by pointer. There is no Keymap slot for
per-tag actions, and the Keymap is platform-owned and identical across Extensions (`CONTEXT.md` →
Keymap), so an Extension cannot add one either. That is divergence #8 in
`docs/research/raycast-ui-primitives.md`, and ADR-0042's Cost names it plainly: a small
keyboard-first regression in a keyboard-first product.

This issue is a **decision, not an implementation** — it needs a triage answer before anything can be
built, which is why it is `triage` / `needs-triage` while its siblings are `todo`.

## Options

1. **Do nothing.** Accept and keep the recorded divergence. Cheapest, and honest — the tags are a
   convenience over actions that are already reachable through ⌘K.
2. **A page-local navigation slot** — e.g. Tab / ⇧Tab moves focus into the metadata tag list and ⏎
   activates. No new global binding, but Tab currently has meaning in the Input Bar and the change
   touches ADR-0031's front-most-list rule.
3. **Surface the same choices as rows in the ⌘K card.** The card is already keyboard-complete —
   sections, filter, ↑↓/⏎, Esc — so this needs no new key handling at all, and it follows ADR-0037's
   "a card row can never do something the keyboard would not" principle in the direction that matters
   here. The tag's action would appear in the focused item's **Actions** section.
4. **Reserve a global binding.** Needs a Keymap change and its own ADR, and every plausible slot is
   either taken or an OS/edit convention (`SystemKey`, `crates/moe-core/src/keymap.rs`; ADR-0031
   deliberately leaves ⌃K to
   macOS kill-line).

## Decision: option 1 — accepted, because the tags add no capability the keyboard lacks

Chosen over this ticket's own recommendation (option 3), on evidence from the shipped code:

- **A tag's action is already a card row, by construction.** `tagIntent`'s action case only fires when
  `actionId` names one of the item's own `Action`s (`ui/src/metadata.ts`), and the ⌘K card's **Actions**
  section is `item.actions.map(…)` verbatim (`objectCardRows`, `ui/src/main.ts`). Option 3 would
  therefore add a row that duplicates one already on screen — the tag is a pointer *shortcut* to an
  action the card lists, not a second action.
- **A matching tag's value is already a Filter Card row.** The filter case fires only when the tag's
  text is in the page's `filterValues`, and those are exactly the values the Filter Card lists (plus
  its reset row). "Filter by ⟨label⟩" in the same card opens it.

So the divergence costs a *shortcut*, not a *capability*: the keyboard-first claim survives, and the
regression ADR-0042's Cost named is smaller than it looked — it is a mouse-only convenience over paths
that are already complete, which is the one shape of divergence this product can live with.

**The trigger to revisit:** if a tag ever gains an affordance that is *not* one of the item's actions —
a tag-level action of its own, or a target the card cannot express — this decision is void and the
question reopens. Written here so the next reader does not have to re-derive it.

This is recorded as ADR-0042's fourth amendment, and divergence #8's rationale is sharpened to match:
the tags stay pointer-only, and that is now a stated non-cost rather than an open regression.

## Acceptance

- [x] A decision recorded here **and** as an amendment to ADR-0042.
- [x] Not option 1's inverse: no follow-up issue is needed, because no mechanism is being built. The
      revisit trigger is named above instead.
- [x] Divergence #8 updated — not withdrawn (the tags *are* pointer-only), but its rationale now says
      why that costs nothing.

## Comments

- 2026-10-12 (agent): blocker updated from MOE-0044 to MOE-0048 — the tags this decides about are
  the `TagList` tags, and their rendering is MOE-0048's, not the metadata block's.
- 2026-10-12 (agent): filed from the Raycast primitives grilling session. Left as `needs-triage`
  deliberately: the four options have real trade-offs against ADR-0031 and ADR-0037 that are the
  owner's call, not something to settle inside an implementation issue.
- 2026-10-12 (agent): **decided — option 1, with a proof rather than a shrug.** MOE-0048 shipped the
  tags, which made the question answerable from code instead of from taste: both tag interactions
  resolve to affordances the ⌘K card already lists (`objectCardRows` over `item.actions`; the Filter
  Card over `filterValues`), so option 3's row would duplicate one already on screen. The regression is
  a shortcut, not a capability. Revisit trigger recorded above. This supersedes the recommendation the
  ticket was filed with, which was written before the tags existed.
