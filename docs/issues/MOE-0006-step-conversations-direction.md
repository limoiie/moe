# M7n: ⌃[ / ⌃] step the wrong way — `[` is Back, `]` is Forward

- **ID**: MOE-0006
- **State**: in-progress
- **Labels**: bug, ux
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent
- **Commit**: (pending)

User question: "is the behavior for ⌃[ and ⌃] mistaken? ⌃[ should be backward, and ⌃] should be
forward, right or not?" — Yes, it was inverted.

## Root cause

Both surfaces step by array index over `side_conversations`, whose
`ORDER BY updated_unix DESC` (pinned by `conversations_round_trip_and_order_by_recency`) means
**newest first**: index 0 is the newest conversation, higher indexes are older. Both call sites read
`step…(e.key === "]" ? 1 : -1)`, so `⌃]` moved to `index + 1` = an **older** conversation and `⌃[` to
`index - 1` = a **newer** one. From the newest conversation — the common state right after chatting —
`⌃[` (the macOS Back key) hit the clamp and did nothing, while `⌃]` walked into the past.

The unshifted `[` / `]` pair is macOS's Back/Forward (browsers, Finder, Xcode), and the README already
documents the pair as "previous / next conversation in the chat history" — previous = older, next =
newer. The list-order reading (`[` = up the visible list) would need the `⇧` variant (Slack's ⌘⇧[ /
⌘⇧] steps the sidebar list); the plain pair is Back/Forward.

## Delivery

- `ui/src/main.ts`: `stepChat(backward: boolean)` — `⌃[` steps to the previous (older) conversation
  (+1 in the newest-first array), `⌃]` to the newer one; a conversation outside the list (a blank new
  chat) sits at "now": backward lands on the newest, forward goes nowhere.
- `ui/src/chat.ts`: the same `stepConversation(backward: boolean)` for the Side View, one shared rule.
- Docs: README (`⌃[` / `⌃]` row + the Side View bullet — older / newer); ADR-0036's page-keys bullet
  reads "step backward / forward through `side_conversations`".

## Acceptance

- From the newest conversation, `⌃[` opens the previous (older) one; `⌃]` from there returns to the
  newer one; both clamp at the ends.
- Same behavior on the Quick Ask page and in the Side View; from a blank new chat `⌃[` lands on the
  newest conversation and `⌃]` stays.
- `pnpm build` green.

## Comments

- 2026-10-09 (agent): filed from the user's question; direction confirmed inverted against the macOS
  Back/Forward convention and the README's own "previous / next" wording. Implementing.