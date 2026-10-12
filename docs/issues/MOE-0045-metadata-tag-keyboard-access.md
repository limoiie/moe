# M9q: metadata tags are pointer-only — decide the keyboard path

- **ID**: MOE-0045
- **State**: triage
- **Labels**: needs-triage, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Blocked by**: MOE-0044

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
   either taken or an OS/edit convention (`keymap.rs:57-80`; ADR-0031 deliberately leaves ⌃K to
   macOS kill-line).

## Recommendation

Option 3, with the pointer affordance kept. It costs one `ActionSource` row builder, adds no key
handling, no Keymap change and no ADR, and it makes the tag's action discoverable in the one place
every other action already is. Options 2 and 4 both spend platform-wide keyboard surface on a
per-item affordance, which is the trade ADR-0043 already declined once for the Filter Control.

## Acceptance

- A decision recorded here or as an amendment to ADR-0042.
- If the answer is not option 1, a follow-up issue with the chosen mechanism, its files, and its own
  acceptance criteria.
- If the answer is option 3, divergence #8 in `docs/research/raycast-ui-primitives.md` is updated —
  the tags stay pointer-only, but their actions stop being pointer-only, which is the part that
  actually mattered.

## Comments

- 2026-10-12 (agent): filed from the Raycast primitives grilling session. Left as `needs-triage`
  deliberately: the four options have real trade-offs against ADR-0031 and ADR-0037 that are the
  owner's call, not something to settle inside an implementation issue.
