# M7v: the Thinking block's content — body-sized and indented one level

- **ID**: MOE-0014
- **State**: in-progress
- **Labels**: ux
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent
- **Commit**: (pending)

User feedback on the ChatGPT-style Thinking row (MOE-0012): the expanded reasoning content was still
`text-xs` (12 px) with no indent, so it read as a footnote rather than a nested body.

## Delivery

- `ui/src/messages.ts`: the shared `reasoningParts` body class becomes `text-sm` (the answer's size)
  with `pl-4` (16 px) so the row reads as a parent label with an indented body; the result card and
  both chat surfaces (panel Quick Ask, Side View) follow through the one builder.
- Docs: ADR-0024's MOE-0012 amendment (visual line updated).

## Acceptance

- Expanded reasoning: 14 px, muted, indented from the row's label; markdown structure (bullets, code
  blocks) respects the indent; the label row stays compact (`text-xs`).
- `pnpm build` green.

## Comments

- 2026-10-09 (agent): filed from user feedback; implementing.