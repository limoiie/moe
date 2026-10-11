# The daily agent: an in-house tool loop now, ACP as the external-agent door later

Moe's AI is plain streaming chat (ADR-0005/0024/0036): one request, one answer, no tool calls.
Daily work — researching, answering, querying, explaining — keeps hitting that ceiling: the model
cannot consult the web, fetch a page, or open a file mid-turn. Meanwhile the heavyweight agent
work is deliberately *outside* Moe's scope (the owner writes in Zed and DeepSeek Harness; coding
lives in Zed). The question researched in `docs/research/agent-harnesses-and-acp.md`: build an
agent loop in-house, or adopt ACP (Agent Client Protocol) and connect existing agents. Both
products that matter point the same way — Zed (ACP's author) still keeps a native loop for its
core UX and uses ACP only for *other people's* agents; Raycast (Moe's UX model) rebuilt around an
in-house loop with daily-life tools and adopted MCP/Skills but **not** ACP.

## Decision

- **Phase 1 (this ADR; MOE-0026): a minimal in-house tool loop on the existing pipeline.**
  Conversations created from now on are **Agentic Conversations**: the request advertises Tools,
  the model may call them mid-turn, and Moe orchestrates the Rounds on top of the existing
  HTTP/SSE machinery (`ai::run_stream`) — guard-railed at 8 Rounds per turn, 20 s per tool call,
  ~8k chars per result (truncation noted inside the result). v1 Tools: **Web Search**
  (self-hosted SearXNG), **Web Fetch**, **Read File**. All are read-only and **auto-run:
  visibility is the permission model** — every call renders a Tool Call row in the Thinking-row
  visual language, and Stop (platform-wide, ADR-0006/0036 amendments) remains the veto. The
  permission card (Allow `↵` / Always `⌘↵` / Deny `Esc`, Raycast-style) arrives with the first
  write/execute tool, which is explicitly *not* v1. AI Commands stay tool-free one-shot
  transforms (ADR-0024 unchanged).
- **Search is $0 and self-hosted: SearXNG.** `[ai] searxng_url` in config; tool advertisement is
  config-driven (no URL → `web_search` is not in the request at all). Moe never manages the
  service — README documents the docker run and the `formats: [html, json]` line; a 403 (JSON
  disabled, SearXNG's default) or unreachable instance is a tool error the model sees plus one
  stderr diagnostic. Extraction needs no vendor: local GET → `readability` → `htmd`, with the
  keyless `r.jina.ai` fallback (fires only after a local failure; `[ai] fetch_fallback` disables
  it). Alternatives and the DeepSeek "no server-side search" dead end:
  `docs/research/free-web-search-options.md`.
- **Agentic Conversations persist faithfully, dsh-style — never collapse.** The record is
  append-only (questions, answers, reasoning, Tool Calls, tool results); every later request
  replays it whole, with `tools` attached and `reasoning_content` echoed on every assistant
  message. This is not sentimentality, it is verified economics: DeepSeek's automatic disk cache
  matches **prefix units** — an exact continuation hits the full history at 1/50 of miss pricing
  (flash: $0.003 vs $0.15/M), while a collapsed transcript diverges right after the last user
  message and re-bills the rewritten tail as a miss every turn; the cross-turn reasoning-echo rule
  makes collapse a documented 400 trap; and chat-completions cannot insert tool calls
  mid-conversation, so a collapsed turn can never be reconstructed. Costly tool results are never
  re-run: they stay in the record. Compaction is **deferred** (1M-token context; ⌘N is the escape
  hatch) — when it comes, it follows dsh: summarize-and-replace under pressure only, accepting the
  cache loss at that re-baseline, never mutating a warm prefix otherwise. Mechanics and quotes:
  `docs/research/deepseek-context-caching-and-dsh.md`. This **amends ADR-0024's** "history stores
  only the answer" for Agentic Conversations (the answer text stays reasoning-free everywhere;
  reasoning becomes persisted replay material, re-rendered on reopen, still never copied or
  written back) — see the amendment on ADR-0005 too.
