# Favorites: a curated section above Suggestions, and the root-layer command menu

The root page had no actions at all: `⌘K` / `⌘⇧P` returned early outside a results layer, so a
focused command could not be acted on. This ADR gives the root layer a command menu and adds the
first of its actions for real: **Favorites** — user-curated commands pinned into their own
section above Suggestions (Raycast's Favorites slot).

## Decision

- **Storage**: `data_dir/moe/favorites.json` (`Favorites { ids: [...] }`, insertion-ordered) —
  platform-level like frecency (ADR-0026 store module), outside every Namespace: any extension's
  command can be favorited. Two IPCs: `toggle_favorite(id) -> bool` (persists immediately) and
  `is_favorite(id) -> bool` (the card's label).
- **Search**: `Registry::search` takes the `Favorites` set and, on an **empty query**, emits
  sections in the order **Favorites → Suggestions → source groups**. Favorites render in user
  order; ids whose commands no longer exist (extension removed/renamed) are skipped silently.
  A command pinned in Favorites is **not repeated** in Suggestions or in its source group; the
  same dedup applies to Suggestions versus groups (ADR-0023 kept). Non-empty queries are
  unchanged — favorites are a root-page affordance.
- **The root actions card** (⌘K / ⌘⇧P on the command layer) shows one **Command** section with
  three rows, in this order:
  - **Open Command** — **disabled placeholder**, always first (the menu shape is fixed; skipped by
    ↑↓, never runnable);
  - **Add to Favorites** / **Remove from Favorites** (`⌘⇧F`) — works; toggling re-runs the root
    list so the section updates and toasts the new state. The binding is a platform semantic
    (`SystemKey::Favorite`, default ⌘⇧F) recognized on every surface: at the root it acts on the
    focused command, inside a command on the source command;
  - **Configure Extension** — **disabled placeholder** (dimmed, skipped by ↑↓, never runnable).
  The card itself is the shared `createCard` component (ADR-0028), which learns one new concept:
  `CardRow.disabled`.
- **Inside a command** the same **Command** section is appended after the item's **Actions** and
  before **General**, acting on the source command — so favorites are reachable from anywhere the
  command is open, matching how Raycast surfaces command-level actions.

## Cost

- Toggling a favorite at the root re-runs the whole search (an IPC round trip) — the list is
  local and small, so this is cheap; if it ever matters, the Favorites section could be patched
  in place.
- Disabled rows are visible-but-inert; they exist to hold the menu shape the user specified.
  When Open Command / Configure Extension ship, they become real rows without moving anything.
- Favorites and Suggestions both pin commands out of their groups; a heavily favorited set can
  push source groups far down the root page — acceptable, that is the point of pinning.