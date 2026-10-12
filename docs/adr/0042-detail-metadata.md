# Structured detail metadata, in Raycast's two placements

`Item.detail` is Markdown and nothing else (`Item.detail`, `crates/moe-core/src/contract.rs`), so structured
facts about a result — a status, a link, a set of tags — must be flattened into prose or dropped.
Raycast has a dedicated block for them (`Detail.Metadata` / `List.Item.Detail.Metadata`) with
`Label`, `Link`, `TagList` and `Separator`, and places it differently by context: at the **bottom**
of a `List.Item.Detail`, and in the **right-hand** column of a standalone `Detail`. We adopt the
block and both placements exactly, which is the first change to ADR-0018's `detail` shape since it
was decided.

## Decision

- **Four field types, one ordered list.** `Item.metadata` carries `label` (title, optional icon,
  optional text in a palette color), `link` (title, text, target), `tagList` (title, plus items of
  text/icon/color using ADR-0041's palette), and `separator`. Order is the Extension's; the platform
  neither sorts nor groups.
- **Two placements, exactly Raycast's.** In the `split` shape the block renders at the **bottom** of
  the detail pane, under the Markdown. In the `detail` shape it renders in a **right-hand column**
  beside the Markdown. This is not a compromise between two Moe options: Raycast's own two
  components differ this way, and the difference is legible — a pane sharing the window with a list
  puts metadata last, a page owning the window puts it to the side.
- **The `detail` shape is no longer purely full-width.** ADR-0018 defines it as 详情占满. With
  metadata present it becomes Markdown plus a right column; with metadata absent it is unchanged.
  The column's width joins the other shape constants in `ui/src/layout.ts`, so no renderer composes
  widths (ADR-0018's rule).
- **Metadata alone opens a pane.** `pageShapeOf` tests `item.detail != null || item.pending === true`
  (`pageShapeOf`, `layout.ts`); the test widens to include metadata. A metadata-only item has something to
  show, and silently rendering no pane would be a worse surprise than a pane with no prose.
- **ADR-0013 is not being reversed.** The shape is still *declared*, never guessed: `detailFull`
  remains the Extension's decision, and this change widens only *whether a pane exists*, not which
  shape a result takes. Written down here so it does not read as a quiet reversal later.
- **A tag may be clickable, by reference.** Raycast's `TagList.Item` has `onAction`; a closure
  cannot cross the IPC boundary, so a Moe tag carries an optional `actionId` that must resolve to
  one of the item's declared `Action`s and dispatches through `run_item_action`
  (`run_item_action`, `contract.rs`). A tag with no `actionId` is display-only, and an unresolvable id renders
  display-only rather than erroring, so the row's other tags keep working.
- **A tag matching a filter dimension sets the filter.** When a tag's text equals a value of a
  dimension declared for the page (ADR-0043), clicking it sets the Filter Control instead of running
  an action. That closes the loop between `Item.tags` and the detail's `TagList`: see a tag, click
  it, the list narrows. This is a Moe addition with no Raycast counterpart, and the one place this
  ADR knowingly goes past fidelity.
- **One palette.** Metadata colors are ADR-0041's nine semantic tokens; there is no second color
  vocabulary for the detail pane.

## Cost

- ADR-0018's `detail` shape changes for the first time, and the change is conditional on data — a
  reader comparing the two ADRs must hold "full-width unless metadata" in their head. The
  alternative (bottom placement in both shapes) would have left ADR-0018 untouched but visibly
  departed from Raycast's standalone `Detail`, which is the design being followed.
- Two placements means the detail renderer has two layouts, not one. The Markdown component is
  shared; only the container differs.
- Clickable tags add a second dispatch path into `run_item_action`, so an action can now be invoked
  from somewhere other than the Action Bar or the ⌘K card. Tags are pointer-only, which is a small
  keyboard-first regression worth naming rather than hiding.
- The filter-matching rule couples this ADR to ADR-0043: a tag behaves differently depending on
  whether the page declared a filter. If filters are ever removed, this rule goes with them.

## Amendment: the data model landed without a producer

MOE-0044 implemented the decision above: `Item.metadata`, the whole four-type enum, the widened
`pageShapeOf`, and the split pane's bottom block. Three things are worth recording.

**The enum landed whole, the renderers did not.** `tagList` and its `MetadataTag` struct cross the
contract and the mirror now; only MOE-0048 renders them. That is deliberate — the wire shape is the
part that is expensive to change later, and the two rendering tickets then grow renderers only.

**A link is a button, not an anchor.** The panel is a webview, so an `<a href>` would navigate Moe
away from itself; the row dispatches through `open_external`, which refuses anything but http(s)
before spawning a process (`open_url`, `moe-platform/src/open.rs`). Same route the About card's
feedback link already took.

**Nothing produces metadata yet, so the placement is unverified.** No Extension declares a `metadata`
list, which means the split pane's bottom block has never rendered. The model picker is the closest
candidate — its detail card carries the same facts this ADR wanted structured (`catalog_facts`,
`ai_models.rs`) — but moving them out of the prose is a change to a shipped surface, and the honest
home for it is MOE-0048, which needs a real `TagList` on a real page. Until then this ADR's first
placement claim rests on code review, not on a screen.

## Amendment: the tag list landed, and found its first producer

MOE-0048 implemented the interactive half. The tag-click rule is a pure function (`tagIntent`,
`ui/src/metadata.ts`) rather than a closure inside the renderer — it has three outcomes, and two of
them are the kind that go wrong quietly: an unresolvable `actionId` must degrade to display-only
rather than error, and a tag that matches a dimension must outrank one that carries an action.

**A display-only tag is a span, and an interactive one is a button with `tabindex="-1"`.** ADR-0042
names tags pointer-only, so they are deliberately outside the tab order; using a real button keeps the
cursor and the semantics honest, and makes MOE-0045's keyboard decision a one-attribute change rather
than a re-render.

**The first producer is chat history**, which is what made this ticket visible: its `Agentic` tag now
also appears as a clickable tag in the detail pane. That page already derives its dimension from
`Item.tags`, so the tag→filter rule needs no new declaration to work — which is the loop this ADR
described, on a page that already existed.

The `detail` shape's placement is still unbuilt (**MOE-0047**); the renderer takes its item and its
container so that ticket reuses it instead of writing a second one.
