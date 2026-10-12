# Moe

A keyboard-first desktop command palette: one unified set of keybinding semantics drives every
capability — select text in any app, summon the panel, run a command, and write the result back
to the cursor.

> This file is a glossary only. Implementation details do not live here; see `docs/adr/`.

## Language

### Capability structure

**Extension**:
The unit of capability organization: aggregates several Commands and exclusively owns its own
storage and history. AI Chat is one Extension.
_Avoid_: plugin, sub-app (acceptable colloquially; formally always Extension)

**Command**:
One callable entry an Extension exposes to the user; every Command is searchable in the command
palette and is the interface through which the user interacts with an Extension's internal data.
Its title carries **no extension prefix** — root rows show the extension name as secondary text
and the command's kind as a trailing badge (ADR-0030).
_Avoid_: subcommand, shortcut, action

**Row anatomy (root)**:
What a root command row renders, left to right (ADR-0030): icon · command name (prefix-free) ·
extension name · the command's declared shortcut as a Kbd (when it has one) · kind badge.
Every key panel anywhere on the surface is the same `kbdEl` component.
_Avoid_: list item style

**Namespace**:
The storage isolation domain each Extension exclusively owns; an Extension's history and data
exist only inside its own Namespace and are invisible across Extensions.
_Avoid_: sandbox, partition

**Side View**:
A resident presentation form an Extension can declare, distinct from the platform's default
floating panel; a conversation in the floating panel can be "materialized" into a Side View.
Each Side View belongs to exactly one Extension.
_Avoid_: sidebar mode (the formal term is Side View)

### Panel and interaction

**Command Panel**:
The centered floating panel summoned by the global hotkey; the only place where the user meets
every Command.
_Avoid_: launcher (acceptable colloquially)

**Summon Key**:
The single keybinding that summons the Command Panel; double-tap ⌘ by default, configurable as a
double-tap of another modifier or a combo (ADR-0008). The panel shows in-app guidance until the
system permission is granted.
_Avoid_: shortcut (that broadly means any key in the Keymap)

**Input Bar**:
The unified input area at the top of the Command Panel, always ready for input (like the address
bar's position, but its semantics is input, not path display). Its position and behavior are
identical across all Extension shapes.
_Avoid_: search box, address bar

**Filter Control**:
The Input Bar's trailing control on a result page, narrowing the Result List by a second dimension: a
label plus a chevron, showing the active value when one is set (ADR-0043). It is the trigger — the
surface it opens is the Filter Card.
_Avoid_: dropdown button, filter icon, search bar accessory

**Filter Card**:
The Card opened by the Filter Control, anchored beneath it; its rows are the dimension's values plus
a platform-supplied reset, and choosing one closes it (ADR-0043).
_Avoid_: filter menu, tag picker

**Result List**:
The candidate list below the Input Bar that updates live with the input; the one currently
selected is the Focused Item.
_Avoid_: dropdown

**Row Accessory**:
A trailing value on a result row — a tinted tag, plain text, a relative date, or an icon, each
optionally with a tooltip (ADR-0041). Colors come from the platform's bounded palette, never from an
Extension, and accessories hide in the split shape, where the same information belongs in the Detail
Metadata.
_Avoid_: badge, chip, label

**Empty View**:
The platform's non-focusable placeholder for a Result List with no rows: title, description, optional
icon and actions (ADR-0043). Suppressed while loading with an empty query; it replaced the
Extension-authored notice item.
_Avoid_: empty state, no-results row, notice item

**Section (source grouping)**:
The command palette's root page groups results by **source**: one section per Extension, headed
by the extension name, ordered by each group's best item (ADR-0020). Headers are not focusable
and never participate in keyboard navigation. A future source (e.g. file search) is just a new
extension with a new section.
_Avoid_: category

**Frecency**:
The platform-level usage weight behind Suggestions ordering and search tie-breaks: each use adds 1
to an accumulated score that decays exponentially over time, so frequency and recency are one
number and abandoned commands fade away (ADR-0023 amendment).
_Avoid_: popularity, hit count

**Suggestions**:
The section on an empty input listing the commands with the highest frecency (at most 5, highest
score first — frequency weighted by recency, not raw recency; ties by most recent use — ADR-0023
amendment). Hidden when nothing has been used yet; typing switches back to normal matching
(Raycast-style). ⌃X forgets one, ⌃⇧X clears all (ADR-0025).
_Avoid_: recents, favorites (favorites are a separate, user-curated section — see Favorites)

**Favorites**:
The user-curated section pinned **above Suggestions** on the root page (ADR-0029): commands
toggled via the root actions card's **Add to Favorites** (or `⌘⇧F`; remove with the same row).
Stored platform-level in `favorites.json`, insertion-ordered; a favorited command is not repeated
in Suggestions or in its source group. The root card leads with **Open Command** (the same Apply
as Enter — runs the focused command) and the favorite toggle; **Configure Extension** (a disabled
placeholder) is the extension-settings row below the deletes (ADR-0037 amendment).
_Avoid_: bookmarks, pinned, starred (the star is the icon, not the term)

**List–detail page template**:
The platform's split result page (`ActionResult::list` with `Item.detail`, ADR-0018): a list column
plus a preview pane with one set of keyboard semantics (navigation, Apply, copy, ⌘K actions, the
delete slots, Back). An Extension adopts it by returning Items — optionally grouped with
`Item.group`, whose labels render as non-focusable headers — and by implementing the hooks its
actions route to; it never lays out the page or binds keys.
_Avoid_: custom list view, table page

**Page Shape**:
Result pages have exactly three shapes: **list** (single column), **split** (list on the left +
detail on the right), and **detail** (a single result filling the panel). The platform decides
and renders them in one place (`ui/src/layout.ts`); split ratio and window size are globally
uniform. Extensions only provide content (an Item may carry `detail`) and never lay themselves
out (ADR-0018).
_Avoid_: layout template, view mode

**Live List**:
A list semantics of a Command (`CommandMeta.live`): once entered, every Input Bar change re-runs
the Command with the new query (e.g. "Search Chat History" filters titles as you type). For
non-live Commands, input changes remain palette search. Entering a Live command clears the Input Bar:
the text that found the command is never its parameter.
_Avoid_: dynamic search, autocomplete

**Capture (capturing row)**:
The row an Extension offers when the search has no match (`Extension::fallback_command`); its Apply
**consumes the Input Bar text** — the typed text is the command's parameter (AI: Ask "…", Moe:
Save AI Key), and the page it opens may take the bar over (the Quick Ask page clears it). Matched
command rows never treat the search text as content: the panel delivers the input only to rows
declaring `InputKind::Query`, plus the Live List takeover above (ADR-0036) — and the bar is cleared
when a command that did not consume it opens its page, so the text never lingers as a page's state
(ADR-0038).
_Avoid_: fallback item (the formal term is the capturing row)

