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

## Amendment: the launch keys go global and toggle — ⌘' / ⌘⇧' (⌘/ unbound, MOE-0017)

The panel-only ⌘/ / ⌘⇧/ keys did not deliver the point of the bindings: Quick Ask and the side chat
could only start after summoning the panel, and pressing a key again did nothing. The two launch
keys move to **⌘'** and **⌘⇧'**, and the app registers both as **global hotkeys** at startup through
the global-shortcut plugin — fixed platform bindings like the summon key, not configurable. A chord
another app owns fails to register (logged), and the same chord keeps working inside the open panel
as the fallback; ⌘/ is unbound again.

Both keys **toggle their surface**, the summon key's own semantics (a visible window hides):

- **⌘' — Quick Ask**: with the panel hidden, the show path runs as a normal summon (the selection
  context is captured, ADR-0019/0021) and the `quick-ask-toggle` event puts the webview on the page
  — blank when it was elsewhere, kept as it was when it was already on the page. With the panel up
  on the page, the webview dismisses it. Rust cannot read the page, so the visibility the hotkey
  saw rides the event and the UI decides; the page's conversation is view state that survives
  hiding either way.
- **⌘⇧' — Open Side Chat**: a visible chat window is hidden, a hidden one is shown *as it was* (the
  `side-show` event refreshes without resetting) — a window toggle must not drop the thread the
  user stepped away from. The tray's "AI Chat" and the `ai.side-chat` command still open a blank
  conversation (the direct-start path), and ⌘J still materializes the panel's current conversation.

The registered hotkey consumes the chord, so the global path and the in-panel fallback cannot
double-fire. On macOS these are Carbon hotkeys: no Input Monitoring grant is needed, so both keys
work even before the double-tap summon listener is authorized.
## Amendment: the page is a normal chat layout (MOE-0030)

The page kept one flaw from its birth: the conversation lived under the panel's top Input Bar,
which served as its composer — a chat whose input sits where every other page's search sits.
Raycast's own quick chat moved to the normal chat layout, and so does this page:

- **The Input Bar row yields to a chat header on the page**: back arrow + conversation title on
  the left, the model chip and the actions button in the top-right corner. The Input Bar stays
  the command layer's and the result pages' input (ADR-0034 unchanged there); on the chat page
  its leading-slot identity moves to the header's back button.
- **The composer pill sits at the bottom** — the shared component (`ui/src/composer.ts`) the
  Side View mounts too: round attach button, rounded field with the path-input row, the growing
  draft and the round send/stop button inside the field's right edge. Enter sends (stops while
  generating), ⌘⇧A opens the composer's path row on the page; the Input Bar's attachment mode
  stays a command-layer mode.
- **The bottom bar hides on the page** (ADR-0026): the composer carries send/stop, so the action
  pill would duplicate it; the avatar chip's About card stays reachable from the command layer.
- **The model chip opens the switcher card** (MOE-0029's rows): the ranked models grouped by
  provider, the current one checked; Enter/click writes `[ai] provider` / `[ai] model` in place.
  Both chat surfaces carry the chip, so switching providers never leaves the conversation.

The page dialect (Enter sends, Back confirms while generating, ⌃[/⌃] step, ⌘N/⌃X slots) is
unchanged — only the focused element and the chrome moved.

## Amendment (revision): the page wears the panel's own chrome (owner review, MOE-0030)

The amendment above gave the page a header of its own (back + title left, chip + actions in the
top-right) and a composer pill with an inner send button. The owner's review folded the page
fully into the panel's existing design language instead:

- **The top bar is the Input Bar itself** — the command pages' own row: the leading slot keeps the
  back arrow (ADR-0034) and the field reads the conversation title, read-only. The page invents no
  second header design; chat pages and command pages share one top bar.
- **The composer row is the bottom bar's language**: the round glass add button, one capsule-like
  one-row input, and the round glass actions button — the about chip's and the action pill's glass
  (surface-float + sheen + blur, squircle capsule, `--moe-row-h` tall). Enter sends and stops while
  generating; the capsule carries no send button of its own, and the add button swaps it into the
  attachment path input.
- **The actions button moved from the top-right to the bottom-right end of that row**, and the
  model switcher is a row inside the card it opens ("Set AI Model", plus "Stop Generation" while a
  stream runs) on both chat surfaces — switching models is an action of the conversation, not a
  header control. The header chip is gone.
- On the page the bottom bar (chip + pill, ADR-0026) still yields to the composer row; the about
  card stays a command-layer affordance.

## Amendment: the title carries the active pair, and ⌥M changes the model (MOE-0033)

With the header chip gone (above), nothing on either chat surface said which provider and model
the conversation would run on. The pair now rides the title on both surfaces:

- **Under the title, smaller and muted**: the panel's chat page shows `Provider · Model` beneath
  the conversation title in the Input Bar row (11px, `text-fg-subtle`, hidden on every other
  page); the Side View shows the same line under its centered titlebar title. It follows a switch
  through the model switcher immediately (`onApplied`) and is fetched on page entry / window show
  (`ai_active_pair`); an unset pair leaves the line hidden.
- **`SystemKey::ChangeModel`, default ⌥M** joins the unified keymap (the AI surfaces' key family
  with ⌘' / ⌘⇧'), and the actions card's row is renamed **Change Model** with the `⌥M` Kbd from
  the table (the view layer never invents keys). The key lands the same action as the row — the
  model switcher — on the chat surfaces only (the panel's chat page; the Side View); the physical
  `KeyM` is matched because ⌥M types "µ" on macOS layouts.

## Amendment: the page shows its target, and the command that started one goes (MOE-0056, MOE-0058, MOE-0059)

A conversation's first message carries the captured selection as context, and the page never said so:
entering Quick Ask with text selected looked exactly like entering it with nothing selected. The
decision above already fixes *when* that context applies ("the conversation's first message only"), so
the page now says *what* it is about to carry, in the one place where the question is live — above the
composer, and only while the conversation is still blank.

**The target chip.** A chip in the house glass (the Input Bar chips' material, ADR-0043/0034) sits
above the composer row, left-aligned with the add button, showing `Selection` and its size ("1,234
characters", or the file count when only Finder files were captured). It is armed only on a blank
conversation with no send in flight: the first send consumes the target, a failed send returns it (the
selection never made it into a message), and a started or reopened conversation never shows it. No
target is not news, so there is no "No selection" state here — the Input Bar's chip carries that
answer, because a command may *depend* on a selection while a chat never does.

**The peek.** Hovering the chip opens a card above it — the actions/About card language, a reading
column that scrolls rather than growing — with the full selected text (pre-wrapped, never re-parsed)
and the captured Finder paths. This is the one place the selection's *text* crosses into the webview,
and it is fetched on demand (`selection_detail`, once per reported state) rather than at boot: the
chips render from `selection_state`'s shape alone, so the text moves only when the user asks to look
at it. The chip carries no native tooltip — it would fight the card — so the card's caption carries
the explanation instead.

**The command that started a selection chat is gone.** `ai.chat-about-selection` (MOE-0056) made a
selection-targeted conversation reachable, but it was a *command* for something the page itself can
always do: `ai.quick-ask` already carries the captured selection into the first message, and ⌘' opens
that page. With the target now visible there, the command was a second, narrower door to the same
room — and it made the selection look like something you must pick a command to use. Removing it also
removes its icon (`messages-square`) from the Lucide map, whose only caller it was (ADR-0012's rule:
an icon leaves with its caller).
