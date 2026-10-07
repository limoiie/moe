# About is a platform binding: ⌘⇧K

The About card (ADR-0026) opens from the avatar chip only — a pointer affordance, the one card in
the panel the keyboard cannot reach. Every other card has a platform keybinding (Show All Actions
⌘K / ⌘⇧P, Browse ⌘P, New ⌘N, …), and the chip is the only bottom-bar element without one.

## Decision

- `SystemKey::About` joins the unified keymap (`moe-core::keymap`), default **⌘⇧K** — the shifted
  sibling of ⌘K in the ⌘-layer the palette already uses for its own cards; the display string
  ("⌘⇧K") comes from the Rust table like every other binding (the view layer never invents keys).
- The panel lands it on `toggleAboutCard()` (⌘⇧K opens/closes, the same path as the chip click).
  The two cards are peers: opening either closes the other.
- The chip's tooltip carries the binding ("About Moe (⌘⇧K)"), read from the keymap IPC, so it
  cannot drift from the table.
- The Side View has no About card, so ⌘⇧K has no landing spot there (like ⌘M has none on the
  panel's root layer): a semantic without a landing spot does nothing; it is not remapped.

## Amendment: the rows show their bindings, and Open Config File gets ⌘,

The About card's rows used to carry no bindings at all — the one menu whose rows never showed a
Kbd, which read as a bug next to the actions card (the actions card shows ⏎ / ⌥⏎ / ⌘M / ⌘P /
⌘N / ⌘⇧F on its rows). Two things changed:

- **Rows render their binding** (`AboutRow.keys`) exactly like the shared card's rows do.
- **Open Config File joins the unified keymap as `SystemKey::OpenConfig`, default ⌘,** — the macOS
  Preferences convention (Raycast uses it for its settings too). It lands on the same
  `moe.open-config` command the row runs, in both windows (panel and Side View); the Side View's
  More Actions row shows the same ⌘, Kbd.
- Save AI Key and Send Feedback keep **no** binding — inventing keys just to fill the menu would
  burn modifier space for actions nobody runs often; a binding-less row shows no Kbd.

Consequently the cost noted above is reversed: the About card now shows ⌘, on its one bound row,
and ⌘, is taken platform-wide.

## Cost

- ⌘⇧K enters the modifier space: the keymap is platform-owned (ADR-0014), so extensions and
  future sub-apps must treat About as taken.
- Discoverability leans on the chip tooltip and the README key table; the About card itself shows
  the Kbd on the rows that have bindings (⌘, — see the amendment).