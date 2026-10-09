# Quick Ask: a conversation page hosted in the panel

Quick Ask used to be a one-shot: the palette search text became the question, and the answer was a
full-screen detail card whose Enter wrote the text back. Three things broke down as soon as the
conversation needed a second turn: the search text silently became content (typing `quick` and
pressing Enter sent "quick" to the model instead of opening the command), there was no way to keep
chatting in the panel, and the answer card's capabilities (write-back / ⌥⏎ copy / ⌘J side view) had
nowhere to live once Enter meant *send*.

## Decision

- **Only declaring rows own the Input Bar text.** The panel now enforces `CommandMeta.input`: a row
  declaring `Query` receives the input on Apply (the no-match capture is the canonical case — its text
  *is* the question); a Live row takes the input over and clears it (ADR-0006 amendment);
  `None`/`Selection` rows are applied with no query at all. A command that must not be fed the search
  text therefore changes its declaration: `ai.quick-ask`'s matched row is `None` (Apply opens the
  page) while its capture row from `Extension::fallback_command` keeps `Query`.
- **A result variant declares the page**: `ActionResult::Conversation { conversation_id }` — the
  platform renders a message stream in the panel's detail pane and turns the Input Bar into the
  composer; `None` = blank new chat. The three page shapes of ADR-0018 are unchanged: a conversation
  is detail-shaped content, and extensions still never lay themselves out.
- **Continuation is an Extension entry**: `Extension::panel_continue(conversation_id, message,
  selection, emitter)` (IPC `panel_send`), parallel to `side_continue`/`side_send` (ADR-0004). The AI
  extension implements both on one core (`panel_ask`): the whole history is the model context, replies
  stream as the page's command id (`ai.quick-ask`) while the Side View keeps `ai.side` — a frame names
  the surface that produced it, and both windows adopt the frames of the conversation they show. The
  captured selection (text + Finder files) is context for the conversation's first message only
  (ADR-0019/0021 amendments hold); attachments-only input sends the generic request sentence instead
  of the raw `@path` (IIE4AD-391's rule, now on both surfaces).
- **The page's keys**: Enter sends the draft and clears the input; while generating, Enter requests
  stop (the platform-wide stop slot, IIE4AD-365) instead of Esc, and Back (Esc / empty ⌫) asks first
  with a confirmation dialog — Stop Generation (⏎) or Keep Generating (Esc) — because an accidental
  Back must not silently kill an answer. One Back layer: a draft clears first (ADR-0017's input
  layering), the next Back returns to the command layer (the Input Bar's leading arrow follows).
- **The page is a record surface**: ⌘N / New Chat open a blank page (`ai.quick-ask` with no query
  returns the blank `Conversation`; New Chat was the same action and folded into it — see the
  amendment), ⌃X removes the current conversation and ⌃⇧X all of them through the Extension's
  delete hooks — exactly the chat history page's deletion (ADR-0022) — and ⌃[ / ⌃] step backward /
  forward through `side_conversations` (the previous, older conversation / the newer one), like the
  Side View. ⌘P (Browse) still resolves the Extension's declared record
  list, and the actions card carries New Chat / Remove Chat / Open in Side View (⌘J) / Copy Last
  Answer (⌥⏎) / Write Back Last Answer: the old answer card's capabilities, rebuilt on the
  conversation instead of a single item.
- **One message stream renderer** (`ui/src/messages.ts`) serves the panel page and the Side View:
  same bubbles, same attachment chips, same inline generating indicator, same last-answer tracking —
  the two surfaces cannot drift.

## Cost

- Attachments-only captures send the generic sentence now instead of showing the panel's old
  "type your question" guidance card; the guidance card's job moved into the page (the blank state
  plus the hint line).
- `Selection`-declaring commands (AI Commands) no longer receive the search text as a fallback
  input — without a selection they show their guidance card. The rule is uniform now: the Input Bar
  text is delivered to `Query` rows only.
- The answer is no longer an `Item`: write-back loses its page shortcut (Enter sends) and lives in the
  actions card; Enter/⌥⏎ on the page mean send/stop and copy-last-answer instead.
- `ai.quick-ask`'s declaration is split by row kind (matched = `None`, capture = `Query`); an
  extension whose page and capture differ in input semantics carries the same split (the contract
  already allows `fallback_command` to declare its own `CommandMeta`).
- The page's "current conversation" is transient view state, not an Extension record: entering Quick
  Ask always opens blank, and Open in Side View / the history list are the paths back to a thread. A
  later "resume the last conversation" would need a persisted panel session.
- Both windows adopt any `command-event` frame whose payload carries the conversation they show
  (the panel's `ai.quick-ask` ask and the Side View's `ai.side` continuation alike), so the panel and
  the Side View mirror the same streaming answer while it runs; the stream state (Send vs Stop)
  follows the frames.

## Amendment: Enter is the stop binding on every surface

The page's rule generalizes to the result cards. A pending `Item` (AI Commands streaming a
transform, IIE4AD-365) already yielded its primary action to **Stop Generation** in the action
pill, but the Enter key still ran the item's declared primary — writing back the half-generated
text while the pill said Stop. Now Enter stops on the results layer too: the same path as the pill
and Esc (`stop_generation`, platform-wide), and the pill carries the `⏎` Kbd it always meant. Esc
keeps its first-priority stop on result cards; only the Quick Ask page keeps the confirmation
dialog on Back — an accidental Back there must not kill an answer mid-sentence. Once the stream
stops or finishes, Enter is the item's primary again (write back for AI Commands, ADR-0024).

## Amendment: Back confirms on every streaming surface (ADR-0038)

The asymmetry above is gone: a result card asks the same question on Back (Esc / empty ⌫) — ⏎ Stop
Generation / Esc Keep Generating — while ⏎ and the pill's Stop row still stop immediately. ADR-0038
gives the result pages the rest of the page dialect the dialog belongs to: the Input Bar is not the
page's state, and Back leaves a result page in one step to the fresh root.

## Amendment: New Chat folds into Quick Ask; the surfaces' launch keys (MOE-0016)

`ai.new-chat` and `ai.quick-ask` resolved to the same result — the blank `Conversation` page — so
the duplicate command is gone. The generic New action (⌘N, ADR-0014) still opens the blank page:
`new_command()` now resolves to `ai.quick-ask`, and "New Chat" remains only where the UI paints it
(the actions card's General row, the Side View's header button).

The two surfaces also gain direct launch keys, declared in the platform keymap so the rows' Kbd
comes from the single table (ADR-0030):

- **⌘/ — Quick Ask**: from anywhere in the panel, opens the conversation page with the same landing
  as applying the command's row — always blank, the search text is never sent (the page dialect of
  this ADR is otherwise unchanged).
- **⌘⇧/ — Open Side Chat**: the new `ai.side-chat` command returns `ActionResult::OpenSideView
  { payload: {} }` — the same platform path as the tray's "AI Chat": the panel is hidden and the
  chat window opens with a blank conversation, no panel page entered. The panel's ⌘J still
  materializes the *current* conversation instead; ⌘⇧/ is the direct "just open the side chat".