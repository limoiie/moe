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
  rows in this order:
  - **Open Command** — **live**, not a placeholder: it is exactly the Apply semantics of Enter
    (the row shows the `⏎` Kbd), running the focused command with the same path as Apply
    (frecency recorded, live commands take over the input). Root layer only:
    inside a command "opening" the view you already have would reset it (AI answers), so the row
    is omitted there rather than shown disabled;
  - **Add to Favorites** / **Remove from Favorites** (`⌘⇧F`) — works; toggling re-runs the root
    list so the section updates and toasts the new state. The binding is a platform semantic
    (`SystemKey::Favorite`, default ⌘⇧F) recognized on every surface: at the root it acts on the
    focused command, inside a command on the source command;
  - **Configure Extension** — **disabled placeholder** (dimmed, skipped by ↑↓, never runnable)
    until extension preferences exist.
  The card itself is the shared `createCard` component (ADR-0028), which learns one new concept:
  `CardRow.disabled`.
- **Inside a command** the same **Command** section is appended after the item's **Actions** and
  before **General**, acting on the source command — so favorites are reachable from anywhere the
  command is open, matching how Raycast surfaces command-level actions.

## Cost

- Toggling a favorite at the root re-runs the whole search (an IPC round trip) — the list is
  local and small, so this is cheap; if it ever matters, the Favorites section could be patched
  in place.
- The disabled row is visible-but-inert; it exists to hold the menu shape the user specified
  for Configure Extension. When extension preferences ship, it becomes a real row without
  moving anything.
- Favorites and Suggestions both pin commands out of their groups; a heavily favorited set can
  push source groups far down the root page — acceptable, that is the point of pinning.
## Amendment: the Suggestions row carries its own menu entry

The root menu showed only the three command-level rows, so a command sitting in the Suggestions
section offered no way to forget it from the menu — even though the ⌃X delete slot already
forgot it from the keyboard. The ⌘K card on a Suggestions row now renders
**Remove from Suggestions** (icon: trash, Kbd: `⌃X`, the delete slot's own display string) between
the favorite toggle and the Configure Extension placeholder, and running it takes the delete
slot's path (`delete_suggestion` + refresh).

Two things fell out of this:

- The UI tracked the Suggestions section as "leading entries" (`sections[0]`), but Favorites is
  pushed **before** Suggestions whenever the user has favorites — so the ⌃X slot silently did
  nothing under that condition. The view layer now records the section's real flat range
  (`suggestionsStart` + `suggestionsCount`) and both the slot and the card row ask
  `isSuggestionFocus`.
- The card row only appears on the root layer (Suggestions exist only on the empty query), so the
  in-command Command section keeps its existing three rows.
