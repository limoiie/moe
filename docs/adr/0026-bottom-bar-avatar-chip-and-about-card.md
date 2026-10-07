# The bottom bar: a ten-row grid, an avatar chip, and the About card

Raycast's panel reads as a fixed grid: exactly ten result rows, with a floating bottom bar that
overlays the last row's band — the left chip and the right action pill are the same height as a
row and line up with it. Toasts do not pop up on their own; they expand the bottom-left chip
into a pill. This ADR adopts that layout and turns the left chip into the About entry.

## Decision

- **Ten rows plus one section header.** The panel height is *measured* at runtime — input bar
  height + 10 × row height (40px) + one section header (30px, fixed so it can be budgeted) +
  8px bottom padding + the card's 2px borders + the 24px window margins — and applied through the
  existing `resize_panel` IPC on startup and on shape switches. Constants live in
  `ui/src/layout.ts` (`ROW_HEIGHT`, `VISIBLE_ROWS`, `SECTION_HEADER_HEIGHT`, `HEADER_ROWS`,
  `BOTTOM_PADDING`, `CARD_BORDER`, `panelHeightFor`); `tauri.conf.json`'s 512px is only the
  first-frame fallback and must match. With the default input bar this yields **768×512**
  (supersedes ADR-0018's fixed 768×540). Every list shows at least one group label (Suggestions
  on the root page), so one header is reserved on top of the ten rows; pages showing more headers
  at once fit fewer rows, which is inherent to grouping.
- **The bottom bar overlays the last row's band exactly.** `#bottom-bar` is positioned 8px above
  the card's bottom edge with a height of one row, so its band equals the list's last visible row
  (the list has no bottom padding of its own anymore; the detail pane keeps its padding since it
  is content, not rows). The chip's avatar is centered on the item icons' x (28px from the card
  edge); the pill is padded 8px from the right edge as before. The bar is `pointer-events: none`
  with the chip and pill opting back in, so row clicks outside them still work.
- **Avatar chip (bottom-left).** Shown in the same shape language as the pill: a circle around
  Moe's app avatar on the command layer, expanding to icon + name once inside a command — the
  owning extension's identity, fetched via a new `extension_meta(command_id)` IPC
  (`ExtensionMeta { id, title, icon }`, avatar = the extension's first command icon, cached per
  command in the UI).
- **Toasts live in the chip.** `toast(text, icon)` expands the chip into a pill with the message
  and collapses it after 1.4s; the separate top-centered toast element is gone. Lightweight
  feedback now happens where the eye already is (bottom-left), Raycast-style.
- **The chip opens the About card.** Same UX as the actions card (ADR-0015): floating card, ↑↓
  select, ⏎ run, `Esc`/empty `⌫` dismiss, click-outside closes, row styling identical — with the
  search input at the **bottom** instead of the top. Rows are the app's meta actions:
  **Open Config File**, **Save AI Key**, **Send Feedback** (opens the repo's issue tracker via a
  new `open_external` IPC that only accepts http/https). The card is a layered-back target like
  the actions card.

## Cost

- The panel is shorter than before (400 vs 540): the detail/split shapes get less vertical room.
  This is the price of the exact ten-row grid; `VISIBLE_ROWS` is one constant if that trade-off
  is ever revisited.
- The banner and the attachment row shrink the list (they are flex siblings), so they show fewer
  than ten rows while visible — acceptable, the grid restores as soon as they are gone.
- The About card duplicates `openActionsCard`'s rendering code by intent (a few dozen lines):
  the two cards are deliberately separate surfaces with different row sources and input
  placement; a shared abstraction can wait until a third card needs it.

## Amendment: one card language — the input at the bottom, rows in sections

The panel's two cards — the actions card and the About card — are now one layout, so both read
the same way (the Side View's More Actions card joins them in ADR-0028, with its anchor and its
input edge as the two deliberate differences):

- **Every card's input sits at the bottom edge** — the actions card's filter input moves from the
  top edge to the bottom edge, matching the About card; the input-at-the-bottom contrast in the
  decision above stops being the About card's privilege and becomes the panel's card convention
  (supersedes ADR-0015's "top input filters" phrasing too). Rows stay anchored to the top edge,
  so the list does not jump down when the filter narrows it. The Side View's More Actions card is
  the exception: it anchors top-center and keeps its input on the top edge (ADR-0028).
- **Rows are grouped into Raycast-style sections**, each headed by a small uppercase label that
  is not focusable and never participates in ↑↓ navigation (the same header contract as the root
  page's source sections, ADR-0020): the actions card shows the focused item's **Actions** first
  and, when the Extension declared them, the platform general actions (Browse ⌘P / New ⌘N,
  ADR-0014) under **General**; the About card groups its meta actions as **App** (Open Config
  File, Save AI Key) and **Support** (Send Feedback); the Side View's More Actions menu groups
  its entries as **Chat** / **App** / **Window**.
- **Filtering flattens the card** (Raycast): while the filter input is non-empty, matching rows
  render as one header-less list; clearing the input brings the sections back. ↑↓ always walks
  the flat row order, so headers never interrupt navigation.
- **The `Action` contract is unchanged**: sections are the card's own grouping (the item's
  actions vs the platform's general entries), not extension-provided metadata.