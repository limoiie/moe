# M8a: the list–detail page template — grouped lists as contract

- **ID**: MOE-0019
- **State**: done
- **Labels**: feature, ux
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent
- **Commit**: e7b1e70

User feedback: "group the chat sessions of the chat history in the side-chat window properly" and
"group the chat sessions (list) in the search-chat-history's page as well, which is list-details
left-right two-panes page, and abstract this layout into a general page template, so that other
commands who have similar list-details ui&ux pattern can directly adopt it by infilling data,
inheriting the actions&keybindings directly, preserving the identical behavior".

## Design — ADR-0018 amendment

- **`Item.group`** (optional, camelCase, omitted when None): a free-form label the platform renders
  as a non-focusable header before the first item of each group. Navigation, focus, the ⌘K card's
  Actions section and the delete slots keep counting the items only — the source-section rule
  (ADR-0020) applied inside a list. Ungrouped lists keep their old payload and rendering.
- **Buckets** (`moe-extensions::recency`): Today / Yesterday / Previous 7 Days / Previous 30 Days /
  Older, duration-based (no calendar/timezone handling), used by the chat history page. The Side
  View's history card mirrors the same labels in `ui/src/chat.ts` (it cannot call into Rust for a
  label) and groups its rows the same way.
- **The template, written down**: any command returning `ActionResult::list(items)` inherits the
  layout, the keyboard semantics (↑↓/⌃N/⌃P, ⏎, ⌥⏎, ⌘K, ⌃X/⌃⇧X, Esc/⌫) and the streaming path;
  adopting it is filling `Item`s and implementing the hooks the actions route to.

## Delivery

- `crates/moe-core/src/contract.rs`: `Item.group` plus a wire-shape test; `CommandEvent` gains the
  `large_enum_variant` allow (a per-delta streaming event must not box the Item).
- `crates/moe-extensions/src/recency.rs`: `group_label` with edge tests (incl. future timestamps).
- `crates/moe-extensions/src/ai.rs`: history items carry the bucket as `group`.
- `ui/src/types.ts` + `ui/src/main.ts`: items mode renders group headers through the existing
  `sectionHeaderEl` (non-focusable; the flat focus index counts items only).
- `ui/src/chat.ts`: the history card groups its rows with the same labels; highlight and navigation
  skip headers (and scrolling targets the focused row by `data-index`).
- Every other `Item` literal gains `group: None` (the field is required to construct, optional on
  the wire).
- Docs: ADR-0018 amendment + a CONTEXT.md glossary entry for the template.

## Acceptance

- Rust: `cargo test --workspace` green (recency edges, the `group` wire shape); `cargo fmt --all
  --check`, `cargo clippy --workspace --all-targets -- -D warnings` green.
- UI: `pnpm -C ui build` green.
- GUI check pending: the Search Chat History page shows Today / Yesterday / … headers that ↑↓
  skips; the Side View's ⌘P card groups the same way and its highlight never lands on a header;
  every ungrouped list page renders exactly as before.

## Comments

- 2026-10-09 (agent): filed from user feedback; delivered in the working tree (uncommitted).
  Milestone prefix guessed as M8a — rename if the milestone series differs.
- 2026-10-09 (agent): closed as part of the stack commit at the user's request; commit e7b1e70.