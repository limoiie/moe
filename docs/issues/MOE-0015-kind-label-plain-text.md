# M7w: the row's kind label reads as plain text, not a chip

- **ID**: MOE-0015
- **State**: in-progress
- **Labels**: ux
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent

User feedback: "currently, the type of each item is shown as chip at the right end of each item row.
I want them to be shown as text directly."

## Delivery

- `ui/src/styles.css`: `.moe-kind-badge` loses the chip box — `padding`, the `--moe-line-strong`
  border, the 6 px radius and the inline-flex alignment — and keeps `flex: none`, the shared
  secondary-text token (13 px) and `--moe-fg-subtle`; the hover brightening to `--moe-fg-muted`
  stays. Position (trailing end of root command rows), content ("Command" / "AI Command") and the
  `badge` vocabulary in the view layer are unchanged.
- Docs: ADR-0030 amended (the fixed decision line + a new "kind label reads as plain text" section).

## Acceptance

- Root rows: the kind renders as plain 13 px subtle text at the row's end — no border, no
  background; hover brightens it exactly as before; the shortcut Kbd is the only boxed trailing
  element.
- `pnpm build` green.

## Comments

- 2026-10-09 (agent): filed from user feedback.
- 2026-10-09 (agent): delivered in the working tree (uncommitted; the user commits on main).
  `pnpm build` green. GUI check pending: double-tap ⌘ and read a root row's trailing end.