# M9i: one content size for both non-prose blocks, and the tool results align in the excerpt

- **ID**: MOE-0037
- **State**: in-progress
- **Labels**: ux
- **Created**: 2026-10-11
- **Updated**: 2026-10-11
- **Assignee**: agent

The owner's observation after MOE-0036: "the font size for thinking block and tool calling block
aren't the same; and the content for tool calling block isn't aligned with the text of heading."

## Diagnosis

- **Size drift predates the excerpt**: the Thinking body renders `text-sm` (MOE-0014's decision —
  content is body-sized) while the Tool Call result body renders `text-xs` (MOE-0026). Stacked in
  one activity stack with one excerpt style, the mismatch reads as a bug.
- **The tool content's left edge is pushed in by markdown lists**: tool results render through
  `markdown()` — search hits arrive as ordered lists (`1. title / url / snippet`), and
  `.md ol, .md ul { padding-left: 1.2em }` indents the list *inside* the excerpt's 9 px padding,
  so the visible text starts right of the heading label. Thinking content is paragraphs, which
  carry no such indent — hence "only the tool block looks misaligned".

## Decision

- **One content size**: the Tool Call result body joins the Thinking body at `text-sm` —
  generalizing MOE-0014's rule ("content is body-sized") to every non-prose block. Colors stay
  as they are (the tool body keeps its extra-muted tone).
- **Lists align in the excerpt**: inside `.moe-thinking-body.md`, `ul`/`ol` lose the 1.2 em
  padding and switch to `list-style-position: inside`, so markers and text start flush at the
  excerpt's content edge (= the heading label's 16 px). The rule is scoped to the excerpt bodies
  and out-specifies `.md ol` — answer prose keeps its normal list indent. Code blocks (`pre`)
  stay inset cards; their left *edge* aligns, which is the correct look for a card.

## Delivery

- `ui/src/messages.ts`: the tool body's `text-xs` → `text-sm`.
- `ui/src/styles.css`: the scoped list rule under the excerpt block.

## Acceptance

- An expanded Thinking row and an expanded `Searched «…»` row render content at the same size.
- The search result list's markers/text start at the heading label's left edge; paragraphs
  (fetch/read results, thinking text) are unchanged.
- Answer-bubble markdown lists keep their 1.2 em indent (the rule never escapes the excerpt).
- `pnpm -C ui build` green.

## Comments

- 2026-10-11 (agent): filed from the owner's report; implementing in the working tree.
