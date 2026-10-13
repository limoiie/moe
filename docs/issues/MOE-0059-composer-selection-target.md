# M10e: the Quick Chat composer shows the selection it will carry

- **ID**: MOE-0059
- **State**: done
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-13
- **Updated**: 2026-10-13
- **Commit**: 138c977

Requested from use: "add the selection chip to the quick chat's composer properly (above, inner, or
where? how?), so that I can know my chat will have a target. I want the selection to be well presented,
so that the view is clean, and when I want to know the whole text, I can hover to peek as well."

A conversation's first message carries the captured selection — text as context, Finder files as
attachments (ADR-0019/0021) — and the page never said so. Entering Quick Ask with a paragraph selected
looked exactly like entering it with nothing selected, and the only way to find out was to send and read
the answer.

## Decision

- **Above the composer row, not inside it.** The chip sits in its own row between the message log and
  the composer pill, its left edge on the **input capsule's** — the add button's column plus the
  composer row's own gap — so it reads as context for the message the capsule will send rather than as
  a sibling of the attach button. Inside the capsule it would eat the draft's width and compete with
  the attach button; above, with the same 8px under it as the composer has below it, it reads as
  *context for the message about to be sent*, which is what it is. It wears the Input Bar chips' own
  material (house glass, chip shadow) at the composer's scale, so the three chips in the app are one
  family.
- **Armed only while the conversation is blank.** ADR-0019/0021 give the captured context to the
  conversation's **first message only**, so the chip lives on a blank page with no send in flight: the
  first send consumes it, a rolled-back send returns it (the selection never made it into a message),
  and a started or reopened conversation never shows it. No "No selection" state here — unlike the Input
  Bar's indicator, because a command may *depend* on a selection while a chat never does.
- **The chip describes, the peek shows.** The chip names the target and its size ("Selection" ·
  "1,234 characters", or "2 files" when only Finder files were captured). Hovering opens a card above
  it — the actions/About card language, a reading column that scrolls rather than growing — with the
  full selected text and the captured Finder paths. That is the one place the selection's *text* crosses
  into the webview, and it is fetched on demand (`selection_detail`, once per reported state) rather
  than at boot: the chips still render from `selection_state`'s shape alone.
- **No native tooltip on the chip.** A `title` tooltip would appear over the peek card and fight it, so
  the card's caption carries the explanation ("The next message carries the selected text as context.").
- **The rules live in `ui/src/selection.ts`** beside the Input Bar indicator's, so the two chips cannot
  disagree about what was captured or what to call it; nine more tests pin them.

## Delivery

- `crates/moe-app/src/main.rs` — the `selection_detail` command (the contract's own `Selection`) and a
  test pinning its wire shape.
- `ui/src/selection.ts` + `ui/src/selection.test.ts` — `chatTarget`, the `detail` and `icon` fields on
  the shared indicator, and the nine tests.
- `ui/index.html` + `ui/src/styles.css` — the target row, the chip, and `.moe-peek-card`.
- `ui/src/main.ts` — the renderer, the on-demand fetch (keyed by the reported state), the hover open /
  delayed close, and the hooks: `render()`, `selection-changed`, and the send's flag flips.
- `docs/adr/0036-quick-ask-is-a-panel-conversation-page.md` — the amendment.

## Acceptance

- [x] TS: the rules are pinned — the chip is off every page but the chat, off a started conversation and
      off a send in flight, absent when nothing was captured, and each state's label, detail and icon.
- [x] Rust: `selection_detail`'s shape (text + absolute paths, camelCase) is pinned.
- [x] `just test` green (257 Rust tests), `just ui-test` 85 (was 76), `clippy`, `tsc`, `vite build`
      green.
- [ ] The live pass: on a blank Quick Ask page with text selected, the chip reads "Selection · N
      characters"; hovering opens the card with the whole text; sending hides it; a failed send brings
      it back; a reopened conversation never shows it.
- [ ] The live pass: the peek card's placement (above the composer, over the log's tail) reads right,
      and a long selection scrolls inside the card rather than growing it.

## Comments

- 2026-10-13 (agent): filed and closed with the command's removal (MOE-0058) — the selection's home is
  the page, not a command. Two things reasoning carries rather than tests: the card's placement (jsdom
  does not lay out) and the fetch-on-demand rule (the text crossing into the webview is an IPC the tests
  do not exercise end to end).
- 2026-10-13 (agent): known nuance, not handled here — the AI context truncates a selection past 4,000
  characters (`SELECTION_LIMIT`) with a note inside the message, while the peek shows the whole text.
  The card says what the message carries, not what the model finally reads; if that gap matters, the
  caption is where to say so, and the limit belongs to the Extension, not to the platform's payload.
- 2026-10-13 (owner, from use): the chip must align with the **input**, not the add-attachment button,
  and the space between chip and input must equal the space under the input. Both are now structural
  rather than eyeballed: a slot of the button's own column (`--moe-row-h`) plus the composer row's gap
  puts the chip's left edge on the capsule's, and the row's 8px matches the card's `pb-2` below the
  composer. The row is `flex` for the same reason — a block row would add its line box's descender to
  that 8px.
