# Raycast UI primitives: what the API provides, and how Moe maps to it

> **Status:** research note. It specifies nothing.
> **Decisions:** ADR-0041 (row accessories, tag palette) · ADR-0042 (detail metadata) ·
> ADR-0043 (Filter Control, Filter Card, Empty View) · ADR-0044 (card unification, layer stack).
> **Vocabulary:** `CONTEXT.md` → Card, Filter Control, Filter Card, Row Accessory, Detail Metadata,
> Empty View.
> **Reviewed:** 2026-10-12

## Why this exists

Moe's page patterns were designed Raycast-first, but "Raycast-like" was doing a lot of unexamined
work: which parts of Raycast are actually available to an extension author, which are host chrome we
would have to invent, and where Moe already deliberately differs. This note answers those three
questions and then points at the ADRs. It is the reference for *fidelity*, not the source of any
decision.

Raycast's UI documentation calls its supported components a design system: an extension declares a
view, and Raycast renders it natively. That model — not its React API — is what Moe follows.

## 1. What Raycast actually provides to extension authors

| Raycast API | What it does |
|---|---|
| `List`, `List.Item`, `List.Section` | Presents similar entities. The host search bar fuzzy-matches titles and `keywords`; sections group items; items carry `accessories`, `icon`, `detail`; lists support `isLoading`, `pagination`, and an `EmptyView`. |
| `List.isShowingDetail` + `List.Item.Detail` | Adds a right-hand detail pane for the selected item; the docs recommend hiding row accessories while it is shown. |
| `Grid`, `Grid.Item`, `Grid.Section` | Image-first alternative to `List`, with the same search bar, actions, sections and dropdown accessory. |
| `Detail`, `Detail.Metadata` | Renders CommonMark with a structured metadata block (`Label`, `Link`, `TagList`, `Separator`) in the right-hand area. |
| `List.Item.Detail.Metadata` | The same block, rendered at the **bottom** of a list item's detail. |
| `Form` and its fields | Collects structured values (text, textarea, dropdown, tag picker, date, file) with validation and drafts. Distinct from the search bar. |
| `ActionPanel`, `Action`, `ActionPanel.Section` | Context-aware actions on a view or item, groupable, each with an optional shortcut. First and second become primary/secondary: `↵` and `⌘↵` in List/Grid/Detail, `⌘↵` and `⌘⇧↵` in Form. |
| `ActionPanel.Submenu` | Replaces the panel with a nested choice list; supports filtering, `isLoading`, `onOpen` lazy loading. |
| `List.Dropdown` / `Grid.Dropdown` | A secondary filter dimension on the search bar's right side, opened by click or `⌘P`; supports `storeValue`, sections, and its own filtering. |
| `List.Item.Accessory` | Row-trailing `tag`, `text`, `date` (formatted relatively), `icon`, `tooltip`; colors from the `Color` vocabulary. |
| `Color` | Nine standard names (Blue, Green, Magenta, Orange, Purple, Red, Yellow, PrimaryText, SecondaryText) that adapt to the theme; plus `Color.Dynamic` (light/dark pair) and `Color.Raw` (arbitrary hex/HSL/keyword). |
| `Icon`, `Image`, `Image.Mask`, `FileIcon` | A large built-in icon enum, bundled/remote assets, masks, and `tintColor`. |
| `useNavigation`, `Action.Push` | Push/pop views on a navigation stack; Escape pops. |
| Toast, HUD, Alert | Operation feedback, post-close confirmation, and a confirmation prompt (`confirmAlert`). |

### Boundaries: host-owned, not extension-owned

- **The search bar and the window are the host's.** Extensions set search text, placeholder,
  filtering, and one accessory; they cannot add arbitrary chrome. Window operations are limited to
  `clearSearchBar`, `closeMainWindow`, `popToRoot`.
- **There is no extension-level bottom action rail, avatar chip, or About surface.** Raycast's
  `ActionPanel` is the contextual action mechanism; the floating capsule and chip are Moe inventions
  (ADR-0015, ADR-0026).