**Focused Item**:
The single item in the Result List currently receiving keyboard operations. Every primary /
secondary action acts on it.
_Avoid_: highlighted item, selected item ("selection" is reserved for the text selection in other
apps)

**AI Command (AI text transform)**:
A one-shot text transform command of the AI Commands extension (Improve Writing / Make Shorter /
Translate …): input = the selection (or the input-bar text when there is none); the finished result
card **is the decision point** — nothing touches the host app until the user applies (⏎ write back,
⌥⏎ copy, "{Command} Again" in ⌘K re-runs the transform on the result, Esc discards), and a stopped
generation writes back nothing (ADR-0024 amendments).
_Avoid_: quick ask (that is AI Chat), prompt

**Thinking (reasoning block)**:
The chain of thought some models stream inline (`…` / `<thinking>`-style wrappers): never the answer —
the platform strips it from the streamed, copied and written-back answer text and carries it in the
item payload — in an Agentic Conversation it is part of the record, replayed to the endpoint and
re-rendered on reopen (ADR-0039), while in a plain Conversation it stays ephemeral — and the UI
folds it into a ChatGPT-style row above the answer (`Thinking` while it
streams, `Thought for N seconds` once settled; collapsed by default — ADR-0024 amendments).
_Avoid_: reasoning text, chain of thought as content

**Detail Full**:
A results-layer shape: the single result is itself the content (AI answers, notices) and its
detail fills the panel; the opposite of the list + detail shape. Declared by the command's result
(`detailFull`), never guessed from item count (ADR-0013).
_Avoid_: full-screen card, zoom mode

**Detail Metadata**:
The structured block accompanying a result's Markdown detail — labels, links, tag lists, separators —
rendered at the bottom of the detail pane in the split shape and in a right-hand column in the
detail shape (ADR-0042).
_Avoid_: sidebar, properties panel, info panel

**Apply**:
The primary action on the Focused Item (Enter): runs the default semantics of what the item
represents.
_Avoid_: execute, open, enable

**Stop (stop generation)**:
An item still streaming is marked `pending`; while pending, **Enter requests stop** (the primary
action yields to Stop Generation — result cards and the Quick Ask page alike) and **Back asks first**
on every streaming surface: the confirmation dialog (⏎ Stop Generation / Esc Keep Generating), while
the pill's Stop row stops immediately (ADR-0036 amendments, ADR-0038). Stopping is platform-wide
(all ongoing generations at once) and keeps what was generated instead of rolling it back (ADR-0006
amendment).
_Avoid_: cancel, interrupt

**Secondary Action**:
A named alternative action on the same Focused Item, each with a fixed shortcut (e.g. ⌥Enter
copies plain text, copy as HTML, …). The primary/secondary distinction is a consistent Command
semantics convention, not something each Extension invents.
_Avoid_: context menu

