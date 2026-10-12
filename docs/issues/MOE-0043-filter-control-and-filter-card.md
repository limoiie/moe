# M9o: the Filter Control narrows a result list by tags

- **ID**: MOE-0043
- **State**: done
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Blocked by**: MOE-0041, MOE-0042
- **Blocks**: MOE-0046, MOE-0048
- **Commit**: 7a500d1 (712ca26, 5c304e4, e479809, 02bddd0 post-close)

The tracer bullet. There is no way to narrow a list by a second dimension: `Item` has no tags and
`ActionResult::List` carries only `items` and `detail_full`. ADR-0043 adopts Raycast's
`List.Dropdown` search-bar accessory as a **Filter Control** in the Input Bar plus a **Filter Card**,
with filtering evaluated by the platform.

This ticket delivers the visible path end to end. The correctness rules that surround it — values
that must not vanish while typing, a frozen filter while a stream runs, session-scoped state — are
**MOE-0046**, not here.

## Decision

ADR-0043 holds the reasoning. The load-bearing points:

- **`Item.tags` plus a dimension declaration** on `ActionResult::List` (id, label, optional icon,
  optional explicit value list). Values are **derived from the returned items' tags** when no list is
  given, so a declared list cannot drift from the tags actually present. Filtering is client-side;
  server-side (a filter argument on `Extension::invoke`) is the named fallback if pagination lands.
  `payload` was never an option — it is documented as opaque to the UI.
- **A platform-supplied "All ⟨label⟩" row resets**, and the active value carries a check via the
  existing `CardRow.checked` (`CardRow`, `ui/src/card.ts`) — no new row capability. No sections in
  v1: derived values have no natural grouping.
- **The control is `#query-row`'s third child** (`#query-row`, `ui/index.html`), which has two today,
  rendering **label + chevron** — "Tags" at rest, "Tags: Agentic" when set, truncated — with a right
  inset mirroring the row's `px-5`.
- **It must not change the Input Bar's height.** Panel height is measured from the real bar
  (`measuredPanelHeight`, `ui/src/main.ts`) but `resize_panel` fires only at startup and on a shape
  change — `applyPanelSize` early-returns, and there is no `ResizeObserver`. A control that grew the
  row mid-session would silently clip the ten-row grid (ADR-0026) with no error.
- **Result pages only** — `list` and `split`. Not `detail` (nothing to narrow), not the command layer
  (the root groups by Extension source per ADR-0020, where a tag axis has no meaning).
- **No global shortcut.** Raycast binds ⌘P here; Moe's ⌘P is `Browse` (`Browse`,
  `crates/moe-core/src/keymap.rs`) and toggles the history card in the Side View (`toggleHistoryCard`,
  `ui/src/chat.ts`). Keyboard access comes from a **"Filter by Tag" row in the ⌘K card** — Raycast's
  own `ActionPanel.Submenu` pattern — plus click and Tab. The retired `⌘⇧P` is **not** revived
  (ADR-0015 amendment).
- **The Filter Card anchors top-right under its trigger**, per the now-explicit rule that a card
  anchors near its trigger (ADR-0028 amendment). Built on `createCard` and registered in the layer
  stack.
- **AND with the query** in the basic case: the query re-runs the command (Live List), the tag narrows
  what comes back.
- **First real consumer:** `Conversation.agentic` exists in storage but never reaches the `Item`
  (`Conversation`, `crates/moe-core/src/conversation.rs`) — chat history is the natural first
  dimension, and it is what makes this ticket demoable.

## Delivery

A vertical slice: Rust contract → the shared contract mirror → the control and card → the ⌘K row →
one Extension that declares a dimension.

- `crates/moe-core/src/contract.rs` — `Item.tags` and the dimension declaration on
  `ActionResult::List`; the shared mirror in `ui/src/types.ts`.
- `ui/index.html`, `ui/src/main.ts` — the control as the input row's third child, and the card.
- `ui/src/actions.ts` — a "Filter by Tag" row for pages that declare a dimension.
- `ui/src/styles.css`.
- The first consumer in `crates/moe-extensions/`.

## Acceptance

- [x] The control renders only on `list` / `split` result pages whose result declares a dimension —
      one predicate, `filterableDimension()`, gates mode AND page shape AND the declaration, so the
      control, the card and the ⌘K row cannot disagree. **The height claim is arithmetic, not a
      measurement:** the row is sized by the input's line box and its own padding, and the control is
      21px inside a ~27px-tall content box, so it cannot grow the row. Nobody has measured it.
- [x] Choosing a value narrows the list; the label shows the active value; "All ⟨label⟩" restores
      everything; the active value carries a check (`CardRow.checked`).