- **There is no free-floating custom overlay.** Raycast's global command search is host chrome, and
  `ActionPanel.Submenu` is contextual, not an arbitrary positioned window. Moe's top-centered Side
  View cards have no Raycast counterpart.
- **Grid and Form exist but imply page shapes Moe does not have.** ADR-0018 fixes exactly three.

## 2. The fidelity rule

**Follow Raycast on user-visible behavior and layout. Do not follow its API, and do not follow its
keybindings.** Record every divergence (§4).

- Its **API** is React components; Moe's is serde contracts plus a vanilla TypeScript view layer
  (ADR-0007). Fidelity at the API layer is foreclosed, and attempting it would reverse that ADR.
- Its **keybindings** collide with Moe's Keymap, which is platform-owned and identical across
  Extensions (`CONTEXT.md` → Keymap). The one it shares, it now follows: `⌘P` opens the search-bar
  dropdown in Raycast, and Moe gives that chord to the Filter Control on any page that declares a
  dimension, leaving Browse the slot everywhere else (ADR-0043's fourth amendment). Every other Raycast
  chord remains Moe's own — ADR-0014's amendment corrects that ADR's ⌘P attribution.
- Its **behavior and layout** are what make it feel like Raycast: fixed page shapes, search bar plus
  accessory, contextual actions, one empty mechanism, loading below the search bar, metadata placed
  by context. All of that is adopted.

## 3. Mapping: Raycast → Moe

| Raycast | Moe | Decided in |
|---|---|---|
| `List` / `List.Item` / `List.Section` | Result List / `Item` / Section (source grouping) and `Item.group` | ADR-0018, ADR-0020 |
| List search bar | Input Bar | ADR-0034 |
| `List.Dropdown` search-bar accessory | **Filter Control** + **Filter Card** | ADR-0043 |
| `Dropdown.storeValue` | Session-scoped filter state per (Extension, command) | ADR-0043 |
| `List.Item.Accessory` | **Row Accessory** (`Item.accessories`) | ADR-0041 |
| `Color` standard nine | Nine theme-paired semantic tokens | ADR-0041 |
| `Color.Raw` / `Color.Dynamic` | Rejected / deferred | ADR-0041 |
| `List.isShowingDetail` + `List.Item.Detail` | `split` Page Shape | ADR-0018 |
| `List.Item.Detail.Metadata` (bottom) | **Detail Metadata** at the pane's bottom | ADR-0042 |
| `Detail` + `Detail.Metadata` (right) | `detail` Page Shape + a right-hand metadata column | ADR-0042 |
| `List.EmptyView` | **Empty View** (replaces the notice item) | ADR-0043 |
| `isLoading` bar below the search bar | Hairline progress under the Input Bar; stale rows kept | ADR-0043 |
| `ActionPanel` / `.Section` | Show All Actions card; `ActionSource` sections and `ActionRank` order | ADR-0037, ADR-0028 |
| `ActionPanel.Submenu` | A "Filter by Tag" row in the ⌘K card opening the Filter Card | ADR-0043 |
| Primary `↵` / secondary `⌘↵` | Apply `⏎` / Secondary Copy `⌥⏎` | `CONTEXT.md` |
| `useNavigation` push/pop | Layered Back (`Esc` / empty `⌫`), one step to the fresh root | ADR-0038 |
| Toast / HUD | Avatar Chip as toast host | ADR-0026 |
| `confirmAlert` | The stop-confirmation dialog | ADR-0036, ADR-0038 |
| `Icon` enum | Lucide semantic names | ADR-0012, ADR-0041 |

## 4. Deliberate divergences from Raycast

| # | Raycast | Moe | Why |
|---|---|---|---|
| 1 | Secondary action is `⌘↵` | `⌥⏎`, meaning Copy | The primary/secondary distinction is a fixed platform convention, not a per-Extension choice |
| 2 | Search-bar dropdown opens on `⌘P` | *(withdrawn 2026-10-12)* `⌘P` opens the Filter Card on a page that declares a dimension; Browse keeps the chord elsewhere | Raycast fidelity won: the chord is adopted where Raycast has the control, and the general action yields the slot there (ADR-0043's fourth amendment) |
| 3 | Extensions filter their own lists (`filtering={false}`, `onChange`) | The platform filters over `Item.tags` | Nothing paginates, so the whole set is already loaded (ADR-0043) |
| 4 | `Color.Raw` and `Color.Dynamic` available | Nine bounded semantic tokens only | ADR-0035: roles, never hues — and contrast must be measured against the worst *composed* translucent backdrop (ADR-0041) |
| 5 | `Icon` enum | Lucide names | ADR-0012 (ADR-0041) |
| 6 | Tags tinted wherever they appear | Tinted on result rows; the root kind badge stays plain text | ADR-0030's amendment: a descriptor is not a control. This is a divergence from *internal* uniformity, and it is the one that costs the most visually (ADR-0041) |
| 7 | `TagList.Item.onAction` only | A tag matching a filter dimension sets the Filter Control | Closes the tags↔filter loop; goes *past* Raycast, knowingly (ADR-0042) |
| 8 | Actions are keyboard-reachable | Metadata tags are pointer-only | No keymap slot for per-tag actions; named as a keyboard-first regression (ADR-0042) |
| 9 | React components | Serde contracts + vanilla TS DOM | ADR-0007 |
| 10 | `ActionPanel` as a fixed bottom region | A floating capsule overlaying the last row band, beside an avatar chip | ADR-0015, ADR-0026 |
| 11 | No About surface; Preferences is a `⌘,` window | An About Card from the avatar chip (`⌘⇧K`), plus a Settings Window | ADR-0026, ADR-0027 |
| 12 | Window size host-owned, unspecified | Fixed 768×510, a measured ten-row grid | ADR-0026 |

## 5. Non-goals

- **Grid / image-first results.** ADR-0018 fixes exactly three Page Shapes; a fourth is a product
  decision, not an extension freedom.
- **Form / multi-field entry.** No panel-page counterpart. Structured entry lives in the Settings
  Window, which `CONTEXT.md` defines as an ordinary window and explicitly "not a panel page".
  Raycast's `Form` is therefore unmapped, not deferred.
- **Pagination.** Nothing in `ui/` or `crates/` paginates or lazy-loads. ADR-0041's accessory cap and
  ADR-0043's client-side filtering both lean on this; introducing pagination reopens both.
- **`Image.Mask`, `FileIcon`, remote images, Quick Look.**
- **Menu bar commands, deeplinks, OAuth, background refresh** — outside the UI primitives.
- **The chat page.** It has no Input Bar (ADR-0034 amendment); its composer is a separate pattern.

## 6. Known gaps and follow-ups

Work implied by the four ADRs, in rough dependency order:

1. ~~`iconEl` needs a semantic-color→class mapping~~ **Done (MOE-0039):** `inkClass` maps a semantic
   color name to a class and `iconEl` takes `color` (`inkClass`, `ui/src/icons.ts`).
2. ~~A relative-date formatter in TypeScript, and migration of `ai.rs`'s `relative_time` subtitle~~
   **Done (MOE-0039):** `relativeTime` lives in `ui/src/dates.ts`, its buckets are ported into
   `dates.test.ts`, and the history subtitle is now a `date` accessory.
3. Three notice-item call sites migrate to the Empty View (`notice_item`, `ai.rs`;
   `no_match_item`, `ai_models.rs`; `notice_item`, `ai_commands.rs`); each needs an audit for `actions` a user
   would lose, and `no_match_item`'s Markdown guidance must move into `description` (ADR-0043).
4. `pageShapeOf`'s `hasDetail` test widens to include metadata (`pageShapeOf`, `ui/src/layout.ts`), and the
   `detail` shape's new metadata column width joins the constants there (ADR-0042).
5. `conversationRow` must be expressible as `CardRow` before the history card migrates; if it is not,
   extend `CardRow` rather than keep the duplicate (ADR-0044).
6. The layer-stack refactor must preserve confirm-dialog → attach-mode → pending-item precedence
   above the cards (`confirmOpen`, `attaching`, `openStopConfirm`, `main.ts`). This is its main regression risk (ADR-0044).
7. `CardRow.keepOpen` is the first per-row behavior flag; the discipline for future fields is
   "needed on at least two surfaces" (ADR-0044).
8. The three-accessory cap has no Raycast precedent; revisit if a real list needs four, since what
   gives is the row height and the ten-row grid (ADR-0041).
9. Keyboard access to metadata tags (divergence #8).
10. `Color.Dynamic`, and sections in filter value lists — both deferred with a named trigger
    (exact brand reproduction; a declared value list that needs grouping).

## Sources

### Raycast official extension documentation

- [User Interface](https://developers.raycast.com/api-reference/user-interface) ·
  [List](https://developers.raycast.com/api-reference/user-interface/list) ·
  [Grid](https://developers.raycast.com/api-reference/user-interface/grid) ·
  [Detail](https://developers.raycast.com/api-reference/user-interface/detail) ·
  [Form](https://developers.raycast.com/api-reference/user-interface/form)
- [Action Panel](https://developers.raycast.com/api-reference/user-interface/action-panel) ·
  [Actions](https://developers.raycast.com/api-reference/user-interface/actions) ·
  [Colors](https://developers.raycast.com/api-reference/user-interface/colors) ·
  [Icons & Images](https://developers.raycast.com/api-reference/user-interface/icons-and-images) ·
  [Navigation](https://developers.raycast.com/api-reference/user-interface/navigation)
- [Keyboard](https://developers.raycast.com/api-reference/keyboard) ·
  [Window & Search Bar](https://developers.raycast.com/api-reference/window-and-search-bar) ·
  [Feedback](https://developers.raycast.com/api-reference/feedback) ·
  [Best Practices](https://developers.raycast.com/information/best-practices)

### Moe

- Glossary: [`CONTEXT.md`](../../CONTEXT.md)
- New: [ADR-0041](../adr/0041-row-accessories-and-tag-palette.md) ·
  [ADR-0042](../adr/0042-detail-metadata.md) ·
  [ADR-0043](../adr/0043-filter-control-and-empty-view.md) ·
  [ADR-0044](../adr/0044-card-unification-and-layer-stack.md)
- Amended by this work: [ADR-0014](../adr/0014-general-actions-browse-actions-new.md) (⌘P
  attribution) · [ADR-0015](../adr/0015-floating-action-bar-and-kbd.md) (retired ⌘⇧P) ·
  [ADR-0026](../adr/0026-bottom-bar-avatar-chip-and-about-card.md) (512→510) ·
  [ADR-0028](../adr/0028-one-actions-card-both-windows.md) (input-edge row; anchor-follows-trigger) ·
  [ADR-0034](../adr/0034-input-bar-identity.md) (stale token sizes)
- Background: [ADR-0007](../adr/0007-vanilla-ts-view-layer.md) ·
  [ADR-0012](../adr/0012-ui-icons-lucide.md) ·
  [ADR-0013](../adr/0013-result-view-shape-is-declared.md) ·
  [ADR-0018](../adr/0018-three-page-shapes.md) ·
  [ADR-0030](../adr/0030-root-row-anatomy.md) ·
  [ADR-0031](../adr/0031-front-most-list-owns-navigation-keys.md) ·
  [ADR-0035](../adr/0035-two-themes-semantic-tokens.md) ·
  [ADR-0037](../adr/0037-actions-card-registered-sources.md) ·
  [ADR-0038](../adr/0038-result-pages-own-the-input.md)
- Code cited above: `crates/moe-core/src/contract.rs` · `crates/moe-core/src/keymap.rs` ·
  `crates/moe-extensions/src/{ai,ai_models,ai_commands,recency}.rs` ·
  `ui/src/{layout,card,actions,main,chat,models,icons}.ts` · `ui/{index,chat}.html`
