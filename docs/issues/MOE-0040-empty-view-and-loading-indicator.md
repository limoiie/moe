# M9l: the result list gets an Empty View and a loading bar — no more blank panel

- **ID**: MOE-0040
- **State**: todo
- **Labels**: ready-for-agent, bug, feature, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12

`render()` has no zero-length branch (`render`, `ui/src/main.ts`): both the commands and items paths
end at `listEl.replaceChildren(...lis)`, so a query that matches nothing shows a blank panel. Three
Extensions paper over it by returning a real `Item` as the empty answer (`notice_item`,
`ai.rs`; `no_match_item`, `ai_models.rs`; `ai_commands.rs`) — which
means the "no results" row is focusable and can be Applied. A search refresh shows nothing at all.
ADR-0043 adopts Raycast's `List.EmptyView` and its `isLoading` bar, and retires the notice item.

## Decision

- **Empty View declared on `ActionResult::List`** — title, description, optional icon, optional
  actions — rendered by the platform when there are no rows. **Non-focusable**: a focusable
  "no results" row is a row the user can Apply.
- **Raycast's suppression rule, verbatim:** never displayed while loading and the query is empty.
  That is what keeps a first paint from flashing "no results".
- **The notice item is retired.** Raycast has exactly one empty mechanism, and having two guarantees
  Extensions keep guessing which to use. Three call sites migrate; `no_match_item`'s guidance lives
  in `detail` (Markdown), which has no Empty View counterpart, so it moves into `description`.
- **Loading is a hairline progress bar under the Input Bar** — Raycast's placement — and **stale rows
  stay visible** until the new set lands. `.moe-generating`'s bouncing dots are **not** reused:
  search latency and token streaming are different states, and conflating them makes a 40ms debounce
  look like a generation.

## Delivery

- `crates/moe-core/src/contract.rs` — the declaration; `ui/src/types.ts` — the mirror.
- `ui/src/main.ts` — `render()`'s zero-length branch, and a loading flag on the `refresh()` path
  (`:2115-2125`, debounced at `:2478-2488`).
- `ui/src/styles.css` — the hairline.
- `crates/moe-extensions/src/ai.rs`, `ai_models.rs`, `ai_commands.rs` — the three migrations.
- `crates/moe-core/src/registry.rs` — the root's no-match path returns `Vec::new()` when no
  Extension captures; ADR-0043 gives the root a platform Empty View there too.

## Acceptance

- A query with no match and no capturing row shows the Empty View, not a blank panel — on the root
  page and on result pages alike.
- The Empty View never receives focus: ↑↓ and ⏎ cannot land on it, and the delete slots still count
  items only.
- Nothing flashes while loading with an empty query.
- All three notice-item sites render as Empty Views with equivalent copy, and no `actions` a user
  could reach is lost (audited per call site, not assumed).
- Stale rows remain visible during a refresh; the list is never blanked mid-typing.
- `just lint`, `just test`, `just ui-build` green.

## Comments

- 2026-10-12 (agent): filed from the Raycast primitives grilling session. Labelled `bug` as well as
  `feature` because the blank panel is a present defect, independent of the new mechanism. ADR-0043
  records the decision. No blocker: this is the cheapest high-value item in the set, and MOE-0043's
  Filter Control multiplies the ways to reach zero results, so it should land first.