- [x] Esc / empty-⌫ closes the Filter Card before backing out of the page (it is registered in the
      layer stack), and the ⌘K row opens it.
- [x] No global binding was added or changed.
- [x] A row's tags never render as row accessories on a `split` page — vacuously true: row
      accessories are MOE-0039 and do not exist yet.
- [x] 18 UI tests, `tsc --noEmit`, `tsc + vite build`, and 7 Rust suites green. **`just lint` is red
      on the pre-existing `cargo fmt` diff in `crates/moe-extensions/src/ai_client/anthropic.rs`** —
      unchanged by this work, verified present on a clean `HEAD` earlier.
- [x] **Nothing has rendered on screen.** The whole of this ticket is user-visible behavior that only
      a manual pass confirms: the control appearing on history and not on a full-detail page, the
      card's anchor and its check, Esc ordering, and the platform Empty View when a tag excludes
      everything. Worth checking before trusting it. **Verified on screen 2026-10-12** — a manual pass
      confirmed all four, and the same pass produced the two changes in the comment below.

## Comments

- 2026-10-12 (agent): filed from the Raycast primitives grilling session; ADR-0043 records the
  decision, and divergence #2 in `docs/research/raycast-ui-primitives.md` records the ⌘P cost.
  Blocked by MOE-0041 and MOE-0042 so the Filter Card is built on the unified component and registers
  in the stack once, rather than becoming a sixth hand-rolled surface.
- 2026-10-12 (agent): **first half landed** (66f0b8e) — the contract. `Item.tags`, the `Filter`
  declaration on `ActionResult::List`, `Filter::derived()` and a chainable `with_filter()`, plus the
  first consumer: chat history tags every conversation Agentic or not and declares one derived
  dimension labelled Tags. Tested on its own terms (the declaration rides the result with derived
  values; the tag derivation itself). **Not demoable alone** — nothing in the UI reads the fields —
  which is the one way this step departs from a tracer bullet, and the reason it is recorded rather
  than glossed.
