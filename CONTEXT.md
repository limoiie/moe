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
_Avoid_: subcommand, shortcut, action

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

**Result List**:
The candidate list below the Input Bar that updates live with the input; the one currently
selected is the Focused Item.
_Avoid_: dropdown

**Section (source grouping)**:
The command palette's root page groups results by **source**: one section per Extension, headed
by the extension name, ordered by each group's best item (ADR-0020). Headers are not focusable
and never participate in keyboard navigation. A future source (e.g. file search) is just a new
extension with a new section.
_Avoid_: category

**Suggestions**:
The section on an empty input listing the most recently used commands (at most 5, most recent
first, ADR-0023). Hidden when nothing has been used yet; typing switches back to normal matching
(Raycast-style). ⌃X forgets one, ⌃⇧X clears all (ADR-0025).
_Avoid_: recents, favorites (favorites are a separate, user-curated section — see Favorites)

**Favorites**:
The user-curated section pinned **above Suggestions** on the root page (ADR-0029): commands
toggled via the root actions card's **Add to Favorites** (remove with the same row). Stored
platform-level in `favorites.json`, insertion-ordered; a favorited command is not repeated in
Suggestions or in its source group. The root actions card's **Command** section also holds
**Open Command** / **Configure Extension** as disabled placeholders.
_Avoid_: bookmarks, pinned, starred (the star is the icon, not the term)

**Page Shape**:
Result pages have exactly three shapes: **list** (single column), **split** (list on the left +
detail on the right), and **detail** (a single result filling the panel). The platform decides
and renders them in one place (`ui/src/layout.ts`); split ratio and window size are globally
uniform. Extensions only provide content (an Item may carry `detail`) and never lay themselves
out (ADR-0018).
_Avoid_: layout template, view mode

**Live List**:
A list semantics of a Command (`CommandMeta.live`): once entered, every Input Bar change re-runs
the Command with the new query (e.g. "AI: Search Chat History" filters titles as you type). For
non-live Commands, input changes remain palette search.
_Avoid_: dynamic search, autocomplete

**Focused Item**:
The single item in the Result List currently receiving keyboard operations. Every primary /
secondary action acts on it.
_Avoid_: highlighted item, selected item ("selection" is reserved for the text selection in other
apps)

**AI Command (AI text transform)**:
A one-shot text transform command of the AI Commands extension (Improve Writing / Make Shorter /
Translate …): input = the selection (or the input-bar text when there is none); the streamed
result automatically writes back to the selection and dismisses the panel on completion; a
stopped generation does not write back (ADR-0024).
_Avoid_: quick ask (that is AI Chat), prompt

**Detail Full**:
A results-layer shape: the single result is itself the content (AI answers, notices) and its
detail fills the panel; the opposite of the list + detail shape. Declared by the command's result
(`detailFull`), never guessed from item count (ADR-0013).
_Avoid_: full-screen card, zoom mode

**Apply**:
The primary action on the Focused Item (Enter): runs the default semantics of what the item
represents.
_Avoid_: execute, open, enable

**Stop (stop generation)**:
An item still streaming is marked `pending`; while pending, Esc's first priority is requesting
stop (platform-wide, stops all ongoing generations at once), keeping what was generated instead
of rolling it back (ADR-0006 amendment).
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
The layered behavior triggered by ⌘K (generic sub-app keybinding: ⌘⇧P): lists the Focused Item's
primary/secondary actions as one layer for keyboard selection. One semantics, two keys: ⌘K is
the palette convention, ⌘⇧P for sub-apps to reuse (ADR-0014).
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
The bottom-left chip (ADR-0026): Moe's app avatar on the command layer, the current extension's
icon + name once inside a command. It doubles as the **toast host** (toasts expand it in place)
and opens the **About card** on click.
_Avoid_: extension badge, logo

**About Card**:
The card opened from the avatar chip or ⌘⇧K (ADR-0027): the app's meta actions (Open Config File /
Save AI Key / Send Feedback) with the search input at the bottom and rows grouped into App /
Support sections; same UX as the actions card (ADR-0026).
_Avoid_: settings page, preferences window

**Write Back**:
How a command's result is delivered back into the user's text context: replace the Selection when
there is one, insert at the cursor otherwise. Only write-back Commands behave this way.
_Avoid_: paste (pasting is just one way to implement a write back)

### AI Chat

**Conversation**:
One continuous question-and-answer record inside the AI Chat Extension, owned by its Namespace.
_Avoid_: chat

**Quick Ask**:
A single- or short-turn question asked directly inside the Command Panel, whose result can be
written back.
_Avoid_: quick mode

**Materialize**:
The action that turns the current conversation in the Command Panel into the Extension's
Side View.
_Avoid_: expand, pop out

**Attachment**:
A local file carried with a question (v1: text and images). Expressed in the input bar as an
`@path` mention (ADR-0010); content is read at request time (text inlined, images into the
multimodal content), history stores only the reference. Files selected in Finder become
attachments too (ADR-0021); a pure-attachment question shows guidance in the panel or a generic
request sentence in the Side View — the raw path is never sent as the question.
_Avoid_: upload, file object
