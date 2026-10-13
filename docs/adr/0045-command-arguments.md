# Command arguments: a positional chip in the Input Bar

Raycast's commands can declare **arguments**: a command that needs a parameter before it can run shows
it in the search bar, *before* the query text, and `Tab` focuses it. Moe's AI Commands had the same
need and no way to express it. Translate was two commands ("Translate to English" / "Translate to
Chinese") whose only difference is one parameter, and with both on the root page neither was the
obvious one to press — the honest default for someone who writes in both languages (translate into the
*other* one) existed in neither.

## Decision

- **The Extension declares, the panel renders (ADR-0003/0006).** `CommandMeta.arguments` carries the
  declaration: an id, a title, an optional default, and a bounded option list. The panel owns the
  chip, the picker, `Tab`, the session memory and the wire; the Extension only reads the value it is
  handed on invoke. An argument with no options is a declaration error the panel drops silently — a
  control with nothing to choose would be a control that lies.
- **The value is an invocation input, not a query.** `Extension::invoke` / `invoke_streaming` take
  `&ArgumentValues` beside the query and the selection: the third launch input, keyed by argument id.
  Empty is the honest default — a command that declares none never sees one, and a caller that sends
  none (a re-run, another surface) leaves the Extension on its own default.
- **The panel sends the effective value, always.** The chip's value resolves as *remembered → declared
  default → first option*, and that resolution is what travels on invoke, so the Extension never has
  to guess which surface ran it or whether the user touched the chip. The rules live in
  `ui/src/arguments.ts` (tested); the memory is a session `Map` keyed by command id, like the
  filter's — never disk, so a restart shows the declared defaults again.
- **`Tab` is the argument's key** — Raycast's own binding. It focuses the focused command's first
  argument and cycles; with one declared argument it keeps the picker that is already open, because
  Tab must never clear what the user typed into its filter. The picker is the house card (ADR-0044):
  a searchable list with its input at the bottom edge, single-select, peers with the other cards —
  and the filter it keeps is the *card's* input, never the panel's search box.
- **Translate becomes one command with one argument.** `aicmd.translate` declares `language` (default
  `auto` → "Zh ↔ En", plus `en` / `zh`), and the auto instruction tells the model to detect the text's
  own language and translate into the other. Anything the extension does not recognize — including
  nothing sent — reads as auto: a wrong explicit target would translate a text into its own language,
  which is the one failure the argument exists to prevent.
- **A re-run carries the arguments.** `{Command} Again` is the same command with the same settings and
  new text, so the panel sends the values with the item action the re-run comes from (ADR-0024's
  refine loop): without it, "Translate to Chinese Again" would fall back to auto and translate the
  Chinese result back into English.

## Cost

- The chip is the one Input Bar element that moves the text column (ADR-0034's alignment invariant is
  about the bar *at rest*): the argument is part of the input's content, so the placeholder shifts the
  way typed text would. It is `align-self: stretch`, and its container is hidden when empty so the
  row's gap never counts an empty slot — the row band and the measured panel height still cannot move.
- `CommandMeta` gained a field and `Extension::invoke` a parameter, so every Extension and every test
  double carries them (28 command literals, 20 invoke impls, enumerated by the compiler). That is the
  price of one contract instead of a Translate-shaped special case.
- Only bounded choice lists are implemented: a free-text argument (Raycast's `text` type) has no
  declaration form yet, and no argument value is persisted to disk.
- `Tab` now means "the argument" wherever the focused command declares one. On every other page it
  keeps its browser meaning — in a palette that is "move focus out of the input", a pre-existing
  accident this ADR deliberately leaves alone.
