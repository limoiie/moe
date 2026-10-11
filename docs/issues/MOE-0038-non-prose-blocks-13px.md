# M9j: the non-prose blocks settle at 13px — heading and content alike

- **ID**: MOE-0038
- **State**: in-progress
- **Labels**: ux
- **Created**: 2026-10-11
- **Updated**: 2026-10-11
- **Assignee**: agent

The owner's ask: "change the font size of the blocks to 13px, including both the heading and
content" — the Thinking rows and Tool Call rows, on every surface the shared builders serve
(panel result card, Quick Ask, Side View, reopen re-render).

## Decision

- **One explicit size for the whole block**: `.moe-thinking { font-size: 13px }` in CSS; the
  builders drop their Tailwind size utilities (`text-xs` on the rows, `text-sm` on the bodies) so
  heading label, status word, and excerpt content all inherit the same 13px. `.md` child rules
  are em-relative (`h2` 1.05em, `code` 0.9em), so markdown inside the excerpt scales with it.
  Icons keep their 12px attribute size (glyphs, not text); the answer prose keeps its own
  `text-sm` (untouched).
- **Supersedions, surfaced per domain.md**: this replaces MOE-0014's "content is body-sized
  (`text-sm`)" decision and the same detail inside ADR-0024's MOE-0012 增补 — heading and
  content are now equal at 13px, halfway between the old 12/14 split. MOE-0037's "one content
  size for both blocks" invariant is preserved (both are 13px); the excerpt alignment is
  unaffected (it is margin/border/padding geometry, not font metrics).

## Delivery

- `ui/src/messages.ts`: the two `details` class strings lose `text-xs`; the two body class
  strings lose `text-sm`.
- `ui/src/styles.css`: `font-size: 13px` on `.moe-thinking`.

## Acceptance

- Computed font size is 13px for: the Thinking label ("Thinking" / "Thought for N seconds"), the
  Tool Call row label + status word, and both expanded bodies — identical across the panel result
  card, Quick Ask, and the Side View.
- Answer bubbles/prose still render at their own size; shimmer, chevron, collapse, and the
  excerpt rule/marker alignment unchanged.
- `pnpm -C ui build` green.

## Comments

- 2026-10-11 (agent): filed from the owner's ask; implementing in the working tree.