- **Conversations created before this feature stay plain forever.** The capability flag rides on
  the conversation: plain conversations never send `tools` (their stored assistant messages have
  no reasoning to echo — the 400 trap), and Agentic Conversations keep sending `tools` on every
  later request even if the `[ai] tools` kill-switch flips (their history contains tool rounds;
  the kill-switch governs *new* conversations only).
- **Phase 2 (direction only, no commitment): an ACP client as a separate door for External
  Agents.** When heavyweight agents are wanted inside the palette — DeepSeek Harness (ships an ACP
  server profile), Gemini CLI (native), Claude (`claude-agent-acp`) — Moe adds the
  `agent-client-protocol` Rust crate (Apache-2.0, 1.0+ since 2026-06, powers Zed; Tauri apps
  already exist in the client list), pins protocol **v1**, implements `session/request_permission`
  (and optionally `fs`; **no `terminal`**), gives each conversation a working directory (ACP
  requires an absolute `cwd`), and installs agents from the ACP Registry JSON. The Phase-1 UI —
  Tool Call rows, the permission card, plan/activity rendering — is exactly what ACP's
  `tool_call`/`request_permission`/`plan` notifications render into, so nothing here is throwaway.
  Re-evaluate at ACP v2 stabilization (steering/queueing fit a palette). The bare noun **"Agent"**
  stays undefined in the glossary until then, reserved for this External Agent sense.
- **Rejected:** *ACP-first* — daily questions need immediacy (no subprocess spawn, login, or `cwd`
  before "explain this error") and Moe's differentiator is system context (Selection, Finder
  files, clipboard) that no external coding agent has and ACP's fs/terminal capabilities don't
  cover — plus distribution: Moe stays "one API key and it works" (ADR-0005/0011/0019). *pi RPC
  or dsh as the primary engine* — preview/single-vendor protocols, TS-extension customization in
  a Rust shop; both remain reachable through Phase 2 (`pi-acp`, dsh's ACP profile). *Codex
  `app-server` direct* — labeled "experimental… not supported for production," commercial-use auth
  policy limits, chat-completions wire removed, and the community ACP adapter already exists.
  *Paid or card-gated search* (Tavily/Brave/OpenRouter `:online`) — the owner's $0 constraint;
  free-key APIs stay viable behind the search seam.

## Cost

- Every agentic request carries tool definitions (~200 tokens) and the whole replayed record;
  growth bills at cache-hit prices, but the prefix must never be rewritten — no history edits, no
  post-hoc system-prompt changes (dsh's in-history lesson). The system prompt is a stable embedded
  head (~1k tokens, pi-style; instructs inline URL citations; not user-editable in v1).
- The message schema grows: messages carry structured parts (answer / reasoning / tool_call /
  tool_result) instead of bare text; `FrameBuilder` and the item payload grow a `toolCalls` array
  beside `reasoning` (ADR-0006's contract absorbs it as payload, not new frame kinds). Existing
  rows migrate as plain answers.
- Moe's network boundary widens past ADR-0005's "only the configured endpoint": the user's
  SearXNG instance, arbitrary fetched URLs, and the optional r.jina.ai fallback (one URL, only
  after a local failure). Still no vendor accounts and no new keys — SearXNG is keyless; the
  keychain keeps exactly one AI key (amendment recorded on ADR-0005).
- Read-only-auto means a prompt-injected page could direct Read File/Web Fetch within the
  blocklist's blind spots; the v1 mitigations are the secrets blocklist (`~/.ssh`, `~/.gnupg`,
  `~/.aws`, `.env*`, `~/Library/Keychains`), size caps, full visibility of every call, and Stop.
  The real answer (graded permissions) ships with the first write/execute tool.
- Phase 2 is deliberately vague: ACP v2 is a moving draft, adapters are community-maintained
  (pi-acp is one-maintainer MVP software; codex-acp has moved orgs once), and terminal-type auth
  needs an interactive-terminal handoff Moe doesn't have yet. None of that blocks Phase 1.
