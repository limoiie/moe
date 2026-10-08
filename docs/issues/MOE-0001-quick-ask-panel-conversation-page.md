# M7i: Quick Ask v2 — the panel's conversation page

- **ID**: MOE-0001
- **State**: in-progress
- **Labels**: feature, ux
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent
- **Commit**: (pending)

User feedback round: Quick Ask becomes a real conversation page inside the panel, instead of
"the search text becomes the question". The old shape broke down as soon as a conversation needed a
second turn — the search text silently became content (typing `quick` + Enter sent "quick" to the
model), there was no way to keep chatting in the panel, and the answer card's capabilities
(write-back / ⌥⏎ copy / ⌘J side view) had nowhere to live once Enter meant *send*.

## Requirements

1. **Enter on a matched row opens the page; only captures send text.** Typing text searches commands;
   Enter on the matched **Quick Ask** row must enter the quick chat page, not send the search text to
   the model. The input text becomes a question **only** through the no-match capture row
   ("Ask \"…\"").
2. In the quick-ask page:
   1. Enter sends the input into the conversation and clears it;
   2. while generating, **Enter** is the stop-generation command (not Esc);
   3. Back (Esc / empty ⌫) during generation asks first with a confirmation dialog;
   4. a **Remove Chat** action — the same deletion as the chat history page;
   5. continuous chatting (one conversation, multiple turns);
   6. a **New Chat** action for a blank chat;
   7. **⌃[ / ⌃]** step through chat history, like the Side View.

## Design — ADR-0036

- The panel enforces `CommandMeta.input`: rows declaring `Query` receive the Input Bar text on Apply
  (the capture row is the canonical case); Live rows take the input over; `None`/`Selection` rows get
  no query. `ai.quick-ask`'s matched row is `None`; its `fallback_command` capture row stays `Query`.
- `ActionResult::Conversation { conversation_id }` declares the page (None = blank new chat);
  the platform renders a message stream in the detail pane and turns the Input Bar into the composer.
- `Extension::panel_continue(conversation_id, message, selection, emitter)` (IPC `panel_send`),
  parallel to `side_continue`/`side_send`; the whole history is the model context, replies stream as
  `ai.quick-ask` frames. The panel captured selection (text + Finder files) is context for the first
  message only; attachments-only input sends the generic request sentence (IIE4AD-391's rule).
- Page keys: Enter sends / stops; Back during generation opens the confirmation dialog
  (Stop Generation ⏎ / Keep Generating Esc); a draft clears first, then Back returns to the root.
- Record surface: ⌘N / New Chat → blank page (`ai.new-chat`); ⌃X / ⌃⇧X → the AI delete hooks
  (`delete_item` / `delete_all` with `ai.search-history`, ADR-0022); ⌃[ / ⌃] → `side_conversations`;
  ⌘P → the history list; ⌘J → Side View; actions card: New Chat / Remove Chat / Open in Side View /
  Copy Last Answer (⌥⏎) / Write Back Last Answer.

## Delivery

- `crates/moe-core`: `ActionResult::Conversation`, `Extension::panel_continue`,
  `Registry::panel_continue` (routed by the page's command) + tests.
- `crates/moe-app`: `panel_send` IPC (passes `AppState.selection`).
- `crates/moe-extensions/ai.rs`: one `panel_ask` core for `panel_continue` (empty id = new
  conversation) and the capture path (`quick_ask`); `ai.new-chat` returns the blank page;
  `panel_message` (mentions + selected files, generic sentence) extracted and tested.
- `crates/moe-extensions/ai_commands.rs`: guidance card updated (the panel no longer passes the
  search text; input = the selection).
- `ui/src/messages.ts` (new): the shared message stream (bubbles + streamed Markdown + generating
  indicator + last-answer tracking) now used by the Side View (`chat.ts` refactored) and the panel
  page; both windows adopt any frame carrying the conversation they show.
- `ui/src/main.ts`: `chat` mode (render, composer, streaming), the confirmation dialog
  (`index.html` #confirm-card), chat actions card, delete/new/history/materialize/copy/write-back
  paths, the chip shows the extension, `InputKind`-honoring `applyCommand`.
- Docs: ADR-0036; CONTEXT (Quick Ask redefined; Capture + Message stream terms; Stop note); README
  (keymap rows + AI Chat / AI Commands sections).

## Acceptance (verified 2026-10-09)

- Enter on the matched Quick Ask row opens the blank page (no request); the capture row asks in the
  page (question bubble + streaming answer).
- Consecutive sends land in one conversation (visible in ⌘P history); ⌃[ / ⌃] steps; ⌘N blanks;
  ⌃X removes; ⌥⏎ copies the last answer; ⌘J opens the Side View.
- Enter stops while generating; Back during generation asks first.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` (146 tests), `pnpm build` — all green.

## Comments

- 2026-10-09 (agent): filed locally — the Linear workspace is out of free issues, so the tracker
  moved to `docs/issues/` (see `docs/agents/issue-tracker.md`). Add the commit sha to the header
  line when this lands.
- 2026-10-09 (agent): **started** — claimed, and re-verified the delivery on the current tree
  (which also carries the separately staged pointer-intent work). All seven requirements checked
  against the code (`applyCommand` InputKind rule; `quick_ask` / `panel_ask` / `panel_continue`;
  `sendChatMessage` / `stopChatGeneration` / `openConfirm` / `removeCurrentChat` / `stepChat`;
  chat actions card); one hardening made: the stop-confirmation dialog now owns the keyboard before
  the ⌘⇧A attachment toggle (a modal must not open another mode). `cargo fmt --check`,
  `clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (146 tests) and
  `pnpm build` are green. Remaining: the delivery commit (`Commit:` line), then `State: done`.