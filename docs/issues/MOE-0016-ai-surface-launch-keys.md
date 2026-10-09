# M7x: Quick Ask owns ⌘/; ⌘⇧/ opens the side chat directly; New Chat folds into Quick Ask

- **ID**: MOE-0016
- **State**: in-progress
- **Labels**: feature, ux
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent

User feedback, three asks:

1. "it seems that the new-chat command and the quick-ask command are the same action, if so,
   remove new-chat"
2. "bind mod+/ to quick-ask"
3. "create a new command to open the side chat directly, then bind mod+shift+/ to it"

## Verification (ask 1)

Confirmed identical: `ai.new-chat` returned — and `ai.quick-ask` with no query returns — the same
`ActionResult::Conversation { conversation_id: None }`; both declare input kind `None`, so applying
either opens the blank Quick Ask page. `ai.new-chat` is removed.

## Design — ADR-0036 amendment

- **Remove `ai.new-chat`**: the duplicate row leaves `commands()`. The generic New action (⌘N,
  ADR-0014) keeps its behavior: `new_command()` resolves to `ai.quick-ask`, which opens the same
  blank page. "New Chat" remains only where the UI paints it (the actions card's General row and the
  Side View's header button).
- **⌘/ — Quick Ask**: a new platform semantic `SystemKey::QuickAsk` ("⌘/") in the keymap table;
  `ai.quick-ask` declares its Kbd through `display_of`, so the row cannot drift from the binding
  (ADR-0030). The panel recognizes the key from any layer (like ⌘J / ⌘⇧K) and lands exactly where
  applying the command's row lands: the blank conversation page — the search text is never sent.
- **⌘⇧/ — Open Side Chat**: a new `ai.side-chat` command (input kind `None`, Kbd via
  `SystemKey::OpenSideChat` = "⌘⇧/") returning `ActionResult::OpenSideView { payload: {} }` — the
  tray's "AI Chat" path: the platform hides the panel and shows the chat window with a blank
  conversation, no panel page entered. ⌘J still materializes the *current* conversation instead.

## Delivery

- `crates/moe-core/src/keymap.rs`: `SystemKey::QuickAsk` / `SystemKey::OpenSideChat` + default rows
  `⌘/` / `⌘⇧/`; semantic-count test updated, launch-binding test added.
- `crates/moe-extensions/src/ai.rs`: `ai.new-chat` removed; `new_command()` → `ai.quick-ask`;
  `ai.quick-ask` declares the ⌘/ Kbd; `ai.side-chat` added with the ⌘⇧/ Kbd and the direct
  Side View result; tests updated/added (entry resolution, Kbd declarations, side-chat result).
- `ui/src/main.ts`: `openQuickAsk` / `openSideChat` helpers (a single invoke each, sharing the
  command rows' landing paths) + the two key branches in the panel keydown (`?` covers shift+/ on
  most layouts).
- Docs: ADR-0036 amendment (this decision), ADR-0014 amendment (AI's New entry), README key table
  and Quick Ask paragraph.

## Acceptance

- Rust: `cargo test --workspace` green; `cargo clippy --workspace --all-targets -- -D warnings`
  green; `cargo fmt --all --check` green.
- UI: `pnpm -C ui build` green.
- GUI check pending (double-tap ⌘): ⌘/ opens the blank Quick Ask page from any panel layer; ⌘⇧/
  opens the side chat directly; ⌘N on AI surfaces still opens the blank page; the Quick Ask root row
  shows the ⌘/ Kbd; "New Chat" no longer appears as a command row.

## Comments

- 2026-10-09 (agent): filed from user feedback; verification, design and delivery in one pass.
  Delivered in the working tree (uncommitted; the user commits on main). GUI check pending.