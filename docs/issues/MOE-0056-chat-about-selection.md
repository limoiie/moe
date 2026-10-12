# M10b: chat about the selected text

- **ID**: MOE-0056
- **State**: done
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-13
- **Commit**: dd703f7

Requested from use: "I need a Ask selection or something like that for chat with agent about the selected
text."

The mechanism already existed and was tested — the AI extension's capture row attaches the selection to
the ask (ADR-0019/0021), and `panel_ask` puts it in the first request as context. What did not exist was
a way to *reach* it: the capture row is only offered when the query matches **no** command
(`Registry::search`'s fallback step), and the empty-query branch never consults it at all. So with text
selected and an empty query there was no path to a chat about it, and with a query you had to type
something that matched nothing to be offered the ask — the opposite of discoverable.

## Decision

- **An ordinary command, not a new platform hook.** `ai.chat-about-selection` declares
  `InputKind::Selection`, which the platform already captures when the panel is summoned (ADR-0006/0021)
  and already passes to `Extension::invoke`. It is therefore searchable by name, present on the root page
  like any other command, and needs no registry change.
- **Its Apply starts the conversation**: the same `panel_ask` the capture row runs, with a neutral opening
  question ("What should I know about the selected text?") and the selection as the first request's
  context. The opening reply streams into the panel and the user continues from there.
- **Pressing the command is the consent to send**, exactly as Enter on the capture row is. Nothing is sent
  without a deliberate action, which is the line that keeps this from being presumptuous.
- **No selection degrades to the blank page** rather than erroring (ADR-0006's Selection fallback): the
  destination is still the right one and the user can type instead of being told off. This is also what
  makes the command testable — the capture path needs a configured provider.
- **It routes through `invoke_streaming`** as well as `invoke`: with a `NoopEmitter` the opening reply
  would be persisted but never delivered, leaving the page showing a question with no answer until reload.

## Delivery

- `crates/moe-extensions/src/ai.rs` — the command, `chat_about_selection`, and its two routing arms.

## Acceptance

- [x] The command is registered with `InputKind::Selection` and is not live (it opens a conversation; it
      does not re-run one).
- [x] With no selection it returns the blank Quick Ask page rather than an error.
- [x] It routes through both the plain and the streaming invoke paths, so the opening reply is delivered
      as it is produced.
- [x] 123 Rust tests (was 122), 66 UI tests, clippy green.
- [ ] **Not verified on screen.** The selection capture, the opening reply streaming into the panel, and
      the conversation continuing are the parts a manual pass confirms — and the capture path itself is
      not unit-testable without a configured provider, which is why the test stops at the degradation case.

## Comments

- 2026-10-12 (agent): filed and closed in one pass. The design question worth recording: this could have
  been a registry change (offer the capture row on an empty query when a selection exists, pinned at the
  top). The command is smaller, needs no new section or header in the ten-row grid, and is discoverable by
  name — but it is *not* pinned when a selection is present, which is the one thing the registry route
  would have given. If the command turns out to be hard to find in practice, that is the follow-up.
- 2026-10-12 (agent): a keybinding was deliberately not added. Every plausible chord is either taken or an
  OS/edit convention (ADR-0031), and a command that is searchable by name does not need one to be usable.
- 2026-10-13 (agent): **superseded — the command is removed (MOE-0058).** Reported from use as useless,
  and the follow-up the first comment anticipated is the one that landed: what the command made reachable
  is now visible where the conversation starts — the Quick Ask composer shows the selection its first
  message will carry, and hovering it peeks the whole text (MOE-0059). The mechanism this ticket built is
  untouched (`panel_ask` and the capture row carry the selection exactly as before); only the
  command-shaped door to it is gone, because ⌘' and the page already are one.
