# The bottom bar: a ten-row grid, an avatar chip, and the About card

Raycast's panel reads as a fixed grid: exactly ten result rows, with a floating bottom bar that
overlays the last row's band — the left chip and the right action pill are the same height as a
row and line up with it. Toasts do not pop up on their own; they expand the bottom-left chip
into a pill. This ADR adopts that layout and turns the left chip into the About entry.

## Decision

- **Exactly ten rows by default.** The panel height is *measured* at runtime — input bar height +
  10 × row height (40px) + 8px bottom padding + the 24px window margins — and applied through the
  existing `resize_panel` IPC on startup and on shape switches. Constants live in
  `ui/src/layout.ts` (`ROW_HEIGHT`, `VISIBLE_ROWS`, `BOTTOM_PADDING`, `panelHeightFor`);
  `tauri.conf.json`'s 480px is only the first-frame fallback and must match.
  With the default input bar this yields **768×480** (supersedes ADR-0018's fixed 768×540).
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