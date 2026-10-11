# M9g: the tool loop speaks Anthropic — MiniMax models get web search

- **ID**: MOE-0035
- **State**: in-progress
- **Labels**: feature
- **Created**: 2026-10-11
- **Updated**: 2026-10-11
- **Assignee**: agent

The owner's ask: "I want minimax models to use web search tools as well." MOE-0026 built the tool
loop on the openai-chat wire only (`agent.rs` hardcodes `Protocol::OpenAiChat`; the gate in
`ai.rs` is `config.ai.tools && provider.protocol == Protocol::OpenAiChat`) — its own comment names
this issue as the follow-up that generalizes it (ADR-0040). MiniMax's providers resolve to the
**Anthropic** protocol, so MiniMax-M3 (the owner's active model) never sees a tool. Verified
against MiniMax's live docs (platform.minimax.io/docs, 2026-10-11): their Anthropic-compatible
surface fully supports `tools`/`tool_use`/`tool_result` — it is their Claude-Code surface.

## Facts driving the design (verified 2026-10-11, primary sources)

- **Wire shape** (docs.anthropic.com + MiniMax Messages API reference): tools are
  `{name, description, input_schema}`; a call streams as `content_block_start`
  (`content_block: {type:"tool_use", id, name, input:{}}`) + `content_block_delta`
  (`{type:"input_json_delta", partial_json}`) + `content_block_stop`; the turn ends with
  `message_delta.stop_reason = "tool_use"`. Results ride a **user** message:
  `content: [{type:"tool_result", tool_use_id, content}]`, immediately after the assistant
  message, results first in the content array. `tool_result.content` may be a plain string.
- **Thinking replay**: assistant thinking blocks carry a `signature` that must be "returned
  unchanged in later turns" (MiniMax) / passed back "complete and unmodified" within a tool-use
  turn under penalty of 400 (real Anthropic). MiniMax M3 is an interleaved-thinking model.
  `signature_delta` streams after the thinking deltas (currently ignored by our parser).
- **MiniMax deviations to code around**: `thinking.type` whitelist is `disabled|adaptive` only —
  **today's `enabled`+`budget_tokens` body is invalid there** (M3.1 even 400s on `disabled`;
  M2.x ignores it and always thinks); `tool_choice` only `auto|none` (we send none — fine);
  `is_error` absent from their tool_result schema; `input_json_delta` missing from their
  documented delta enum but proven in practice (their official Vercel provider wraps the
  unmodified `@ai-sdk/anthropic` SSE parser; Claude Code cannot work without it) — handle both
  partial deltas and a complete `input` in `content_block_start`; ids look like
  `call_function_…` not `toolu_…` (never pattern-match prefixes); `max_tokens` counts thinking
  tokens; responses may carry an extra `base_resp` object.
- Real Anthropic deprecated `thinking.type:"enabled"` on 4.6 and rejects it on 4.7+ — the
  provider-id split below keeps legacy behavior for the `anthropic` provider only; adapting that
  provider to `adaptive`+`output_config.effort` per model generation is a separate concern.

## Decisions

- **Scope: the Anthropic protocol only.** `openai-responses` keeps its own follow-up (its tool
  shape — `function_call` items, reasoning item ids — shares nothing with this work). The gate
  becomes `matches!(protocol, OpenAiChat | Anthropic)`.