**Copy**:
The default semantics of SecondaryCopy (⌥Enter by default): write the Focused Item's text to the
system clipboard, **without touching the host app** (the opposite of Write Back; ADR-0002
amendment).
_Avoid_: clipboard-copy to the system

**Selection**:
What the user selected in another app: a text selection, or files selected in Finder (ADR-0021).
Moe's own list selection is not called Selection; it is called Focus. The selection captured
before summoning automatically becomes AI context for questions; selected files become question
attachments.
_Avoid_: using Selection for the Focused Item

**Keymap (unified keybindings)**:
The platform-level keybinding semantics identical across all Extensions (navigation, Apply,
secondary action, Esc layered back, Show All Actions, and the general actions
Browse/New/Delete/DeleteAll). Extensions must not override it; they may only register named
actions for their Commands (actions may carry their own shortcuts).
_Avoid_: shortcut mapping, bindings

**Show All Actions**:
The layered behavior triggered by ⌘K on every surface (command panel and Side View alike): lists
what can run from the page as one layer for keyboard selection, in **one canonical order**
(ADR-0037 amendment) — the page's ⏎ action (Apply / Open Command, or Stop while a stream runs),
then its ⌥⏎ action, then the object's own actions (the focused item whatever the page shape; the
conversation on a chat page), then the destructive Delete ⌃X / DeleteAll ⌃⇧X, then the
extension's settings, then the app's global rows (Open Settings, Open Config File, window). The
sections (Actions / Command / General; Chat / App / Window) keep their labels, order by their
topmost row, and hairline-separate adjacent rank groups inside a section. Assembled from
registered sources (ADR-0037); one semantics, one key (ADR-0014 amendment).
_Avoid_: command menu

**Browse**:
A platform general action, ⌘P by default: opens the current Extension's record list (AI = chat
history). The entry is declared by the Extension (`browse_command`); no declaration means no
records to browse, and the platform shows a single inline hint (ADR-0014).
_Avoid_: history, list view (Browse means specifically this keybinding semantics)

**New**:
A platform general action, ⌘N by default: creates a new record (AI = new conversation). The
entry is likewise declared by the Extension (`new_command`); together with Browse and Actions it
forms the general actions spanning Commands/sub-apps (ADR-0014).
_Avoid_: new window, plus button

**Delete / DeleteAll (delete slots)**:
Platform general actions, ⌃X / ⌃⇧X by default: delete the current record / delete all records
(AI = current conversation / all conversations). Implemented by the Extension's `delete_item` /
`delete_all` hooks, returning the actual count (ADR-0022). Intercepted only on the results layer /
list contexts; the command layer and editors pass the native cut through. On the Suggestions
section they mean forget-one / forget-all instead (ADR-0025).
_Avoid_: clear, delete key

**Action Bar**:
The floating bottom bar region over the list's last visible row band (one row tall, ADR-0026):
the **avatar chip** on the left and the **action pill** (primary action + actions) on the right.
_Avoid_: hints bar (the old name)

**Avatar Chip**:
The bottom-left chip (ADR-0026): a button whose resting face is its own icon, expanding to the
current extension's icon + name once inside a command. It doubles as the **toast host** (toasts
expand it in place) and opens the **About Card** on click.
_Avoid_: extension badge, logo

**Card**:
The floating overlay family: one component and one behavior set — rows in non-focusable sections, a
filter input, ↑↓ select, ⏎ run, Esc / empty ⌫ dismiss, click-outside closes — instantiated at
several anchors, because a card sits next to the control that opens it (ADR-0028, ADR-0044).
Instances: the actions card, the About Card, the history card, the model switcher, the Filter Card.
_Avoid_: menu, popover, dialog, dropdown

**About Card**:
The card opened from the avatar chip or ⌘⇧K (ADR-0027): the app's meta actions (Open Config File /
Save AI Key / Send Feedback) with the search input at the bottom and rows grouped into App /
Theme / Support sections; the Theme rows (System / Light / Dark, ADR-0035 amendment) apply
immediately, persist to config.toml, and mark the active one with a trailing check; same UX as the
actions card (ADR-0026). Rows show their Kbd when the action has one (Open Config File = ⌘,);
unbound rows show none.
_Avoid_: settings page, preferences window

**Write Back**:
How a command's result is delivered back into the user's text context: replace the Selection when
there is one, insert at the cursor otherwise. Only write-back Commands behave this way.
_Avoid_: paste (pasting is just one way to implement a write back)

