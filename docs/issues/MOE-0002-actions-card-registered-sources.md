# M7j: the actions card is assembled from registered sources

- **ID**: MOE-0002
- **State**: done
- **Labels**: feature, ux
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent
- **Commit**: bc3ebd6

The ⌘K card was merged by hand at every call site: `openActionsCard` had one branch per layer
(command layer / chat page / result page) and the Side View kept a third hard-coded list, so "which
rows appear for this page" lived in three places. The general slots were also only half-listed:
Browse/New appeared on result pages but not on the command layer or the chat page, and Delete —
which ⌃X already fires on every record page — was never listed at all.

## Requirements (user feedback)

1. The card's content must follow the page:
   1. **list page** — all actions applicable to the focused item;
   2. **list-details page (split)** — all actions applicable to the focused item;
   3. **details page** — all actions applicable to the object the page represents.
2. The card must also carry the actions that can fire from the current page (sub-app level):
   Browse ⌘P, New ⌘N, Delete ⌃X, navigation ⌃N/⌃P, etc.
3. These actions must be registered as sections properly.
4. Refactor the registration so the card **loads automatically per page** instead of merging rows
   manually for each page and item kind.

## Design — ADR-0037

- `ui/src/actions.ts`: `ActionSource` (section title + `when(ctx)` + async `rows(ctx)`) and
  `createActionPlan(sources).sections(ctx)` — sources in registration order, empty sections dropped.
  `PageContext` = surface · page kind (`commands` / `items` / `chat`) · focused item · page command
  id · suggestion focus.
- The panel registers three sources: **Actions** (the object: the focused item on any result shape —
  the shape never changes the object — or the conversation on the chat page), **Command** (ADR-0029:
  Open Command on the root only, favorite, Remove from Suggestions, Configure placeholder),
  **General** (the platform slots live on the page: Browse ⌘P / New ⌘N resolved from declared
  entries; Delete ⌃X / DeleteAll ⌃⇧X on record pages and Remove Chat / Remove All Chats on the chat
  page; page-aware labels, keys from the keymap table).
- Navigation is a key, not a row (↑↓ / ⌃N ⌃P; the card owns them while open, ADR-0031).
- Landing spots stay in the surface (the panel's Browse opens the history list, the Side View's
  opens its history card); the Side View registers its own plan (Chat/App/Window) with the same
  mechanism, now including the page slots (Chat History ⌘P · New Chat ⌘N · Remove Conversation ⌃X /
  Remove All Conversations ⌃⇧X).

## Delivery

- `ui/src/actions.ts` (new): the plan + source types.
- `ui/src/main.ts`: the three panel sources, `panelPageContext`, `panelActionPlan`; `openActionsCard`
  is now a two-liner; `ActionRow` / `runActionRow` / `actionCardRow` / `iconOfActionRow` /
  `commandActionRows` removed; `openEntry` now prefers the focused command on the command layer
  (its stale `sourceCommandId` could hijack the new General rows there).
- `ui/src/chat.ts`: the Side View's card runs through the same plan; Chat section gains the page
  slots.
- Docs: ADR-0037; CONTEXT (`Show All Actions` now names the three sections); README (card bullet +
  AI Chat / Side View bullets).

## Acceptance

- Command layer: Command (Open Command / Favorite / Remove from Suggestions / Configure) + General
  (Browse Records / New Record, resolved for the focused command).
- Result pages (list / split / detail): Actions (the focused item's) + Command + General
  (+ Delete Record / Delete All Records).
- Chat page: Actions (Open in Side View / Copy Last Answer / Write Back) + Command + General
  (Browse Chats / New Chat / Remove Chat / Remove All Chats).
- Side View: Chat (Chat History / New Chat / Remove Conversation / Remove All Conversations) +
  App + Window.
- Empty sections hide; every row runs the same path as its key binding.
- `pnpm build` (tsc + vite) green.

## Comments

- 2026-10-09 (agent): filed and claimed; implemented in the working tree (UI-only change — no
  contract or Rust changes). `Commit` pending.
- 2026-10-09 (agent): delivered in `bc3ebd6` (`pnpm build` green; no Rust changes since
  MOE-0001's 146 tests); closing.