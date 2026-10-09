# M7q: thinking blocks, verified — the reported stream is the canonical pair

- **ID**: MOE-0009
- **State**: in-progress
- **Labels**: bug
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent
- **Commit**: (pending)

Follow-up to MOE-0008: the user reports the thinking text still reaching the result body and the
clipboard (⌥⏎), showing "thinking" / "response" around it.

## Findings

1. **The splitter is Rust-side.** `pnpm build` rebuilds the frontend only; the running app must be
   recompiled/restarted (`cargo tauri dev` does that on Rust changes, `cargo tauri build` for a
   bundle). The reported behaviour (a card that stays — MOE-0007 — while the reasoning is neither
   split nor rendered as a Thinking block) matches a binary built after MOE-0007 but before
   MOE-0008.
2. **The reported text is the canonical pair** (`…`…`…`, MiniMax-M2's documented format — the M3
   docs note reasoning arrives on `reasoning_content` in the OpenAI-compatible protocol). What the
   report shows as bracket-less "thinking"/"response" tokens is a rendering artifact of one
   markdown/HTML pass collapsing `…` tags; the raw clipboard content carries the angle brackets,
   and MOE-0008's parser handles that shape (this issue adds the regression test that pins it).

## Delivery

- `crates/moe-extensions/src/ai.rs`: `split_reasoning` reorganised into the two named styles
  (canonical `…`/`…` anywhere with a lone-closer fallback; same-delimiter `…` start-anchored), and
  a regression test using the **exact reported stream**: the body must be only the final answer
  line, with no tag bytes in either part.
- No behaviour change beyond MOE-0008 (the tests are the deliverable: they prove the shape the user
  hit is handled).

## Acceptance

- `split_reasoning_strips_the_reported_stream` passes; the existing style tests keep passing.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test
  --workspace` green.
- News for the user: rebuild the app (Rust side) and re-run the same command.

## Comments

- 2026-10-09 (agent): filed; investigation showed the canonical parser handles the reported shape —
  the live leak comes from the not-yet-rebuilt binary. Regression test added; if a rebuild still
  leaks, the next step is capturing the raw SSE bytes for the exact tag spelling.