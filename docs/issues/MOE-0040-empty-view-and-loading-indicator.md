# M9l: the result list gets an Empty View and a loading bar — no more blank panel

- **ID**: MOE-0040
- **State**: done
- **Labels**: ready-for-agent, bug, feature, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Commit**: 39596c8

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

- [x] A query with no match and no capturing row shows the Empty View, not a blank panel — on the
      root page and on result pages alike.
- [x] The Empty View never receives focus: it is a `<li>` with no click handler and no
      `data-focused`, so ↑↓ and ⏎ cannot land on it.
- [x] Nothing flashes while loading with an empty query (Raycast's suppression rule).
- [x] The notice-item sites that are *empty states* render as Empty Views with equivalent copy:
      `ai.rs`'s two history cases and `ai_models.rs`'s no-match picker — **two sites, not three**;
      `ai_commands`' seven notice items are content cards and are exempt (see the comment below and
      ADR-0043's amendment).
- [x] Stale rows remain visible during a refresh; the list is never blanked mid-typing.
- [x] `just test` (250 tests) and `just ui-build` green. **`just lint` is red on a pre-existing
      `cargo fmt` diff in `crates/moe-extensions/src/ai_client/anthropic.rs`, present on a clean
      `HEAD` and untouched by this work.**

## Comments

- 2026-10-12 (agent): filed from the Raycast primitives grilling session. Labelled `bug` as well as
  `feature` because the blank panel is a present defect, independent of the new mechanism. ADR-0043
  records the decision. No blocker: this is the cheapest high-value item in the set, and MOE-0043's
  Filter Control multiplies the ways to reach zero results, so it should land first.
- 2026-10-12 (agent): **done** (39596c8). `ActionResult::List` carries an optional `EmptyView`;
  `render()` shows it for the zero-row case on both layers; the loading hairline is wired into
  `refresh()` and `rerunLive()`; the two empty-state notice sites migrated.

  Two corrections found while implementing, both recorded in ADR-0043 by amendment:

  **The migration is two sites, not three.** All seven `ai_commands` notice items go through
  `ActionResult::detail(...)` — the full-detail shape — carrying multi-paragraph Markdown and a
  fenced error block. They are content cards: the list has one item, so an Empty View would never
  trigger, and a one-string `description` could not hold the prose. They keep their cards. The real
  migrations were the sites that returned a **one-item list whose single item was a notice** —
  `ai.rs`'s zero-row and read-failure history cases, and `ai_models.rs`'s no-match picker — because
  that is what made "no results" focusable.

  **There is no `actions` field**, though Raycast's `EmptyView` has one. Moe's only dispatch path is
  `run_item_action`, which needs an `Item`, and an empty list has none; a field with no way to run it
  would be dead API. Both migrated sites declared zero actions, so nothing is lost.

  One find worth keeping: the palette's own no-match state needed no Extension change at all.
  `Registry::search` already returns an empty `Vec` when nothing matches and nothing captures, so the
  platform supplies the copy — which makes the root page the Empty View's first real consumer, ahead
  of any Extension that declares one.
