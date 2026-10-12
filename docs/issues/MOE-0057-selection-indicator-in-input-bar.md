# M10c: the Input Bar says whether the selection was captured

- **ID**: MOE-0057
- **State**: done
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-13
- **Updated**: 2026-10-13
- **Commit**: c1d4f81

Requested from use: "for the command which accepts selected text or as its first param, can we place an
indicator in the input so that I can know whether I selected or not."

A selection command (`InputKind::Selection`) runs on whatever the summon captured — text, Finder files,
or nothing. The platform captures it before the panel shows (ADR-0002/0021) and hands it to the
Extension, but the panel itself never sees it: `AppState.selection` had no IPC, so at the one moment the
answer matters — the command focused, before ⏎ — it was invisible. Running without a selection is how
`ai-commands` ends up showing its "First select the text to transform" guidance card: the platform said
so only after the fact.

## Decision

- **Shape-only IPC.** `selection_state` returns the captured selection's *shape* — character count,
  file count, and whether the clipboard fallback is still reading — never the text. The panel only ever
  needed to know *whether* something was captured; the text stays between the platform and the
  Extension, which is the boundary the selection is deliberately kept behind.
- **The indicator speaks for the focused command on the command layer.** It is shown only when the
  focused row declares `input: "selection"`, so the chip answers "will ⏎ use my selection?" exactly
  where that is actionable. A result page has already consumed the selection (ADR-0038 gives that page
  its own input) and the chat page's bar is a title, so neither shows it.
- **Three states, because "not yet known" is not "nothing".** The clipboard fallback (MOE-0013) settles
  *after* the panel is up; with two states the chip would say "No selection" and then flip — the flicker
  MOE-0055 taught the panel to avoid. Pending reads "Reading selection…", and the backend emits
  `selection-changed` on every show and again when a late capture settles (generation-guarded, so a
  stale worker cannot repaint a newer summon's state).
- **One material with the Filter Control.** Same house glass, same `align-self: stretch` height rule
  (ADR-0043's height invariant: the measured panel height must not move), and the same accent hairline
  for "this is set" that an active filter value wears. It is status, not a control: no hover, no
  pointer, no focus. It sits to the *right* of the Filter Control so that control never moves.

## Delivery

- `crates/moe-app/src/main.rs` — `SelectionStatePayload` (+ its `of` derivation), the
  `selection_pending` flag (read and written under the selection lock, like `selection_generation`),
  the `selection_state` command, and the two `selection-changed` emits.
- `ui/src/selection.ts` + `ui/src/selection.test.ts` — the indicator's rules (which command, which
  state, what each label and tooltip says), ten tests.
- `ui/src/main.ts` — `selectionState` (read once at boot, kept current by the event), the renderer, and
  the `render()` hook beside `renderFilterControl`.
- `ui/index.html` + `ui/src/styles.css` — the chip in `#query-row` and `.moe-selection-state`.
- `ui/src/icons.ts` — `text-cursor-input` (fixed use), the chip's glyph.
- `docs/adr/0034-input-bar-identity.md` — the amendment that records all of the above.

## Acceptance

- [x] Rust: the payload's shape (chars, files, pending) and its camelCase wire names are pinned by
      tests — the mirror the panel reads is a tested contract, not a convention.
- [x] TS: the indicator's rules are pinned by ten tests — which commands get a chip, each state's
      label and tooltip, and the three shapes a capture can have (text, files, both).
- [x] `just test` green (257 Rust tests, including the two new ones), `just ui-test` 76 (was 66),
      `tsc` and `vite build` green.
- [ ] The live pass: focused on a selection command with text selected, the chip reads "Selection" and
      its tooltip carries the character count; with nothing selected it reads "No selection"; arrowing
      on and off the command shows and hides it without the panel resizing.
- [ ] The live pass: a clipboard-fallback capture (an AX-opaque app) goes "Reading selection…" to the
      settled state with no "No selection" flip in between.

## Comments

- 2026-10-13 (agent): filed and built in one pass. The one thing reasoning had to carry is the height
  invariant — jsdom does not lay out, so "the chip cannot grow the row" rests on the Filter Control's
  own rule (`align-self: stretch`, 12px content against a ~24px row band) rather than on a test, and
  the live pass is what confirms it.
