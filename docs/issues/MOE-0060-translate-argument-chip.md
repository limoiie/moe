# M10f: Translate auto-detects, and the Input Bar grows argument chips

- **ID**: MOE-0060
- **State**: done
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-13
- **Updated**: 2026-10-13
- **Commit**: d5735f7

Requested from use: "by default, the translate command should be able to translate between english and
chinese automatically — if the target text is in Chinese, convert it into English, vice versa.
additionally, there is a positional param chip shows at the beginning of the input when the translate
command is focused. user can press tab to focus this chip, which will pop up a drop down menu for
selecting the target language. by default, this chip shows Zh <-> En, indicating auto translate. and,
such menu always searching or keeping the input even if there is no match."

Translate was two commands (`Translate to English` / `Translate to Chinese`) whose only difference is
one parameter, and with both on the root page neither was the obvious one to press — the honest default
for someone who writes in both languages (translate into the *other* one) existed in neither. The
parameter had nowhere to live in the contract: a command's only launch inputs were the query text and
the captured selection.

## Decision

- **One `Translate`, one `language` argument** (default `auto` → "Zh ↔ En", plus `en` / `zh`). The
  auto instruction detects the text's own language and translates into the other; the two explicit
  directions stay one keystroke away instead of being two commands. Anything unrecognized — including
  nothing sent — reads as auto: a wrong explicit target would translate a text into its own language.
- **Command arguments are a contract, not a Translate special case** (ADR-0045): the Extension
  declares them on `CommandMeta`, the panel renders the chip and the picker, and the value travels
  back on invoke as `ArgumentValues`. `Tab` focuses the chip (Raycast's binding); the picker is the
  house card, searchable, single-select, peers with the Filter Card.
- **The picker keeps its input** when nothing matches: `createCard` shows its filtered-empty line and
  leaves the typed text alone, and it is the *card's* input — the panel's search box is never touched
  by an argument's filter.
- **A re-run carries the arguments**: the panel sends them with the item action the refine loop runs
  from, so "{Command} Again" is the same command with the same settings and new text.

## Delivery

- `crates/moe-core/src/contract.rs` — `ArgumentOption` / `CommandArgument` / `ArgumentValues`, the
  `CommandMeta.arguments` field, and the `invoke` / `invoke_streaming` parameter.
- `crates/moe-core/src/registry.rs`, `crates/moe-app/src/main.rs` — the plumbing: the registry passes
  the values through, `invoke_command` and `run_item_action` accept them, and the refine loop
  re-invokes with them.
- `crates/moe-extensions/src/ai_commands.rs` — the merged Translate, its argument declaration, and
  `instruction_for` (three instructions, auto as the fallback).
- `ui/src/types.ts` — the mirror, with `arguments?` optional (the Rust side omits an empty vector).
- `ui/src/arguments.ts` + `ui/src/arguments.test.ts` — the effective-value rule and the values one
  invocation carries, eleven tests.
- `ui/index.html`, `ui/src/styles.css`, `ui/src/main.ts` — the chip, the picker, `Tab`, the session
  memory, and the invoke paths that send the values.

## Acceptance

- [x] Rust: the declaration is pinned (only Translate declares one; the default names the auto option;
      every option is named), and the instruction resolution is pinned (auto / en / zh / unknown /
      nothing sent / another command ignoring it).
- [x] TS: the effective-value rule is pinned — remembered wins, a stale remembered id degrades to the
      declaration, no default falls to the first option, an empty list yields no chip.
- [x] `just test` green (260 Rust tests), `just ui-test` 96 (was 85), `clippy`, `tsc`, `vite build`
      green.
- [ ] The live pass: with the Translate row focused, the chip reads "Zh ↔ En" at the input's beginning;
      Tab opens the picker; picking "English" turns the chip into "English" and the run translates into
      English; the picker's filter keeps what is typed when nothing matches.
- [ ] The live pass: with Chinese selected and the chip left on auto, the run produces English (and the
      other way round); "{Command} Again" keeps the chosen direction.

## Comments

- 2026-10-13 (agent): filed and closed in one pass, with ADR-0045 recording the contract. The wide part
  is the contract change itself — 28 `CommandMeta` literals and 20 `invoke` impls, enumerated by the
  compiler and done mechanically — which is why the ticket is one slice: the chip, the picker, `Tab`,
  the wire and the translate behavior are only demonstrable together.
- 2026-10-13 (agent): two things the tests do not cover, both named in the acceptance above — the
  chip's geometry (jsdom does not lay out; the height rule is the trailing chips' `align-self: stretch`
  construction) and the end-to-end translate (it needs a configured provider).