**Settings Window**:
An ordinary window hosting Moe's settings (MOE-0029): sidebar sections, one pane each — the AI
pane manages the providers in use (per-provider keys, picker visibility, the active pair) over
the Model Catalog, the General pane the appearance and the config-file entry. Not a panel page
and not floating: it stays open until closed (Esc / traffic light), and it is reached from the
tray, the palette, the Side View's gear and every page's actions card.
_Avoid_: preferences dialog, settings page (the About card is not it either)

### AI Chat

**Conversation**:
One continuous question-and-answer record inside the AI Chat Extension, owned by its Namespace.
_Avoid_: chat

**Model Catalog**:
The embedded models.dev snapshot (refreshed daily, MOE-0025) behind the Provider defaults and the
model picker (MOE-0028): per-model limits, modalities, capabilities and pricing; an official
provider's base URL, key variables and model list come from it.
_Avoid_: model list, registry (the command Registry is a different thing)

**Model Switcher**:
A row inside both chat surfaces' actions cards ("Change Model", ⌥M) and the card it opens
(MOE-0029/MOE-0030 revision): the bare model names of the providers that can run — each listed
provider carries a resolvable key — ranked with the picker's order, grouped by provider, the
active one checked; the focused row's facts (context, modalities, capabilities, pricing, dates)
ride a companion card floating beside the list. Enter or click writes the active pair, which the
chat titles then carry under the conversation title (MOE-0033). Which models the card and the
picker list at all is the Settings Window's checkboxes (`hidden_models`).
_Avoid_: model menu, model dropdown, model chip

**Provider**:
The AI endpoint behind a Conversation and the AI Commands (ADR-0040): an official id from the
embedded model catalog (base URL, key variable names and model list from the catalog; the protocol
from the checked-in table) or a custom `[ai.providers.<id>]` block. One active provider per config
(`[ai] provider` + `model`); its key lives in its own file (`keys/<id>`; the implicit `default`
provider of legacy configs keeps the `api-key` file).
_Avoid_: backend, account

**Protocol**:
The wire dialect a Provider speaks (ADR-0040): `openai-chat` (the only one carrying Tools in v1),
`openai-responses`, or `anthropic`. Adapters normalize all three into one delta vocabulary, so the
chat pipeline never branches per protocol.
_Avoid_: API version, driver

**Quick Ask**:
A conversation hosted in the Command Panel (ADR-0036): applying the Quick Ask row opens the page
(the search text is never sent as its first question — only the capturing row does that); the
page's own composer row — the bottom bar's glass: add button, one-row capsule input, actions
button (MOE-0030) — takes the draft (Enter sends, answers stream), while the Input Bar above
reads the conversation title (read-only) in the command pages' own row design, and the
conversation continues until a New Chat (⌘N) or a step through history (⌃[ / ]) replaces it. A
record surface like the Side View: Remove Chat (⌃X) deletes it through the same delete slot, and
⌘J materializes it into the Side View.
Declared by `ActionResult::Conversation` and continued by `Extension::panel_continue` (IPC `panel_send`).
_Avoid_: quick mode

**Materialize**:
The action that turns the current conversation in the Command Panel into the Extension's
Side View.
_Avoid_: expand, pop out

**Message stream**:
The panel's Quick Ask page and the Side View render conversations through the same stream of
bubbles (`ui/src/messages.ts`): user bubbles + Markdown answers updated in place while streaming,
with the last answer as the copy / write-back target.
_Avoid_: chat view, transcript

**Attachment**:
A local file carried with a question (v1: text and images). Expressed in the input bar as an
`@path` mention (ADR-0010); content is read at request time (text inlined, images into the
multimodal content), history stores only the reference. Files selected in Finder become
attachments too (ADR-0021); a pure-attachment question shows guidance in the panel or a generic
request sentence in the Side View — the raw path is never sent as the question.
_Avoid_: upload, file object

**Tool**:
A named capability Moe executes on the model's behalf mid-turn (v1: Web Search, Web Fetch, Read
File). Advertised per request from config, invoked by the model through function calling, and its
result returns to the model — never written back to the host app.
_Avoid_: plugin, skill, function (function calling is the wire mechanism, not the domain object)

**Tool Call row**:
The collapsible muted row in the message stream rendering one Tool invocation — icon, plain label,
shimmer while running, expand for the truncated result; the same visual language as Thinking,
stacked in arrival order above the answer.
_Avoid_: activity card, step

**Round**:
One request→response exchange inside an agentic turn's loop; a turn runs at most a fixed number of
Rounds (8 in v1) before it is force-settled.
_Avoid_: iteration, step

**Agentic Conversation**:
A Conversation created with Tools offered: its record is append-only — questions, answers,
reasoning, Tool Calls — and every later request replays it whole, keeping the endpoint's prefix
cache warm (ADR-0039). Conversations created before Tools existed stay plain forever.
_Avoid_: agent mode; the bare "Agent" is reserved for the future External Agent (an ACP
subprocess), never for this capability
