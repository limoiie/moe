# M7p: thinking blocks are not the answer — strip them from the result, render them as Thinking

- **ID**: MOE-0008
- **State**: done
- **Labels**: bug, ux
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent
- **Commit**: 15e34cf

User feedback: models like minimax-m3 stream their chain of thought inside the content
(`…reasoning…answer`, sometimes with the opening tag dropped by the server), so the reasoning text
became the result: it showed in the card, got copied, and (before MOE-0007) was written back.

## Requirement

1. Strip the think part from the final result — the answer is what streams, persists, copies and
   writes back.
2. Render the reasoning properly in the UI (not as part of the answer).

## Decision

- `ai::run_stream` splits every accumulated frame into `(reasoning, answer)` before building the frame
  (`split_reasoning`): handles `…`, `<thinking>…</thinking>`, a closing tag with no opener (servers
  that strip it), unterminated blocks (still thinking — the answer stays empty while the indicator
  runs) and multiple blocks; the answer gets the text around the blocks.
- The answer is the only thing emitted as the item body, persisted, copied and written back; the
  reasoning rides the item **payload** (`reasoning`) so the UI can render it.
- The UI renders the payload's reasoning as a collapsible, muted **Thinking** block above the answer:
  open while the stream runs (watch it live), folded once it settles, reopenable by hand. One shared
  builder (`messages.ts`) serves the result card and the chat bubbles (panel Quick Ask + Side View).
- Chat history keeps the answer only: reasoning is live-view material, not conversation content.

## Delivery

- `crates/moe-extensions/src/ai.rs`: `split_reasoning` + unit tests; `FrameBuilder` gains the
  reasoning argument; `run_stream` emits/persists the answer; `answer_item` payload carries
  reasoning.
- `crates/moe-extensions/src/ai_commands.rs`: `transform_item(title, answer, reasoning, pending)`.
- `ui/src/messages.ts`: `reasoningEl` + `updateStreaming(text, pending, reasoning?)`.
- `ui/src/main.ts` (result card + panel chat frames) and `ui/src/chat.ts` (Side View frames).
- Docs: ADR-0024 amendment (Chinese 增补); README (AI Commands / Quick Ask bullets).

## Acceptance

- A transform through a thinking model shows the reasoning in a Thinking block and the clean answer
  as the body; write-back and copy contain only the answer (the stopped/notice paths included).
- A stream stopped mid-thinking shows the partial thinking (block) and "(generation stopped)".
- Quick Ask bubbles behave the same; history reloads show the answer without the block.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test
  --workspace`, `pnpm build` green.

## Comments

- 2026-10-09 (agent): filed from user feedback (minimax-m3).
- 2026-10-09 (agent): delivered in `15e34cf`; `cargo fmt --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test --workspace` (148), `pnpm build` green. GUI check
  pending: run a transform through minimax-M3 — the answer is clean, the reasoning shows in the
  Thinking block, and copy/write-back carry the answer only.