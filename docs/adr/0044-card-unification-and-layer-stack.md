# One card component, and an explicit layer stack

Five card surfaces exist. Three use `createCard` — the panel's actions card
(`actionsCard`, `ui/src/main.ts`), the Side View's More Actions card (`actionsCard`, `ui/src/chat.ts`), and the
model switcher (`ensureCard`, `ui/src/models.ts`). Two are hand-rolled: the About card has its own row
builder, renderer and key handling (`aboutRowEl`, `renderAboutCard`, `main.ts`), and the Side View's
history card has its own open/close, rows and Esc handling (`openHistoryCard`, `closeHistoryCard`, `conversationRow`, `chat.ts`).
ADR-0026 deferred unification "until a third card needs it"; ADR-0043's Filter Card is the sixth.
Meanwhile `createCard` cannot host the About card at all, because `runRow` always closes before
running (`runRow`, `ui/src/card.ts`) and the About card's theme rows must stay open (`runAboutTheme`,
`main.ts`, per ADR-0035's amendment).

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
  card hand-rolls — `onQuery` for server-backed per-keystroke replacement (`onQuery`, `card.ts`), which
  the model switcher uses (`onQuery`, `models.ts`) — and its 80ms debounce (`chat.ts`) is that
  pattern. Precondition: `conversationRow`'s visuals must be expressible as `CardRow`
  (`title`/`icon`/`trailing`/`keys`); if one is not, extend `CardRow` rather than keep the
  duplicate. This removes the last copy of the input discipline ADR-0031's Cost warns "must be copied
  into every new list-like overlay".
- **An explicit layer stack replaces the if-chain.** Overlay precedence is a chain of early-returning
  if-statements maintained in three places at once: the `cardOpen` guard
  `actionsCard.isOpen() || aboutCardOpen || modelSwitcher.isOpen()` (`cardOpen`, `main.ts`), the `back()`
  chain (`back()`, `main.ts`), and a window-level Esc check (`aboutCardOpen`, `main.ts`). Adding the Filter
  Card means editing all three correctly, every time. A small ordered stack with a `top()` makes
  `cardOpen` derived and `back()` a single pop.
- **Precedence above the cards is unchanged, and is this refactor's hard constraint.** The
  stop-confirm dialog owns every key while open (`confirmOpen`, `main.ts`), attach mode comes next
  (`attaching`, `main.ts`), and a focused `pending` item's stop-confirm sits above the cards in `back()`
  (`openStopConfirm`, `main.ts`). Those are not cards and do not join the stack; the stack orders cards only. This
  is the main regression risk of the change, and the reason it gets its own ADR rather than riding
  along in ADR-0043.
- **`createCard` keeps owning key discipline** — `stopPropagation` for every consumed key, and
  empty-`⌫` forwarded as dismiss (`card.ts`) — so the stack never has to reason about which
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

## Amendment: the history card stays hand-rolled (MOE-0049)

The Cost above says `CardRow`'s optional fields "will keep growing; the discipline is that a field earns
its place by being needed on at least two surfaces". MOE-0049 was the ticket where that discipline
either held or got amended, and **it held**: the history card stays hand-rolled.

What the migration turned out to need was two capabilities, neither of which has a second consumer:
`CardRow.id`, so the card's focused row can be mapped back to a conversation (the model switcher marks
its active model with `checked` and rebuilds rows per query — it would not read one), and a **focus
setter**, because ⌃[ / ⌃] move the card's own highlight and `Card` can only report focus through
`focused()`. The third the ticket listed, `filteredEmptyText`, already landed with the Filter Card.

So the price is paid and stated: the Side View keeps its own Esc/empty-⌫ handler and its own
rows/sections rendering, which is the duplication this ADR was written to end, and adding another
list-like card there would still cost a hand-rolled key path. **The revisit trigger** is a second
consumer for `CardRow.id` or a card that needs programmatic focus; either makes the migration correct
again, as its own ticket.

## Amendment: the stack is a module, and it found a live drift

Built as `ui/src/layers.ts` — `createLayers()` returns `register` / `top()` / `anyOpen()` /
`closeTop()`, with nothing DOM-aware in it. **Registration order is front-most-LAST**, which is also
the order the anchors stack, so the list reads the way the screen looks. The panel registers its
actions card, about card and model switcher; the Side View registers the same three, its order being
its old if-chain's so that window behaves identically.

The predicted failure mode had already happened, and only the refactor made it visible: **the Side
View's `back()` closed the model switcher and the panel's did not.** In the panel, Esc with the
switcher open and the focus elsewhere (right after click-running a row) backed out of the page
instead of closing the switcher — while the key guard still refused to let empty Backspace through,
because that guard *did* list the switcher. Two of the three sites disagreed about the same overlay.
The stack closes it because it is registered, which is the argument for this ADR demonstrated on the
day it landed.

Two boundaries recorded, because they are what keeps the change honest:

- **The modal states are not layers.** The stop-confirm dialog, attachment mode and a streaming
  item's stop-confirm sit above the cards and keep their own precedence, exactly as the Decision
  said. They never coexist with a card, and their keys are handled before the stack is consulted.
- **Click-outside was deliberately left alone.** The three sites this ADR named are key-routing
  sites; click-outside is not one. Each card is dismissed by a different owning element — the action
  pill for the actions card, the chip for About, the history button for history — so a uniform stack
  op would need an ownership predicate. Adding a card therefore still costs one registration *plus* a
  click-outside line if it needs pointer dismissal; the acceptance criterion is written to say
  "`back()` and the key guard" rather than overreaching.
