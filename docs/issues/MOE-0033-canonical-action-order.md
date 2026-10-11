# M9e: every actions card respects one canonical action order

- **ID**: MOE-0033
- **State**: done
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-11
- **Updated**: 2026-10-11
- **Assignee**: agent
- **Commit**: 8b06425

The owner's ask: the ⌘K actions menu on every page should sort its rows in one predefined order —
the primary action and the secondary action always on top, then the page's own actions, then
Delete / Delete All, then the settings, then the global settings — and every actions menu on every
page should respect that order automatically, instead of each page and each source deciding its
own. A follow-up review added: keep the original sections (ordered by the canonical order, with
thin separators between the groups inside a section); show the active provider · model under the
chat titles; rename the switcher row to **Change Model** and bind it to `⌥M`.

## Decisions

- **The vocabulary** (`ui/src/actions.ts`, ADR-0037 amendment): `primary` (the page's ⏎ action:
  Apply / Open Command, or Stop while a stream runs) → `secondary` (the ⌥⏎ action) → `action`
  (the object's / page's own rows: New, Browse, Change Model, Open in Side View, re-run…) →
  `destructive` (Delete ⌃X / DeleteAll ⌃⇧X, or Remove Chat / Remove All Chats) → `settings`
  (Configure Extension) → `global` (the app's escape hatches: Open Settings, Open Config File,
  Hide Side View). A row that declares nothing is a plain `action`.
- **Rationale for the order**: the first two slots mirror the two key bindings (⏎ / ⌥⏎), so the
  card's top always matches what the keys do; then everything the card is *about* (the object and
  its record), closed by the irreversible delete; then everything the *app* is about (extension
  settings, then global rows) — context-free navigation never sits between the user and the
  record's own actions. The owner's sketch (delete before settings, global settings last) is kept.
- **Enforced by the plan, not by call sites**: `createActionPlan(...).sections(ctx)` sorts each
  source's rows by rank — stable inside a rank (the source's own order decides ties) — and orders
  the sections by their **topmost row's slot** (ties keep registration order). A row declares what
  it *is*, never where it goes.
- **The sections stay** (owner review): Actions / Command / General on the panel, Chat / App /
  Window in the Side View, each keeping its label. Inside a section, the plan marks the first row
  of every rank group after the first (`separator`) and the card draws a thin rule above it, so
  the canonical order is visible without flattening the sections. Separators hide while filtering,
  like headers.
- **The chat titles carry the active pair** (ADR-0036 amendment): `Provider · Model` under the
  conversation title — smaller and muted — on the panel's chat page (the Input Bar row) and in the
  Side View's titlebar; it follows a switch immediately and is fetched on page entry / window show
  (`ai_active_pair`); an unset pair leaves the line hidden.
- **`Change Model` (`⌥M`)**: the switcher row is renamed from "Set AI Model" and its Kbd comes
  from the new platform semantic `SystemKey::ChangeModel` (`moe-core::keymap`, default ⌥M — the
  view layer never invents keys). The key lands the same action as the row (the model switcher) on
  the chat surfaces only, matching the physical `KeyM` (⌥M types "µ" on macOS layouts).

## Delivery

- `ui/src/actions.ts`: `ActionRank` / `ActionRow` / `ActionSource.title`; `sections()` sorts rows
  by rank, orders sections by their topmost row, marks rank groups with `separator`.
- `ui/src/card.ts`: `CardRow.separator` — a hairline above the row (hidden while filtering).
- `ui/src/keymap.ts`: the shared `keyDisplay` map + `loadKeyDisplay()` (both windows read the
  platform table once; the panel's local copy folded into it).
- `ui/src/main.ts`: the three panel sources rank their rows (Open Command `primary`; Stop
  Generation `primary` while streaming; Copy Last Answer `secondary`; item actions by their
  declared kind; Remove from Suggestions / Delete / Delete All `destructive`; Configure Extension
  `settings`; Open Settings `global`); the chat row renamed to Change Model (`⌥M`); the `⌥M`
  handler on the chat page; the title subtitle (`chatPair` + `refreshChatPair`).
- `ui/index.html`: the query row wraps the input with the muted `#query-subtitle`.
- `ui/chat.html` + `ui/src/chat.ts`: the centered title block with `#chat-model`; the Chat / App /
  Window sources restored and ranked; the `⌥M` handler; the pair fetch on show.
- `crates/moe-core/src/keymap.rs`: `SystemKey::ChangeModel` + the ⌥M table row (count test 17 →
  18) + a binding test.
- Docs: ADR-0037 amendment (the canonical order, sections, separators), ADR-0036 amendment (the
  title pair, ⌥M), CONTEXT (`Show All Actions`, `Model Switcher`), README (action-pill bullet, key
  table, model switcher, Quick Ask page, Side View).

## Acceptance

- On every page of both surfaces the card reads top-down: ⏎ action, ⌥⏎ action, the object's own
  actions, Delete / Delete All, Configure Extension, the app's global rows — with the same
  semantic row in the same slot everywhere; sections keep their labels and order by their topmost
  row; adjacent rank groups inside a section are hairline-separated.
- Both chat titles show `Provider · Model` under them, smaller and muted; switching through the
  switcher (row or `⌥M`) updates the line in place.
- `⌥M` opens/closes the model switcher on the chat surfaces; the row's Kbd is the platform
  table's `⌥M`.
- `pnpm -C ui typecheck` and `pnpm -C ui build` green; `cargo clippy --workspace --all-targets --
  -D warnings` and `cargo test --workspace` green.

## Comments

- 2026-10-11 (agent): filed and delivered in one pass from the owner's request; ADR-0037 amended.
- 2026-10-11 (agent): **revision (owner review)** — the sections came back: they now order by
  their topmost row and hairline-separate the rank groups inside (the first pass had flattened the
  card into one headerless list, which the owner rejected). Same pass: the chat titles gained the
  active `Provider · Model` subtitle, and "Set AI Model" became **Change Model** on `⌥M`
  (`SystemKey::ChangeModel`; the panel and the Side View share one `keyDisplay` map read from the
  platform table). Verified with a throwaway Node run of the real plan: sections read
  Actions → Command → General (panel) / Chat → App → Window (side), rows sorted with separators
  exactly at the rank transitions. Manual check left: the visual pass of both cards and both
  titles in the running app.