- **The OpenAI-shaped wire stays the shared intermediate** (mod.rs's contract): `wire_messages`
  gains a `reasoning_signature` sibling to `reasoning_content` on assistant messages;
  `anthropic::body` normalizes — assistant `tool_calls` → `thinking`(text+signature)? + `text`? +
  `tool_use`… blocks, consecutive `role:"tool"` messages fold into **one** user message of
  `tool_result` blocks (parallel calls), error results prefix `Error: ` into the content
  (no `is_error` — MiniMax's schema lacks it, and both dialects read the text).
- **Thinking parameter by provider id**: `minimax*` → reasoning on = `{"type":"adaptive"}`,
  reasoning off = omit (never send `disabled` — M3.1 400s); `anthropic` → unchanged
  (`enabled`+budget). Carried on `ChatRequest` as an explicit thinking-style value resolved in
  `ai.rs`, so the adapter stays a pure function.
- **Signature persistence**: `Message.reasoning_signature: Option<String>` + a
  `reasoning_signature TEXT` column via the existing idempotent `ALTER TABLE` migration pattern;
  captured from a new `SseLine::ReasoningSignature` variant; replayed verbatim. A reconstructed
  single thinking block matches MiniMax's documented example `[thinking, tool_use]`; exact
  multi-thinking-block interleaving inside one assistant message is out of scope (undocumented on
  MiniMax; the live loop keeps the round's assembled blocks in memory anyway).
- **SSE parsing**: `anthropic::sse_delta` gains `content_block_start`(tool_use) →
  `ToolCalls(fragment{index, id, name})`, `input_json_delta` → `ToolCalls(fragment{index,
  arguments})`, a complete non-empty `input` in `content_block_start` → one fragment carrying the
  serialized arguments (MiniMax fallback path), `signature_delta` → `ReasoningSignature`. The
  shared `merge_tool_call_fragments` needs no change (index-keyed, id/name from carrier).
- **Runner**: `AgentTurnRequest` carries `protocol`, `reasoning`, thinking style; the round body
  goes through `request_body(protocol, …)` instead of `chat_body_with_tools`; openai-chat output
  stays byte-identical (mod.rs's byte-compatibility rule).
- **Tools themselves are untouched**: specs stay OpenAI-shaped (the adapter converts), guardrails
  / cancellation / activity-stack UI / persistence flow are protocol-neutral already.

## Delivery

- `crates/moe-core/src/conversation.rs`: `Message.reasoning_signature` (serde default).
- `crates/moe-platform/src/db.rs`: the column + migration + read/write.
- `crates/moe-extensions/src/ai_client/mod.rs`: `SseLine::ReasoningSignature`, `ChatRequest`
  gains `tools` + the thinking style.
- `crates/moe-extensions/src/ai_client/anthropic.rs`: tool-bearing body (spec conversion,
  wire-message normalization incl. thinking+signature and folded tool_results), SSE tool/signature
  parsing, tests for each documented MiniMax shape.
- `crates/moe-extensions/src/ai_client/openai_chat.rs`: `body()` grows the tools passthrough
  (byte-compatible).
- `crates/moe-extensions/src/agent.rs`: protocol/reasoning/style plumbing, signature capture into
  `RoundEnd` and the record, `wire_messages` signature echo.
- `crates/moe-extensions/src/ai.rs`: the gate + turn construction (both call sites).
- Docs: README (the "chat-protocol providers only" caveat lifts for Anthropic), ADR-0040 增补 if
  the decision needs recording, MOE-0034 comment caveat retracted.

## Acceptance

- Active pair minimax-cn / MiniMax-M3, `searxng_url` set: a current-events question in Quick Ask
  streams a `Searched «…»` Tool Call row and an answer citing URLs; the record persists and a
  follow-up replays it (thinking signature included) without a 400.
- A reasoning-on MiniMax request sends `thinking:{"type":"adaptive"}` (never `enabled`, never
  `disabled`); an `anthropic`-provider reasoning request is byte-identical to today.
- Parallel tool calls in one round replay as one user message with N `tool_result` blocks,
  results first.
- Error results reach the model as `Error: …` content (no `is_error` key on the wire).
- openai-chat regressions: the MOE-0026 runner tests pass unchanged; `wire_messages` output for
  chat-protocol conversations is byte-identical (no `reasoning_signature` key when absent).
- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `pnpm -C ui build` green.

## Comments

- 2026-10-11 (agent): filed from the owner's ask; MiniMax + Anthropic wire facts verified against
  platform.minimax.io/docs and docs.anthropic.com the same day (deviations list above). The
  openai-responses protocol is explicitly a separate follow-up.
- 2026-10-11 (agent): delivered in the working tree. `conversation.rs` gained
  `reasoning_signature`; `db.rs` the column + idempotent migration + read/write; `mod.rs`
  `SseLine::ReasoningSignature` + `ChatRequest.tools`/`.anthropic_thinking` + the
  `AnthropicThinking` dialect enum; `anthropic.rs` the tool-bearing body (spec conversion,
  thinking(+signature)-first assistant blocks, `tool_result` runs folded results-first into the
  next user message or standing alone at the record's end) and the tool/signature SSE parsing
  (both `input_json_delta` fragments and MiniMax's complete-input-in-start shortcut); `providers.rs`
  `anthropic_thinking()` (minimax* → adaptive, anthropic → legacy enabled+budget, reasoning-off →
  omit); `agent.rs` the protocol/reasoning/max_tokens/dialect plumbing + signature capture and
  echo; `ai.rs` both gates → `matches!(OpenAiChat | Anthropic)`, `continue_agentic` likewise,
  `RoundEnd::Settled.reasoning_signature`; `ai_commands.rs` + the responses test constructor
  carry the new fields. Docs: README's protocol sentence, ADR-0040 amendment, MOE-0034 caveat
  retracted, the settings-card blurb. Validation: `cargo test --workspace` green (app 6, core 53,
  extensions **121** incl. the two new anthropic tests, platform 70), `cargo clippy --workspace
  --all-targets -- -D warnings` clean, `pnpm -C ui build` green. Awaiting commit; live acceptance
  pending: a MiniMax-M3 Quick Ask with `searxng_url` set should stream a `Searched «…»` row, and
  a follow-up turn must replay the signed thinking without a 400.
