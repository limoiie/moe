# M10d: the Chat About Selection command goes

- **ID**: MOE-0058
- **State**: done
- **Labels**: ready-for-agent, refactor
- **Created**: 2026-10-13
- **Updated**: 2026-10-13
- **Commit**: 55f2ab2

Reported from use: "remove the chat about selection command -- it is useless."

MOE-0056 added `ai.chat-about-selection` so a selection-targeted conversation was reachable *at all*:
the capture row only fires when the query matches nothing, so with text selected and an empty query
there was no path to one. What the command proved in use is that the **page**, not a command, is where
a selection chat begins: `ai.quick-ask` already carries the captured selection into the conversation's
first message, and ⌘' opens that page directly. With MOE-0059 showing the target on the page itself,
the command was a second, narrower door to the same room — and it made the selection look like
something you must pick a command to use.

## Decision

- **The command and its routing go**: the `CommandMeta`, `chat_about_selection`, its two match arms
  (`invoke`, `invoke_streaming`), and the test that pinned it. `panel_ask` and the capture row are
  untouched — they are the paths that remain, and the composer's chip now makes the second one
  discoverable.
- **Its icon leaves with it**: `messages-square` had no other caller, and ADR-0012's rule is that an
  icon name leaves the map when its last caller does.
- **MOE-0056's file stays as history**, with a comment pointing here; the ADR-0036 amendment records
  the replacement.

## Acceptance

- [x] No command id, symbol or icon reference to `ai.chat-about-selection` / `messages-square`
      remains in the code (the issue history keeps the name, by design).
- [x] `just test` green (122 extension tests, was 123 — the removed test).
- [x] The capture row, `ai.quick-ask` and `panel_ask` are untouched: a selection chat still exists,
      it just starts from the page.

## Comments

- 2026-10-13 (agent): filed and closed together with MOE-0059 — the two are one decision, the
  selection's home is the page rather than a command.
