# M9m: one card component — the About and history cards stop being hand-rolled

- **ID**: MOE-0041
- **State**: todo
- **Labels**: ready-for-agent, refactor, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Blocks**: MOE-0042, MOE-0043

Five card surfaces exist. Three use `createCard` — the panel's actions card (`main.ts:126-136`), the
Side View's More Actions card (`chat.ts:559-564`), the model switcher (`models.ts:349-365`). Two are
hand-rolled: the About card has its own row builder, renderer and key handling (`main.ts:1856`,
`:1882-1909`, `:1948-1970`), and the Side View's history card has its own open/close, rows and Esc
handling (`chat.ts:213-230`, `:304`, `:450-459`). ADR-0026 deferred unification "until a third card
needs it"; ADR-0043's Filter Card is the sixth.

`createCard` cannot host the About card today: `runRow` always closes before running
(`card.ts:226-230`), and the About card's theme rows must stay open (`runAboutTheme`,
`main.ts:1823-1837`, per ADR-0035's amendment).

## Decision

- **`CardRow.keepOpen`** — a row declaring it runs without closing the card. Row-level, not a
  card-level predicate: it is the *row* that knows ("System / Light / Dark stay open so a second
  theme can be tried"; "Open Config File" closes), and a card-level callback would push row knowledge
  back into the owner — the coupling `createCard` exists to remove.
- **The About card migrates.** ADR-0028 already anticipated this: adopting `createCard` "fulfils
  rather than violates" ADR-0026's deferral. App / Theme / Support map onto `CardSection` unchanged,
  the trailing check onto the existing `CardRow.checked`, the theme rows onto `keepOpen`.
- **The history card migrates too.** `createCard` already has the exact mechanism it hand-rolls —
  `onQuery` for server-backed per-keystroke replacement (`card.ts:50-56`), which the model switcher
  uses (`models.ts:354`) — and its 80ms debounce (`chat.ts:425-428`) is that pattern. Precondition:
  `conversationRow`'s visuals must be expressible as `CardRow` (`title`/`icon`/`trailing`/`keys`); if
  one is not, **extend `CardRow` rather than keep the duplicate**.
- This removes the last copy of the input discipline ADR-0031's Cost warns "must be copied into every
  new list-like overlay".

## Delivery

- `ui/src/card.ts` — `keepOpen`.
- `ui/src/main.ts` — delete `aboutRowEl`, `renderAboutCard` and the About key handler; wire the
  About card through `createCard`.
- `ui/src/chat.ts` — delete the hand-rolled history open/close and Esc paths; wire through
  `createCard` with `onQuery`.
- `ui/index.html`, `ui/chat.html` — the two cards' markup joins the shared structure.

## Acceptance

- About: choosing System / Light / Dark keeps the card open and its check follows the new
  preference; every other About row closes it. Sections, filter, ↑↓/⏎, Esc / empty-⌫ and
  click-outside behave exactly as before.
- History: per-keystroke server search still works with its debounce, and ⌃[ / ⌃] still step
  conversations per the *filtered* list (`chat.ts:917`).
- No hand-rolled Esc / empty-⌫ handling remains for either surface outside `createCard`.
- `CardRow` gains no field that only one surface uses (ADR-0044's discipline: a field earns its place
  on at least two surfaces — `keepOpen` is About today, Filter-adjacent multi-select later).
- `just ui-build` green, and no behavior change observable to a user.

## Comments

- 2026-10-12 (agent): filed from the Raycast primitives grilling session; ADR-0044 records the
  decision and amends ADR-0026's deferral. Sequenced before MOE-0042 (the layer stack should order
  unified cards, not special cases) and MOE-0043 (the Filter Card is built on `createCard`, not as a
  sixth hand-rolled surface).