- 2026-10-12 (agent): **second half landed** (7a500d1) — the UI. The tracer bullet completes: ⌘P
  then Tags then Agentic narrows chat history to agentic conversations, and "All Tags" restores it.
  The control is `#query-row`'s third child; the Filter Card is a `createCard` instance anchored under
  its trigger and registered in the layer stack; filtering narrows `v.items` from `v.allItems` so
  focus, navigation, the actions card, the delete slots and `pageShapeOf` keep walking one list.

  Two decisions taken while building, both folded into ADR-0043's amendment: **one predicate decides
  whether a page is filterable** (the first cut gated on `mode` alone and would have shown the control
  on a full-detail page, which ADR-0043 excludes), and **the tag-filtered-to-empty case shows the
  platform's Empty View**, since "No matching chat history" is wrong copy when the query matched and
  the tag did not.

  Also: the three filter rules moved to `ui/src/filter.ts` — `main.ts` queries the DOM at module load,
  so a test cannot import it at all — and are covered by 12 tests. `createCard` gained
  `filteredEmptyText`, because the Filter Card's filtered-empty copy is not the actions card's.

  **Unticked, deliberately:** nothing has rendered on screen, and the height invariant is arithmetic
  rather than measured. Both are stated in the acceptance criteria rather than glossed.

  The plan the half above executed, kept because it matched step for step — the one change is step 4's
  fallback-to-All rule, which ended up in `filter.ts` so it could be tested:

  1. `ui/src/types.ts` — the `Filter` mirror, `Item.tags?: string[]` (the Rust side omits it when
     empty), `filter?: Filter` on the list variant.
  2. `ui/index.html` — the Filter Control as `#query-row`'s **third child**, after the `flex-1`
     column, and the Filter Card anchored `right-2 top-14` (under its trigger, per ADR-0028's
     anchor-follows-trigger amendment).
  3. `ui/src/styles.css` — `.moe-filter-control` (label + chevron, muted; **sized inside the row band
     so `#query-row`'s measured height never changes**, ADR-0043) and `.moe-filter-card` added to the
     floating-card selectors.
  4. `ui/src/main.ts` — the View gains `allItems` / `filter` / `filterValues` / `filterValue`;
     `applyResult` derives the values **before** the tag filter narrows anything and falls back to All
     when the new set no longer carries the remembered value; `setFilterValue()` re-widens `items`
     from `allItems` and resets focus. **Filtering narrows `v.items` rather than happening at render
     time**, so focus, navigation, the actions card, the delete slots and `pageShapeOf` keep walking
     one list — which is what lets the platform filter without touching anything else.
  5. `ui/src/actions.ts` — a Filter source contributing "Filter by ⟨label⟩" (and "Clear ⟨label⟩
     filter" when one is set), which needs `filter`/`filterValue` on `PageContext`.
  6. The Filter Card is a `createCard` instance with `CardRow.checked` marking the active value and a
     platform-supplied "All ⟨label⟩" reset row.
  7. Edge case decided while planning: when the *tag* filter excludes everything, the Empty View must
     be the **platform's**, not the Extension's — "No matching chat history" is wrong copy when the
     query matched but the tag did not. Planned copy: `No matches for “⟨value⟩”` / "Clear the filter
     to see everything."

  Left to **MOE-0046** (unchanged): session scope, the pending-freeze, and the entry reset.
  "Values derived before filtering" moved into step 4, since shipping the vanishing-value bug and
  fixing it a ticket later would have been silly.
- 2026-10-12 (agent): **scope trimmed.** This was eight behaviors plus a consumer in one ticket — too
  much for one context window. The visible path stays here; the edge rules move to MOE-0046, which is
  separately demoable ("type a query and the tag list does not shrink"). Splitting also removed the
  reason to hold a second dimension hostage: MOE-0048's TagList needs a declared dimension, so it now
  blocks on this ticket directly.
- 2026-10-12 (agent): **regression fixed after close** (712ca26). The manual pass the ticked boxes
  above could not do caught it: entering any list result, including ⌘P → Tags, threw
  `undefined is not an object (evaluating 'filter.values.length')` and rendered nothing. `Filter.values`
  is omitted on the wire when empty (`skip_serializing_if = "Vec::is_empty"`, matching `Item.tags`),
  but the mirror declared it required and `applyResult` read it directly — so the only dimension built
  so far, a derived one, always tripped it.

  `values` is now optional in the mirror; the declared-else-derived rule moved into `filterValuesFor`
  (`ui/src/filter.ts`), which reads absent and empty the same way; and both sides are pinned — a Rust
  test asserts the omission and the serialized case, and TS tests cover declared-wins,
  empty-derives and absent-derives. ADR-0043's third amendment records it. The last acceptance box
  below — "nothing has rendered on screen" — is exactly the one this slipped past; it stays unticked
  until a human presses the keys.
- 2026-10-12 (agent): **verified on screen, and two follow-ups landed** (5c304e4). The manual pass the
  fix above was waiting on confirmed the control shows on history and not on a full-detail page, the
  card anchors under its trigger with its check, Esc closes the card before backing out, and the
  platform Empty View shows when a tag excludes everything. The last acceptance box is now ticked.

  Two changes came out of that pass, both reverting an earlier decision (ADR-0043's fourth amendment):
  **the filter now takes Raycast's `⌘P`** — Raycast's docs say the dropdown opens "by using the global
  shortcut `⌘P` or by clicking on it" — so where a page declares a dimension the chord opens the Filter
  Card and Browse keeps it everywhere else; and **the control is now a ghost** (no fill or border at
  rest, a hover wash, a soft accent chip once a value is set). Withdrew divergence #2 in
  `docs/research/raycast-ui-primitives.md` rather than renumber. Opening the Filter Card also now closes
  the actions and About cards, matching the peer rule the other two already followed.
- 2026-10-12 (agent): **the chip restyled and `⌘P` given one owner** (e479809), both from a manual pass
  over MOE-0039's accessory work.

  The ghost treatment is gone: the chip now wears the same glass as the About chip and the action
  capsule (surface-float + sheen + chip shadow), muted at rest and the main text step plus an accent
  hairline once a value is set. The `align-self: stretch` height rule is unchanged, so the height
  invariant still holds.

  And `⌘P` was being advertised twice in the ⌘K card — Browse Records and Filter by Tags both showed it,
  because the fourth amendment gave the filter the chord while Browse kept printing it. Raycast has no
  such case (nothing else of its sits on `⌘P`), so the rule borrowed from it is the general one: one
  owner per chord. The Browse row renders no Kbd on a page that declares a dimension and stays
  clickable. ADR-0043's fifth amendment records both.
- 2026-10-12 (agent): **the chip is one row band tall** (02bddd0). A stretched chip could not exceed
  the row's content box, so the band moved into the row: `.moe-query-column` reserves one row band
  unconditionally and the chip fills it, which makes the height invariant hold without depending on the
  chip being present. The cost is real and accepted: the bar goes 48px → 68px and the panel 768×510 →
  768×530, so `tauri.conf.json` and `PANEL_HEIGHT` both moved (ADR-0026's second amendment, ADR-0043's
  sixth).