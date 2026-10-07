# Front-most list owns navigation keys; modifier-correct key routing

Four user-reported issues shared one root cause: key routing had no notion of "which list is in
front", and modifier matching was too loose.

1. `⌃N`/`⌃P` moved the root list even while a floating card (actions / About / Side View history)
   was open and its rows were the ones with focus.
2. Backspace inside a card's filter closed the card whenever the panel's input bar happened to be
   empty — the window-level "empty Backspace = Back" handler read the *panel* input's value, not
   the card's.
3. Root rows always showed declared shortcut Kbds, adding noise to the resting list.
4. `⌃K` popped the actions card on macOS, stealing the system's kill-line editing key.

## Decision

- **The front-most list owns the keyboard while it is focused.** A card's filter input handles
  `↑↓` and `⌃N`/`⌃P` itself and calls `stopPropagation()` for every key it consumes — including
  Backspace, which it forwards only when the filter is empty (`⌫` = dismiss). The window-level
  handlers add a second guard (`cardOpen`) so the list *below the card* can never react to keys
  whose event happened to start elsewhere on the surface. Every surface follows this rule: the
  panel's actions card, the About card, and the Side View's history card and actions card.
- **`⌃N`/`⌃P` navigate every list, uniformly.** The Side View history card accepts them exactly
  like the panel does. This deliberately overrides the native macOS next/previous-line editing
  bindings, matching the shared keymap table (ADR-0014) — consistency of the navigation slot
  beats the platform editing convention here (product decision).
- **Modifiers are matched strictly.** The actions card opens on `⌘K` only; `⌃K` falls through to
  the webview's native macOS kill-line. `⌘⇧P` remains the alternate binding.
- **Root row shortcuts reveal on focus/hover.** ADR-0030's shortcut Kbd keeps its place in the
  row anatomy but renders only while the row is focused (keyboard) or hovered (mouse), via the
  `.moe-row-keys` class; the resting list stays calm. Hover never steals focus (the standing
  focus/hover rule).

## Cost

- The input discipline (stop propagation for consumed keys) must be copied into every new
  list-like overlay; the `cardOpen` guard at each surface is the safety net when focus sits
  outside the overlay. `createCard` (ADR-0028) carries the discipline once, so cards built on it
  get this for free.
- `⌃N`/`⌃P` never reach the native line-navigation editing bindings in Moe inputs — accepted
  above.
- Root rows hide declared shortcuts at rest, so discoverability of e.g. `⌘,` leans on the Kbd
  appearing as soon as a row is focused — the row is focused on open, so the first row always
  shows its shortcut.