# M10l: the clipboard fallback stops lying about textarea selections

- **ID**: MOE-0066
- **State**: done
- **Labels**: ready-for-agent, bug, ux
- **Created**: 2026-10-15
- **Updated**: 2026-10-15
- **Commit**: ba454c3
- **Assignee**: agent

Reported from use: "In the text area, there is a possibility of failing to capture the selected text
— after I select text in a textarea (e.g., in the chat composer in the Zed editor), then I launch Moe
with Cmd+Space, the AI command will show that there is no selection. However, this isn't always the
case; there are still some instances where it can capture the text. I don't know the reasons. but, if
it fails in a textarea, the following tries will fail continuously for the following relaunching the
moe with reselecting."

## Diagnosis

The symptom's shape — intermittent first failure, then **sticky** for the same text, surviving a Moe
restart — pointed at state outside the process, and there is exactly one such piece: the pasteboard.
Three defects in the capture path (ADR-0002 amendment, MOE-0013's split capture) explain all of it.

1. **The fallback's verdict compared text identity.** `filter_selection(after, before)` dropped the
   captured text when it equalled the pre-copy clipboard, meaning to catch "the app wrote something
   that is not a text selection". But it cannot tell that apart from **the app copying the same text
   again** — which is precisely what re-selecting the same passage looks like. Once a selection had
   been left on the pasteboard, every retry at that text read as "nothing was copied".
2. **A copy landing after the 100 ms budget was never captured and never restored.** The finisher
   returned `None` at its deadline ("the clipboard was never touched"), and if the app's copy landed a
   moment later, the snapshot was gone and the selection stayed on the user's clipboard. That is what
   seeded defect 1 — a self-poisoning loop, and a clipboard-hygiene bug on its own.
3. **AX's "no answer" and "a caret" were the same value.** `Ax::selected_text` returned
   `Ok(None)` for both, and the strategy then used `AXSelectedTextRange`'s length as the tiebreaker:
   `length <= 0` → `Done(None)` **without running the fallback**. An app that exposes a range but
   reports it stale/zero (custom-drawn editors, exactly the "textarea" class reported) could therefore
   never reach the clipboard path, deterministically per app.

## Design — ADR-0002 amendment

- **Landing is the evidence; the text is the payload.** `changeCount` moving since our own `copy_begin`
  plus non-empty text = the selection. The pre-copy `read_text()` and the `before` field are gone (one
  less pasteboard read on the summon path). An app with nothing to copy does not write.
- **The observation window moved to 500 ms, and it is the panel-*after* window.** The finisher owns it
  off the main thread, so it is not a latency budget: it is how long a slow app gets to publish its
  selection. A copy landing inside it is captured **and** its snapshot restored; the never-landed path
  still touches nothing (no churn).
- **AX answers in three states** (`AxSelection::{Text, Cursor, Opaque}`): text, an explicit empty
  selection (a caret — the fallback is skipped, preserving MOE-0013's "no stray ⌘C" rule), and no
  answer (no focused element, no `AXSelectedText` at any level, walk budget, permission) → the clipboard
  decides. The strategy no longer infers a cursor from a range the app may be lying about; the one
  remaining range use is the contradiction guard (AX says cursor while its own range claims a selection
  → the copy settles it). Cost, stated in the ADR: apps that answer nothing yet report a genuine zero
  range now get one synthesized ⌘C per summon (~20 ms grace, no pasteboard write when nothing is
  selected).

## Delivery

- `crates/moe-platform/src/text_target.rs`: `AxSelection` + the three-state `Ax::selected_text` contract;
  the rewritten gate; `accept_copied` (replaces `filter_selection`, identity check dropped);
  `FALLBACK_BUDGET` 100 → 500 ms; `PendingFallback` loses `before`; tests reworked + three new ones
  (`opaque_ax_with_a_zero_range_still_falls_back`, `cursor_with_a_nonempty_range_still_falls_back`,
  `landed_copy_of_the_same_text_is_the_selection` — the sticky case pinned, `cursor_skips_the_clipboard_fallback`).
- `crates/moe-platform/src/mac_text.rs`: `MacAx::selected_text` reports `Text` / `Cursor` (the existing
  "exists but is empty" log) / `Opaque`, and logs the caret verdict explicitly.
- `docs/adr/0002`: the MOE-0066 amendment (three corrections, in the file's language).

## Acceptance

- `cargo test -p moe-platform` 74 passed; `cargo test --workspace` (9 + 57 + 130 + 74) green;
  `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` clean.
- GUI check pending: in the reported editor, select the same passage repeatedly and summon — the
  Selection Indicator must read the character count every time (never "reading" forever, never "no
  selection" with text selected); the user's clipboard must survive the summons (paste it after a
  summon that reported a selection); a caret in the same editor must still show no selection, and the
  summon must not feel slower (the caret path skips the copy).

## Comments

- 2026-10-15 (agent): filed and fixed in one pass. The diagnosis is a mechanism argument plus the
  three defects being provable in the code (each has a test); the *specific* editor's timing was not
  measured here, so the ADR's first two corrections are the ones to verify against the reported app,
  and both are unconditional improvements. Worth noting for a follow-up: the logs already name the
  verdict (`AXSelectedText exists but is empty — a caret` / `failed at level N` / `read AX range = …` /
  `selection late: …`), so a recurrence can be pinned to a verdict instead of guessed at.
- 2026-10-15 (agent): one deliberate cost, recorded in the ADR: an app that answers neither
  `AXSelectedText` nor a usable range now takes the clipboard path on every summon, so a caret there
  costs one synthesized ⌘C (~20 ms, no pasteboard write). The previous "skip by range" shortcut is
  what made such apps unusable; a keystroke is the cheaper mistake. Delivered as ba454c3.
