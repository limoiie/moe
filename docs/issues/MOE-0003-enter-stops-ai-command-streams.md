# M7k: Enter stops generation on streaming result cards (AI Commands)

- **ID**: MOE-0003
- **State**: done
- **Labels**: bug, ux
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent
- **Commit**: cbdf378

An AI Command (Improve Writing / Make Shorter / …) streams its transform into a result card whose
declared primary action is **Write Back Result** — so pressing Enter mid-stream wrote the
half-generated text back into the host app, even though the action pill already showed
**Stop Generation** (the primary action yields to Stop while `pending`, IIE4AD-365). User feedback:
Enter should stop those generations, as it does on the Quick Ask page (ADR-0036).

## Requirement

While an answer streams, **Enter requests stop generation** on the results layer too (AI Commands
and any future streaming `Item`), not just on the Quick Ask page.

## Decision (ADR-0036 amendment)

- Enter on a `pending` Focused Item runs `stop_generation` — the same platform-wide path as the
  pill and Esc — instead of the item's primary action.
- The pill's Stop row now carries the `⏎` Kbd it always meant (it showed Esc while Enter was the
  primary).
- Esc keeps its first-priority stop on result cards; the Quick Ask page keeps its confirmation
  dialog on Back (that asymmetry is deliberate: on the page Enter sends drafts, so Back needs the
  guard).
- After the stream stops or finishes, Enter is the item's primary again (write back for AI
  Commands, ADR-0024); ⌥⏎ copies the partial/complete text as before.

## Delivery

- `ui/src/main.ts`: the Enter branch stops when `v.mode === "items" && v.items[v.focus]?.pending`
  (before `applyFocused`); `primaryActionOf`'s Stop row shows `applyKeys` (⏎).
- Docs: ADR-0036 amendment; CONTEXT (`Stop` reworded: Enter stops on every surface, Back's
  confirmation is page-only); README (`⏎` keymap row + AI Commands bullet).

## Acceptance

- Start an AI Command with a selection; while it streams, `⏎` stops it (toast "Generation stopped",
  the generated part stays, nothing is written back), and the pill reads "Stop Generation ⏎".
- After stopping, `⏎` writes back and `⌥⏎` copies; Esc still stops mid-stream.
- Quick Ask page unchanged (Enter sends / stops, Back asks first).
- `pnpm build` green.

## Comments

- 2026-10-09 (agent): filed and implemented; shipped in cbdf378 (`pnpm build` + `cargo test --workspace` green).