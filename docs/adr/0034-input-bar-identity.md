# Input Bar identity: the leading slot says where you are

ADR-0015 fixed the Input Bar's grid with a **search** icon (16px, `text-zinc-400`) and `text-sm`,
one full step with the list rows. Two problems surfaced with use:

- The search glyph reads "you are searching" on every page, including nested ones where the bar
  filters an extension's records and the meaningful affordance is Back — a search icon where the
  Escape path lives.
- The bar's text was the same size as the row titles, so the bar scanned as just another row
  instead of the panel's anchor.

## Decision

- **The leading slot carries identity and navigation, not a search glyph.** Moe's mark on the root
  page; a **back arrow** on nested pages, clickable (the same path as `Esc` / empty `⌫`); a
  paperclip while attaching (ADR-0010). The placeholder still teaches that typing searches.
- **The grid invariant is untouched (ADR-0015).** The slot is always 16px:
  `px-5` == list `px-2` + row `px-3` and `gap-2.5` == the rows' gap, so icon x and label x stay
  exactly aligned with the item rows **whatever the text size does**.
- **The text size is a token, not a literal.** `--moe-text-input` (16px, one step above the row
  titles) in `styles.css`; the panel height is measured from the real bar (ADR-0026), so the
  10-row grid self-corrects if the token changes.

## Cost

- The old search glyph is gone from the bar; the bar no longer echoes the current extension
  (identity lives in the bottom-left chip) — deliberate: on nested pages the extension identity
  would duplicate the chip, while Back is the thing the bar's Escape/⌫ path makes useful.
- The root page now shows Moe's mark twice (Input Bar and chip). Accepted: the mark at the top
  says "this is Moe's palette", the chip's is a menu.
- The token is text-size only; the bar's row height still comes from the font's line box, which is
  why the measured-height path (ADR-0026) must stay in place.
## Amendment: the mark is a square-m glyph, bigger and bolder

The root state's leading slot used the `moe.png` avatar; it is now **`square-m`** (Lucide) — a
glyph, matching the slot's other states (back arrow, paperclip) in shape language rather than a
bitmap that had to fight the 16px column. And the glyph read too small for the panel's anchor, so
it renders **bigger and bolder than the item-row icons**.

Both numbers are tokens: `--moe-input-icon-size: 20px` and `--moe-input-icon-stroke: 2` (the initial `2.5` read too bold at 20px). The slot
element stays a 16px-wide flex box, so the glyph overflows its column symmetrically and the grid
invariant above is untouched — icon center and label x still line up with the item rows. CSS drives
size and stroke (presentation attributes lose to CSS), so tuning stays in `styles.css`.
