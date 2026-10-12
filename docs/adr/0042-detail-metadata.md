# Structured detail metadata, in Raycast's two placements

`Item.detail` is Markdown and nothing else (`crates/moe-core/src/contract.rs:63`), so structured
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
  (`layout.ts:98-107`); the test widens to include metadata. A metadata-only item has something to
  show, and silently rendering no pane would be a worse surprise than a pane with no prose.
- **ADR-0013 is not being reversed.** The shape is still *declared*, never guessed: `detailFull`
  remains the Extension's decision, and this change widens only *whether a pane exists*, not which
  shape a result takes. Written down here so it does not read as a quiet reversal later.
- **A tag may be clickable, by reference.** Raycast's `TagList.Item` has `onAction`; a closure
  cannot cross the IPC boundary, so a Moe tag carries an optional `actionId` that must resolve to
  one of the item's declared `Action`s and dispatches through `run_item_action`
  (`contract.rs:331-338`). A tag with no `actionId` is display-only, and an unresolvable id renders
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
