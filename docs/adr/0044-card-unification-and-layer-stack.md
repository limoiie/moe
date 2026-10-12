# One card component, and an explicit layer stack

Five card surfaces exist. Three use `createCard` — the panel's actions card
(`ui/src/main.ts:126-136`), the Side View's More Actions card (`ui/src/chat.ts:559-564`), and the
model switcher (`ui/src/models.ts:349-365`). Two are hand-rolled: the About card has its own row
builder, renderer and key handling (`main.ts:1856`, `:1882-1909`, `:1948-1970`), and the Side View's
history card has its own open/close, rows and Esc handling (`chat.ts:213-230`, `:304`, `:450-459`).
ADR-0026 deferred unification "until a third card needs it"; ADR-0043's Filter Card is the sixth.
Meanwhile `createCard` cannot host the About card at all, because `runRow` always closes before
running (`ui/src/card.ts:226-230`) and the About card's theme rows must stay open (`runAboutTheme`,
`main.ts:1823-1837`, per ADR-0035's amendment).

## Decision

- **`CardRow.keepOpen`.** A row declaring it runs without closing the card. Row-level, not a
  card-level predicate: it is the *row* that knows ("System / Light / Dark stay open so a second
  theme can be tried"; "Open Config File" closes), and a card-level callback would push row
  knowledge back into the owner — the coupling `createCard` exists to remove. The Filter Card does
  not need it; single-select closes on choose.
- **The About card migrates onto `createCard`.** ADR-0028 already anticipated this: adopting
  `createCard` "fulfils rather than violates" ADR-0026's deferral. Its three sections (App / Theme /
  Support) map onto `CardSection` unchanged, its trailing check onto `CardRow.checked`, and its theme
  rows onto `keepOpen`.
- **The Side View's history card migrates too.** `createCard` already has the exact mechanism that
  card hand-rolls — `onQuery` for server-backed per-keystroke replacement (`card.ts:50-56`), which
  the model switcher uses (`models.ts:354`) — and its 80ms debounce (`chat.ts:425-428`) is that
  pattern. Precondition: `conversationRow`'s visuals must be expressible as `CardRow`
  (`title`/`icon`/`trailing`/`keys`); if one is not, extend `CardRow` rather than keep the
  duplicate. This removes the last copy of the input discipline ADR-0031's Cost warns "must be copied
  into every new list-like overlay".
- **An explicit layer stack replaces the if-chain.** Overlay precedence is a chain of early-returning
  if-statements maintained in three places at once: the `cardOpen` guard
  `actionsCard.isOpen() || aboutCardOpen || modelSwitcher.isOpen()` (`main.ts:2386`), the `back()`
  chain (`main.ts:2057-2113`), and a window-level Esc check (`main.ts:2301-2307`). Adding the Filter
  Card means editing all three correctly, every time. A small ordered stack with a `top()` makes
  `cardOpen` derived and `back()` a single pop.
- **Precedence above the cards is unchanged, and is this refactor's hard constraint.** The
  stop-confirm dialog owns every key while open (`main.ts:2267-2277`), attach mode comes next
  (`:2290-2300`), and a focused `pending` item's stop-confirm sits above the cards in `back()`
  (`:2085-2088`). Those are not cards and do not join the stack; the stack orders cards only. This
  is the main regression risk of the change, and the reason it gets its own ADR rather than riding
  along in ADR-0043.
- **`createCard` keeps owning key discipline** — `stopPropagation` for every consumed key, and
  empty-`⌫` forwarded as dismiss (`card.ts:282-290`) — so the stack never has to reason about which
  element saw a key first.

## Cost

- The layer-stack refactor touches the panel's whole key-routing path. Behavior must be identical;
  the only visible change is that a new overlay no longer requires three coordinated edits.
- Migrating the About card deletes a renderer ADR-0026 deliberately kept, so that ADR's Cost note is
  superseded rather than honored — recorded there by amendment.
- One component now serves six surfaces with genuinely different content (actions, meta actions,
  conversations, models, filter values). `CardRow`'s optional fields will keep growing; the
  discipline is that a field earns its place by being needed on at least two surfaces.
- `keepOpen` makes "does the card close?" a per-row property, so a reader can no longer assume that
  selecting a row dismisses the card. Only the About card's theme rows use it today.
