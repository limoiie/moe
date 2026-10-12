# Filter Control, Filter Card, and the Empty View

Three gaps surfaced together while specifying the UI primitives. There is no way to narrow a list by
a second dimension: `Item` has no tags and `ActionResult::List` carries only `items` and
`detail_full` (`ActionResult::List`, `crates/moe-core/src/contract.rs`). There is no empty state in the main result
list — `render()` has no zero-length branch (`render`, `ui/src/main.ts`), so a query matching nothing
shows a blank panel, and three Extensions paper over that by returning a real `Item` as the empty
answer (`notice_item`, `ai.rs`; `no_match_item`, `ai_models.rs`;
`notice_item`, `ai_commands.rs`). And a search refresh shows nothing at all. Raycast has one answer for
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
  verbatim" (`payload`, `contract.rs`), so it cannot carry values the UI reads. That is why tags are a
  first-class field rather than a convention inside the payload.
- **A platform-supplied "All ⟨label⟩" row resets the filter**, and the active value carries a check
  — `CardRow.checked` already exists and renders it (`CardRow.checked`, `ui/src/card.ts`), so the Filter Card needs
  no new row capability. Derived values have no natural grouping, so no sections in v1.
- **The Filter Control is the Input Bar's first trailing element.** `#query-row` has exactly two
  children today — the leading identity slot and a `flex-1` column holding `#query` above
  `#query-subtitle` (`#query-row`, `ui/index.html`) — so the control is appended as a third child and the
  input column narrows. It renders as **label + chevron**: "Tags" at rest, "Tags: Agentic" when set,
  truncated, with a right inset mirroring the row's `px-5`.
- **It must not change the Input Bar's height.** Panel height is measured from `#query-row`
  (`measuredPanelHeight`, `main.ts`), but `resize_panel` fires only at startup and on a shape change —
  `applyPanelSize` early-returns when the shape is unchanged and there is no `ResizeObserver`
  (`applyPanelSize`, `main.ts`). A control that grew the row mid-session would silently clip the ten-row grid
  (ADR-0026) with no error, so the control is sized inside the existing row band, always.
- **Result pages only** — the `list` and `split` shapes. Not `detail` (a single full-detail result
  has nothing to narrow) and not the command layer (the root groups by Extension source per
  ADR-0020, where a tag axis has no meaning).
- **No global shortcut.** Raycast binds its dropdown to ⌘P; Moe's ⌘P is Browse on every surface
  (`Browse`, `crates/moe-core/src/keymap.rs`) and toggles the history card in the Side View
  (`toggleHistoryCard`, `ui/src/chat.ts`). Browse is a *general* action present everywhere, while a filter exists
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
  user would lose. `ai_commands.rs`'s are guidance cards, but that is a per-site
  verification, not an assumption.
- Client-side filtering can only offer values present in the loaded set. Invisible without
  pagination; a real limit the day pagination lands — at which point the dimension declaration
  already exists and only the evaluation side moves.
- The Filter Control is the Input Bar's first trailing element, and it collides with
  `#query-subtitle`: on the chat page that column already carries a second line (provider · model,
  `syncChatTopbar`, `main.ts`). The two never render together today because the control is result-pages-only —
  but that is a consequence of the scope rule, not a structural separation, and putting a control on
  the chat page would have to solve it.
- Deriving values before filtering means the UI holds two views of one list (unfiltered for the
  picker, filtered for the rows). Small, but it is state that can disagree if one path is updated
  without the other.
- "No pagination" is now load-bearing in two places: here, and in ADR-0041's row-height reasoning
  for the three-accessory cap.

## Amendment: two empty-state call sites, not three — and no `actions` field

Implementing this decision surfaced two corrections to it.

**The notice-item migration is two call sites, not three.** The decision above says "the three
notice-item call sites migrate". Auditing them found that all seven `ai_commands` notice items go
through `ActionResult::detail(...)` — the full-detail shape, `detail_full` true — carrying
multi-paragraph Markdown and fenced code ("## AI setup needed", a fenced error block). They are
**content cards, not empty states**: the list holds one item, so an Empty View would never trigger,
and a one-string `description` could not hold the prose. They keep their cards. The real migrations
are the sites that returned a **one-item list whose single item was a notice** — `ai.rs`'s zero-row
and read-failure history cases, and `ai_models.rs`'s no-match picker — because that is exactly what
made "no results" focusable.

**No `actions` field, though Raycast's `EmptyView` has one.** Moe's only action dispatch path is
`run_item_action`, which needs an `Item`, and an empty list has none. A field with no way to run it
would be dead API, so it is omitted rather than carried; both migrated sites declared zero actions,
so nothing is lost. If an empty view ever needs a row, the right shape is a reference to a
**Command** — Moe's platform action vocabulary — not an `Action`, and that is a design round of its
own rather than a field to slip in.

**And the palette's own no-match state needed no Extension at all.** `Registry::search` returns an
empty `Vec` when nothing matches and no Extension captures the input, so the platform supplies the
copy there. That makes the command layer the Empty View's first real consumer — ahead of any
Extension declaring one — and it is why the platform default exists at all rather than every empty
list needing a declaration.

## Amendment: one predicate, the platform's copy, and where the rules live

Building the Filter Control settled three things the decision above left to implementation.

