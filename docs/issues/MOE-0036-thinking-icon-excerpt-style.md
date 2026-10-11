# M9h: the Thinking row gets a lightning icon, and expanded non-prose content reads as an excerpt

- **ID**: MOE-0036
- **State**: done
- **Labels**: ux
- **Created**: 2026-10-11
- **Updated**: 2026-10-11
- **Assignee**: agent
- **Commit**: dc690e2

The owner's ask: "for the think block, add a lighting icon in the heading; for all non-prose
block's content, change to excerpt style — a left line aligned with the icon in the heading, and
the content aligned with the text in the heading."

## Decisions

- **Lightning icon**: the Thinking row's summary gains a leading `zap` (Lucide, 12 px,
  `text-fg-faint`) — the same leading-icon slot the Tool Call rows already have, so both
  non-prose row kinds share one anatomy: `[icon 12px][gap 4px][label]…[chevron]`.
- **Excerpt style** for every expanded non-prose body (`.moe-thinking-body` — the Thinking
  content and the Tool Call rows' results alike): a 2 px left rule in `--moe-line-strong`,
  hanging under the heading icon's center (5 px offset + 2 px border + 9 px padding = the 16 px
  where the heading text starts: icon 12 + gap 4). The old `pl-4` utility goes; the rule lives in
  CSS so both builders and every surface (panel result card, Quick Ask bubble, Side View, reopen
  re-render) inherit it from the one shared builder — the MOE-0012 invariant.
- **Prose is untouched**: the answer body keeps its plain `md` rendering; the excerpt rule marks
  exactly the foldable non-answer content.
- Row-level visuals otherwise unchanged: borderless muted rows, collapsed by default, chevron
  rotation, shimmer gated by `prefers-reduced-motion` (MOE-0012/0014/0026 all hold).

## Delivery

- `ui/src/icons.ts`: `zap` registered (Lucide `Zap`).
- `ui/src/messages.ts`: `reasoningParts` summary prepends the icon; both body class strings drop
  `pl-4`; builder comments updated.
- `ui/src/styles.css`: the `.moe-thinking-body` excerpt rule.

## Acceptance

- A Thinking row shows the lightning icon before "Thinking" / "Thought for N seconds" on all
  three surfaces (panel result card, Quick Ask, Side View).
- Expanding a Thinking row or a Tool Call row renders the content with a left rule vertically
  aligned under the heading icon, and the content's text left-edge aligned with the heading's
  label text.
- Answer prose renders unchanged; shimmer and collapse behaviors unchanged.
- `pnpm -C ui build` green.

## Comments

- 2026-10-11 (agent): filed from the owner's ask; implementing in the working tree.
- 2026-10-11 (agent): **done** (dc690e2) — `zap` registered; the Thinking summary leads with it
  (12 px, `text-fg-faint`, the Tool rows' slot); both body builders dropped `pl-4`; the excerpt
  rule lives on `.moe-thinking-body` (5 px offset + 2 px `--moe-line-strong` border + 9 px
  padding = the rule centered under the 12 px icon, text at the label's 16 px). `pnpm -C ui
  build` green. Eyeball check on next app run: expand a Thinking row and a `Searched «…»` row —
  the left rule should hang under the icon with the content flush to the label text.
