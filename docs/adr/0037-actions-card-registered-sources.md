# The actions card is assembled from registered sources

The ⌘K card used to be merged by hand at every call site: `openActionsCard` had one branch per
layer (command layer / chat page / result page) and the Side View kept a third hard-coded list.
"Which rows appear for this page" therefore lived in three places, every new page kind (the Quick
Ask page, ADR-0036) had to re-derive it, and the general slots were only half-listed: Browse/New
appeared on result pages but not on the command layer or the chat page, and Delete — which ⌃X
already fires on every record page — was never listed at all.

## Decision

- **Sources, not merges.** A surface registers ordered `ActionSource`s (`ui/src/actions.ts`): a
  section title, a `when(ctx)` predicate for the pages it joins, and an async `rows(ctx)` builder.
  `createActionPlan(sources).sections(ctx)` loads the card — sources in registration order, empty
  sections dropped. `PageContext` carries what every source keys on: the surface, the page kind
  (`commands` / `items` / `chat`), the focused item, the page's command id, and whether the focused
  root row is a suggestion. No call site merges rows by page kind anymore.
- **The object is the focused item, whatever the shape.** List, split and detail pages differ only
  in layout (ADR-0018); the card's **Actions** section is the focused item's declaration on all
  three, and on a conversation page it is the conversation's own actions (Open in Side View /
  Copy Last Answer / Write Back Last Answer).
- **Three sections, one per question.** The panel's plan registers, in order:
  1. **Actions** — the object's own actions (above);
  2. **Command** — the page command's own rows (ADR-0029): Open Command leads on the root only,
     then the favorite toggle, Remove from Suggestions (⌃X on a Suggestions row, ADR-0025) and the
     Configure Extension placeholder;
  3. **General** — the platform general slots **as they are live on this page** (ADR-0014/0022):
     Browse ⌘P / New ⌘N resolved from the Extension's declared entries, plus Delete ⌃X / DeleteAll
     ⌃⇧X where the page carries records (result pages; the chat page as Remove Chat / Remove All
     Chats). Labels are page-aware, keys always come from the keymap table.
- **Landing spots stay in the surface.** A source builds rows whose `run` is the same path the key
  binding takes (the panel's Browse opens the history list, the Side View's opens its history card),
  so a card row can never do something the keyboard would not.
- **Navigation is a key, not a row.** ↑↓ / ⌃N ⌃P move the Focused Item (and drive the card's own
  list while it is open, ADR-0031); the card lists actions, never a "Navigate" row.
- **Both windows use the pipeline** (ADR-0028): the Side View registers its own sources
  (Chat / App / Window) through the same `createActionPlan`, so the two cards cannot drift again.

## Cost

- `ActionRow` and its dispatch (`runActionRow` / `actionCardRow` / `iconOfActionRow`) are gone: a
  row closes over its run path, so the same action could be registered twice only by intent.
- The General section lists Delete wherever ⌃X is live — including Extensions that do not implement
  the delete hooks: running such a row reports the same NotFound toast the key already reports.
  Declaring delete support (the way Browse/New are declared entries) is the step to take if an
  Extension without hooks ever matters.
- Sources are surface code, not Extension declarations: an action an Extension wants on a card
  still has to be declared as an `Item` action (ADR-0006); the conversation's own actions on the
  chat page are synthesized by the panel for now (`ActionResult::Conversation` could carry them
  later without changing this pipeline).
- The command layer gains a General section (Browse/New for the focused command) and the chat page
  a Command section (favorite); the intro copy of neither page changes — the sections explain
  themselves.