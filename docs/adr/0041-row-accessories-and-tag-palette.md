# Row accessories and a bounded tag palette

Result rows can carry a title and a subtitle and nothing else (`Item`,
`crates/moe-core/src/contract.rs:42-67`), so an Extension holding a status, a date, or a tag has
nowhere to put it: the AI history list folds a relative timestamp into the subtitle, and the model
picker's only real tag vocabulary (`capabilities: Vec<String>`, `ai_models.rs:486-487`) rides a wire
type that never reaches the panel. Raycast solves this with `List.Item.Accessory` plus a bounded
`Color` enum that adapts to the theme. We adopt both — with the palette mapped onto ADR-0035's
semantic token layer, because the panel is translucent and an Extension-supplied hex cannot be
contrast-checked against the composed backdrop.

## Decision

- **`Item.accessories`** — an optional ordered list rendered at the row's trailing end. One tagged
  enum covers Raycast's five kinds: `tag` (a value in a tinted pill), `text` (plain secondary text),
  `date` (a unix timestamp), `icon` (a Lucide name), each with an optional `tooltip`. Shipping four
  of the five costs nothing over shipping one, so the enum is whole.
- **The platform formats dates, not the Extension.** Raycast takes a `Date` and renders it
  relatively ("now", "1d"); Moe formats backend-side today (`relative_time` in `ai.rs:104-123`). One
  UI formatter means one output vocabulary and a value that stays correct while the panel sits open
  — a backend-formatted "now" is stale the moment it renders. The history subtitle migrates onto a
  `date` accessory. `recency.rs`'s group labels stay backend-side: those are section names, not live
  values.
- **A bounded nine-name palette, as theme-paired semantic tokens.** Raycast's standard `Color` is
  exactly nine names — Blue, Green, Magenta, Orange, Purple, Red, Yellow, PrimaryText,
  SecondaryText — that "automatically adapt to the Raycast theme". They become
  `--moe-tag-{blue,green,magenta,orange,purple,red,yellow}`, each with a light and a dark value in
  `styles.css`; PrimaryText/SecondaryText are not new colors but the existing `--moe-fg` /
  `--moe-fg-muted` steps, so an uncolored accessory needs no token of its own.
- **No raw colors, and no dynamic colors yet.** Raycast also offers `Color.Raw` (arbitrary
  hex/HSL/keyword) and `Color.Dynamic` (an explicit light/dark pair). Raw is rejected outright:
  ADR-0035's rule is that view code names roles and never hues, and its contrast guarantee — every
  text step ≥4.5:1 measured against the *worst composed* backdrop — cannot be honored by a value the
  platform has never seen. Dynamic is deferred until a real case needs exact brand reproduction.
- **A tag is tinted text, not a bordered chip.** Raycast's tag "changes the text color to the
  provided color and sets a transparent background with the same color" — no border. That is what
  makes this compatible with ADR-0030, whose amendment de-chipped the root row's kind badge because
  a *bordered* box "reads as a control; it is a descriptor". That reasoning survives: the kind badge
  stays plain text, and only result-row tags carry the tint.
- **Accessories hide in the `split` shape.** Raycast is explicit that when detail is showing "it is
  recommended not to show any accessories on the List.Item and instead show the additional
  information in the List.Item.Detail view". Adopted verbatim — the left column is 280px
  (`LIST_NARROW_CLASS`, `ui/src/layout.ts:77-78`) and the information belongs in the pane, as
  ADR-0042's `TagList`.
- **Three rendered accessories at most; the title truncates first.** Raycast documents no cap, but
  its rows are not 40px tall inside a fixed ten-row grid (ADR-0026). The cap protects the row
  height, which the measured-height path cannot recover from mid-session.
- **Accessory icons stay Lucide** (ADR-0012), not Raycast's `Icon` enum. Tinting one needs a
  semantic-color→class mapping in `iconEl`, which today takes only a `className`
  (`ui/src/icons.ts:141-151`).

## Cost

- **Root rows and result rows now have visibly different trailing treatments** — a plain-text kind
  badge on the root, tinted tags on results. Deliberate, and defensible on ADR-0030's own reasoning,
  but it is a real exception to "one pattern, one UI/UX" and is recorded as a divergence from
  internal uniformity rather than from Raycast.
- Relative-date formatting moves from Rust to TypeScript, so the two must not both format: the
  `ai.rs` subtitle migration is part of this work, not a follow-up, or history rows show two dates.
- Nine names is a ceiling. An Extension wanting a tenth hue gets a semantic argument, not a hex code
  — that is the point, but it will eventually be asked for.
- The cap of three is a Moe guess with no Raycast precedent. If a real list needs four, what gives
  is the row height and the ten-row grid, and both are ADR-0026 constants.
