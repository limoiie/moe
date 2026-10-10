# M7y: conversations grow a tool loop — the daily agent, faithfully persisted

- **ID**: MOE-0017
- **State**: todo
- **Labels**: ready-for-agent, feature
- **Created**: 2026-10-10
- **Updated**: 2026-10-10
- **Blocks**: MOE-0018
- **Assignee**: agent

The owner wants Moe's AI to serve daily work — researching, answering, querying, explaining —
which needs the model to consult the web and read files mid-turn. Writing and coding stay outside
Moe (Zed / DeepSeek Harness). Direction decided in **ADR-0039** after the landscape research
(`docs/research/agent-harnesses-and-acp.md`): Phase 1 is a minimal in-house tool loop on the
existing pipeline; ACP becomes a later door for External Agents. This issue is Phase 1. Every
decision below was settled with the owner in two grilling rounds (Q1–Q10); the supporting facts
are in `docs/research/free-web-search-options.md` and
`docs/research/deepseek-context-caching-and-dsh.md`.

## Decisions (settled — do not re-litigate without a comment here)

- **Surface**: every *new* conversation is agentic — Quick Ask, the Side View, and the capturing
  row alike; the model decides when to call Tools. AI Commands stay tool-free. `[ai] tools`
  (default true) is a kill-switch for *new* conversations only.
- **v1 Tools**: `web_search` (self-hosted SearXNG), `web_fetch`, `read_file`. `run_command` is
  deferred until the permission-card release; Moe-context tools (clipboard/frontmost/screen) are
  deferred to the Phase-2 MCP-server idea.
- **Permission posture**: read-only tools auto-run — visibility is the permission model (Tool
  Call rows) and Stop is the veto. `read_file` is text-only with a secrets blocklist (`~/.ssh`,
  `~/.gnupg`, `~/.aws`, `.env*`, `~/Library/Keychains`) and a size cap (~256 KB, truncated with a
  note); blocked paths return a tool error the model sees.
