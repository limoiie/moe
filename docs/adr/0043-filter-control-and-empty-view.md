# Filter Control, Filter Card, and the Empty View

Three gaps surfaced together while specifying the UI primitives. There is no way to narrow a list by
a second dimension: `Item` has no tags and `ActionResult::List` carries only `items` and
`detail_full` (`crates/moe-core/src/contract.rs:71-106`). There is no empty state in the main result
list — `render()` has no zero-length branch (`ui/src/main.ts:540-615`), so a query matching nothing
shows a blank panel, and three Extensions paper over that by returning a real `Item` as the empty
answer (`notice_item`, `ai.rs:1056-1059`; `no_match_item`, `ai_models.rs:1111-1126`;
`ai_commands.rs:168-180`). And a search refresh shows nothing at all. Raycast has one answer for
each: the `List.Dropdown` search-bar accessory, `List.EmptyView`, and an `isLoading` bar below the
search bar.

## Decision

- **The platform filters; the Extension declares the dimension.** `Item.tags` carries the values,
  and `ActionResult::List` gains an optional dimension declaration (id, label, optional icon,
  optional explicit value list). Values are **derived from the returned items' tags** when no list
  is given, so a declared list cannot drift from the tags actually present. Filtering happens in the
  UI over the already-returned set: Moe has no pagination anywhere, every list renders its whole set
  in one pass, so client-side filtering is not the usual "only what's loaded" compromise.
  Server-side filtering (a filter argument on `Extension::invoke`) is the named fallback if
  pagination ever lands or a dimension needs values absent from the loaded set.
- **`payload` was never an option.** It is documented "opaque to the UI; sent back to the Extension
  verbatim" (`contract.rs:59`), so it cannot carry values the UI reads. That is why tags are a
  first-class field rather than a convention inside the payload.
- **A platform-supplied "All ⟨label⟩" row resets the filter**, and the active value carries a check
  — `CardRow.checked` already exists and renders it (`ui/src/card.ts:24`), so the Filter Card needs
  no new row capability. Derived values have no natural grouping, so no sections in v1.
- **The Filter Control is the Input Bar's first trailing element.** `#query-row` has exactly two
  children today — the leading identity slot and a `flex-1` column holding `#query` above
  `#query-subtitle` (`ui/index.html:22-36`) — so the control is appended as a third child and the
  input column narrows. It renders as **label + chevron**: "Tags" at rest, "Tags: Agentic" when set,
  truncated, with a right inset mirroring the row's `px-5`.
- **It must not change the Input Bar's height.** Panel height is measured from `#query-row`
  (`main.ts:435-439`), but `resize_panel` fires only at startup and on a shape change —
  `applyPanelSize` early-returns when the shape is unchanged and there is no `ResizeObserver`
  (`main.ts:424-433`). A control that grew the row mid-session would silently clip the ten-row grid
  (ADR-0026) with no error, so the control is sized inside the existing row band, always.
- **Result pages only** — the `list` and `split` shapes. Not `detail` (a single full-detail result
  has nothing to narrow) and not the command layer (the root groups by Extension source per
  ADR-0020, where a tag axis has no meaning).
- **No global shortcut.** Raycast binds its dropdown to ⌘P; Moe's ⌘P is Browse on every surface
  (`crates/moe-core/src/keymap.rs:66`) and toggles the history card in the Side View
  (`ui/src/chat.ts:883-888`). Browse is a *general* action present everywhere, while a filter exists
  on some result pages — spending a universal slot on a usually-absent control is the worse trade,
  and rebinding Browse breaks documented muscle memory in both windows. The retired `⌘⇧P` is
  deliberately **not** revived (ADR-0015 amendment). Keyboard access comes from a **"Filter by Tag"
  row in the ⌘K card that opens the Filter Card** — Raycast's own `ActionPanel.Submenu` pattern,
  "handy when an action needs to select from a range of options" — plus click and Tab.
- **The Filter Card anchors top-right, under its trigger.** Every existing card already anchors
  next to the control that opens it (pill→bottom-right, chip→bottom-left, Side View
  toolbar→top-center). The rule becomes explicit — **a card anchors near its trigger** — which gives
  the Filter Card a fourth anchor without importing the Side View's.
- **Filter and query combine with AND**, with two sub-rules that keep it from being confusing: the
  Filter Card's value list is derived **before** the tag filter is applied, so values cannot
  disappear while typing and the active tag can never vanish from its own picker; and while an item
  is `pending`, a filter change re-runs nothing — ADR-0038's "while a result streams, the page owns
  the input" guard covers the filter exactly as it covers the Input Bar, or changing a tag mid-stream
  orphans the generation.
- **Filter state is session-scoped per (Extension, command), never written to disk, and adds no Back
  layer.** That is the faithful reading of both precedents: Raycast's `storeValue` is a cache
  ("restored next time the dropdown is rendered"), not a preference; and ADR-0038 governs the Input
  Bar text and Back's destination, not filter memory. Entering the list fresh via ⌘P/⌘N resets it,
  consistent with ADR-0038's "every entry clears it".
- **The Empty View replaces the Notice Item.** `ActionResult::List` gains an optional empty-view
  declaration (title, description, optional icon, optional actions) that the platform renders when
  there are no rows. It is **non-focusable** — a focusable "no results" row is a row the user can
  Apply. Raycast has exactly one empty mechanism, and having two guarantees Extensions keep guessing
  which to use. The three notice-item call sites migrate; `no_match_item`'s guidance currently lives
  in `detail` (Markdown), which has no Empty View counterpart, so it moves into `description`.
- **Raycast's suppression rule, verbatim:** the Empty View is never displayed while loading and the
  query is empty. That is what keeps a first paint from flashing "no results".
- **Loading is a hairline progress bar under the Input Bar** — Raycast's placement, "a loading bar
  … below the search bar" — and **stale rows stay visible** until the new set lands; the list is
  never blanked mid-typing. `.moe-generating`'s bouncing dots are **not** reused: search latency and
  token streaming are different states, and conflating them makes a 40ms debounce look like a
  generation.

## Cost

- **The ⌘P divergence from Raycast is permanent** unless Browse is ever rebound. Recorded in the
  divergences list rather than left as a surprise, and ADR-0014's attribution is corrected there.
- Retiring the Notice Item touches three Extensions, and each site must be checked for `actions` a
  user would lose. `ai_commands.rs:168-180`'s are guidance cards, but that is a per-site
  verification, not an assumption.
- Client-side filtering can only offer values present in the loaded set. Invisible without
  pagination; a real limit the day pagination lands — at which point the dimension declaration
  already exists and only the evaluation side moves.
- The Filter Control is the Input Bar's first trailing element, and it collides with
  `#query-subtitle`: on the chat page that column already carries a second line (provider · model,
  `main.ts:815-828`). The two never render together today because the control is result-pages-only —
  but that is a consequence of the scope rule, not a structural separation, and putting a control on
  the chat page would have to solve it.
- Deriving values before filtering means the UI holds two views of one list (unfiltered for the
  picker, filtered for the rows). Small, but it is state that can disagree if one path is updated
  without the other.
- "No pagination" is now load-bearing in two places: here, and in ADR-0041's row-height reasoning
  for the three-accessory cap.