**One predicate decides whether a page is filterable.** The decision scopes the filter to the `list`
and `split` shapes, but `mode == "items"` is true of all three — the first cut gated on mode alone and
would have shown the control on a full-detail page, where a single result has nothing to narrow.
`filterableDimension()` now gates on mode AND page shape (through `pageShapeOf`, the single shape
authority under ADR-0018) AND the declaration, and the Filter Control, the Filter Card and the ⌘K row
all read it. Three surfaces, one answer: they cannot disagree about whether the page has a dimension.

**When the tag filter empties the list, the Empty View is the platform's.** The Extension's copy says
why the *query* matched nothing ("No matching chat history"), which is wrong when the query matched
and the tag did not. A page narrowed to zero by a filter therefore shows `No matches for "⟨value⟩"` /
"Clear the filter to see everything." That is the list-is-empty principle one level in — **the party
that caused the emptiness supplies the words** — and it is why the value list has to be derived
before the filter is applied: a value that vanished from its own picker would leave the page narrowed
with no way to see or clear the choice.

**The rules live outside the DOM module.** `applyTagFilter`, `deriveFilterValues` and
`settleFilterValue` sit in `ui/src/filter.ts` rather than beside their caller, because `main.ts`
queries the DOM at module load and a test cannot import it at all. Twelve tests cover them — each one
a way the list could lie: showing rows the value does not match, offering a value nothing carries, or
staying narrowed by a value that has vanished. `createCard` also gained `filteredEmptyText`, since the
Filter Card's filtered-empty copy is not the actions card's.

## Amendment: an omitted `values` list reads as empty

`Filter.values` is omitted on the wire when empty — `skip_serializing_if = "Vec::is_empty"`, the same
shape as `Item.tags` — but the shared mirror first declared it required (`values: string[]`), and
`applyResult` read `filter.values.length` on every list result. A derived dimension (the only kind
implemented) leaves the vector empty, so it arrived **absent**, the read threw
`undefined is not an object (evaluating 'filter.values.length')`, and the whole result page failed to
render — the one path this feature exists to serve.

The omission is deliberate and stays: a derived dimension carries no explicit list, so the mirror must
say so, and `values?: string[]` now matches the wire. The "declared values if any, else derived" rule
moved out of `applyResult` into `filterValuesFor` in `ui/src/filter.ts`, so absent and empty are handled
in one place that a test can reach — `main.ts` queries the DOM at module load and cannot be imported.
Both sides are pinned: a Rust test asserts the vector is omitted while empty and serialized when set,
and TS tests cover declared-wins, empty-derives and absent-derives.

The lesson outlives the field: **a required TS field over an omittable Rust field is a type that lies**,
and `tsc` cannot catch it. Every new contract field must be checked both ways — for the serde attribute
that decides whether it rides the wire at all.

## Amendment: the Filter Control takes `⌘P`, and reads as a ghost

A manual pass over the shipped control reversed two things the decision above had settled the other way.

**The filter takes Raycast's chord.** The decision above — and divergence #2 — gave the Filter Control
no keybinding, because `⌘P` is Browse "on every surface" and "a general action outranks a page-local
control". Raycast's own docs are unambiguous: the dropdown "will be displayed on the right-side of the
search bar. Invoke it either by using the global shortcut `⌘P` or by clicking on it." The fidelity rule
follows Raycast on user-visible behavior, so `⌘P` now opens the Filter Card on any page where
`filterableDimension()` returns one, and **Browse keeps the chord everywhere else** — the command layer,
full-detail pages, the Side View. It is the same toggle on the same card as the click; the ⌘K row shows
the chord and the control's tooltip names it. This is the one place the platform keymap yields a slot to
a page-local control, and it is deliberate: Raycast has no Browse, so where Raycast shows a dropdown Moe
shows the filter. Divergence #2 is withdrawn rather than renumbered.

Opening it now closes the actions and About cards, matching the peer rule those two already followed —
the Filter Card had been the one card that did not.

**The control is a ghost.** At rest it carries no fill and no border — muted ink and a chevron — so it
reads as an accessory of the Input Bar rather than a button competing with the input. A hover reveals the
neutral wash; a set value turns the ghost into a soft accent chip (`--moe-accent-soft` with a
`--moe-accent-line` hairline), the ink staying at the main text step per ADR-0041's contrast rule. The
row-band constraint is untouched — no padding changed — so the measured panel height still cannot move.

## Amendment: one owner per chord, and the chip wears the house glass

Two corrections from a manual pass over the shipped control.

**`⌘P` has exactly one owner.** The amendment above gave the filter the chord on pages that declare a
dimension, but Browse's ⌘K row kept advertising `⌘P` there — two actions claiming one key inside the
same card. Raycast has no such case because nothing else of its sits on `⌘P`, and the model worth
keeping is therefore **one owner per chord**: where `filterableDimension()` returns one, the filter owns
`⌘P`, the Browse row renders no Kbd (it stays clickable), and the keydown path routes `⌘P` to the
Filter Card.

**The chip is a glass chip, not a ghost.** The amendment above made it a ghost. It now wears the same
material as the About chip and the action capsule — `--moe-surface-float`, `--moe-glass-sheen`, the
32px/200% backdrop, `--moe-shadow-chip` — reading `--moe-fg-muted` at rest and, once a value is set,
the main text step plus the accent hairline the composer capsule uses for its focus ring. The
`align-self: stretch` height rule is unchanged, so the height invariant above still holds.