- **Search backend**: `[ai] searxng_url` (optional). Tool advertisement is config-driven: no URL →
  `web_search` is absent from the request. `GET {url}/search?q=…&format=json`, top ~8 results
  (title/url/snippet) as compact text. 403 (JSON disabled — SearXNG's default) or unreachable →
  tool error to the model ("search instance unreachable — answer from knowledge or say so") + one
  stderr diagnostic (MOE-0011 pattern). Moe never manages the service; README documents
  `docker run` + `formats: [html, json]`.
- **Fetch chain**: `reqwest` GET (browser-like UA, redirect cap, 20 s, ~1 MB byte cap) →
  `readability` → `htmd` (skip `html2md`, GPL). On 403/429/empty-shell: keyless
  `https://r.jina.ai/<url>` fallback, **default on**, `[ai] fetch_fallback = "none" | "jina"`.
- **Guardrails**: max **8 Rounds** per turn (forced settle), **20 s** per tool call, **~8k chars**
  per result (truncation noted inside the result).
- **History = faithful persist (dsh-style)**: append-only record — questions, answers, reasoning,
  tool calls, tool results; every later request replays it whole with `tools` attached and
  `reasoning_content` echoed on **every** assistant message (documented DeepSeek 400 otherwise).
  Never mutate a warm prefix: no history rewriting, stable embedded system-prompt head.
  Pre-feature conversations stay plain forever (capability flag on the conversation). No
  compaction in v1 — ⌘N is the escape hatch.
- **Stop semantics** (extends ADR-0006/0036/0038, changes nothing): Stop aborts the in-flight tool
  HTTP call *and* the loop; the partial answer is kept and persisted as today; running rows settle
  as stopped; no further Round fires.
- **System prompt**: embedded, ~1k tokens (pi-style minimalism), instructs inline URL citations
  for web-sourced claims; not user-editable in v1.

## Design

- **Loop orchestration** lives above the existing pipeline: a round runner repeatedly builds a
  request (history + `tools`), streams it, and either settles (no tool calls → final answer) or
  dispatches the called tools and appends their results as the next round's messages. Cancellation
  propagates into tool HTTP calls (the existing per-conversation cancel flag).
- **SSE parsing grows**: `delta.tool_calls` fragments (first chunk carries id/type/function name,
  later chunks only argument fragments — OpenAI-style index accumulation) and
  `delta.reasoning_content`, both pure functions in `ai_client.rs` with unit tests, beside
  `sse_delta`.
- **Record schema**: messages carry structured parts (answer / reasoning / tool_call /
  tool_result) instead of bare text; `Role` grows the tool-role wire values for replay; old rows
  load as plain answers. Replay builds the messages array verbatim from parts — including
  `reasoning_content` on assistant messages whenever `tools` is present.
- **UI — the activity stack**: inside the streaming bubble (Quick Ask) and the Side View stream
  alike, Thinking rows and Tool Call rows render **in arrival order** above the answer text — one
  row per call, chronological, never aggregated ("Used 3 tools" is rejected: observability over
  magic). Row anatomy: tool icon + plain label (`Searched «query»`, `Fetched example.com/path`,
  `Read ~/x.log`) + status (shimmer while running — `prefers-reduced-motion` gated like Thinking;
  failed/stopped calls say so). Collapsed by default, no auto-expand; expanding shows the
  truncated result as muted markdown, no inner scroll; toggles survive per-frame re-renders
  (MOE-0012 carry-over). The item payload grows `toolCalls` beside `reasoning`; `FrameBuilder`
  extends accordingly (ADR-0006 contract absorbs it as payload, no new frame kinds). Reopening an
  agentic conversation re-renders reasoning + Tool Call rows from the record.
- **Config** (`moe-platform/src/config.rs`): `[ai] tools` (bool, default true),
  `[ai] searxng_url` (optional string), `[ai] fetch_fallback` ("jina" default | "none"). **No
  keychain change** — SearXNG is keyless; the only key stays the AI key (ADR-0019).

## Delivery

- `crates/moe-extensions/src/ai_client.rs`: tool-call/reasoning delta parsing, tools-carrying
  request-body builder, pure + unit-tested.
- `crates/moe-extensions/src/ai.rs` + a new tools module (e.g. `agent.rs` / `tools.rs`): round
  runner, tool dispatch (searxng / fetch+extract / read_file), guardrails, cancellation,
  persistence of structured parts. New deps: `htmd`, `readability` (pin; unmaintained since 2023).
- `crates/moe-core/src/conversation.rs` + `crates/moe-platform/src/db.rs`: message parts schema +
  migration (existing rows = plain answers); the conversation's agentic flag.
- `crates/moe-platform/src/config.rs`: the three new `[ai]` keys.
- `ui/src/messages.ts`, `ui/src/main.ts`, `ui/src/icons.ts`, `ui/src/styles.css`: Tool Call rows,
  the activity stack, reopen re-render, toggle carry-over.
- Docs at delivery (MOE-0012 pattern): README (feature + SearXNG setup), config sample comments,
  CONTEXT.md terms (Tool / Tool Call row / Round / Agentic Conversation + the Thinking
  amendment), ADR-0005 amendment (network boundary) — the glossary and amendment land with this
  issue's doc pass.

## Acceptance

- `searxng_url` set: a current-events question in Quick Ask shows a shimmering `Searched «…»`
  row, then the answer streams below with inline source URLs; expanding the row shows the
  truncated results as muted markdown.
- `searxng_url` unset: requests carry no `web_search` tool; the model answers from knowledge; no
  error or notice surfaces anywhere.
- SearXNG with JSON disabled (its default 403): the model receives the tool error and says it
  could not search; one stderr diagnostic line; no crash, no hang.
- Multi-round: a question needing search + fetch runs within 8 Rounds; every call renders as its
  own row in arrival order, interleaved with Thinking rows; the 9th round cannot fire (forced
  settle).
- Stop mid-round (Enter while pending): the in-flight tool aborts, the partial answer persists,
  its row settles as stopped, and no further round fires — on the Quick Ask page and the Side View
  alike, with the Back-confirms dialog intact (ADR-0038).
- Reopen an agentic conversation (history list / ⌘J): Tool Call rows and `Thought for N seconds`
  re-render from the record; a follow-up turn replays the whole history and DeepSeek's `usage`
  shows `prompt_cache_hit_tokens > 0` (manual verification; note the cache is best-effort with a
  hours-to-days TTL).
- Old (pre-feature) conversations: follow-ups behave exactly as today — no tools offered, no 400s.
- `read_file`: reads an ordinary text file (capped); a blocklisted path (`~/.ssh/…`, `.env`)
  returns a tool error the model can see; binary files are refused with a tool error.
- `web_fetch`: a normal page returns readable markdown; a JS-shell/403 page retries via
  r.jina.ai when `fetch_fallback = "jina"`, and fails honestly to the model when `"none"`.
- AI Commands are unchanged: transform requests carry no tools.
- `cargo test` and `pnpm build` green.

## Comments

- 2026-10-10 (agent): filed from two grilling rounds with the owner — Q1 surface (conversations
  agentic everywhere), Q2 tool set + injection posture, Q3 search backend (owner chose
  self-hosted SearXNG over Tavily-free-key/DDG-scrape), Q4 history (owner's KV-cache objection
  verified against DeepSeek docs — faithful persist, dsh-style; compaction deferred; old
  conversations stay plain), Q5 stop/guardrails, Q6 docs packaging, Q7 SearXNG shape, Q8 Jina
  fallback default-on, Q9 glossary, Q10 Tool Call row UI. Direction in ADR-0039; facts in the
  three research notes. Implementation not started.
- 2026-10-10 (agent): renumbered — the MOE-0016 / M7x slot was allocated concurrently by
  `MOE-0016-model-catalog-refresh` (already in-progress in this worktree); this issue is MOE-0017
  / M7y. Cross-references (ADR-0039, the ADR-0005 amendment, both research notes) point at the
  new id.
- 2026-10-10 (agent): **Blocks**: MOE-0018 added — the provider layer (three protocols, ADR-0040)
  rewrites `ai_client.rs` / `ai.rs` / `config.rs` and is sequenced after this loop so the
  openai-chat wire behavior (tool-call parsing, message-array construction, reasoning echo) stays
  byte-compatible.
- 2026-10-10 (agent): claimed; foundation chunk landed in the working tree (commit with this
  comment): `delta.reasoning_content` now parses (`SseLine::Reasoning`) and streams into the
  Thinking payload in `run_stream` — previously the field was ignored entirely; `delta.tool_calls`
  fragments parse (`SseLine::ToolCalls`) with index accumulation (`merge_tool_call_fragments`); the
  tools-carrying request builder (`chat_body_with_tools`, no `tools` key when nothing is
  advertised); and `[ai] tools` / `searxng_url` / `fetch_fallback` parse with defaults
  (`moe-platform/src/config.rs`). Six new tests (client parsers, merge order, body builder, a
  mock-server end-to-end for reasoning_content, reasoning preference);
  `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo test --workspace` green
  (extensions 58). Remaining: round runner + tools module (searxng / fetch / read_file), message
  parts schema + migration, UI activity stack, docs pass.
