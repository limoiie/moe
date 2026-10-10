# Agent harnesses and ACP: build a daily-use agent in-house or adopt the protocol

Research note, 2026-10-10. Every claim below cites a primary source (official docs, spec, source
code, release pages, first-party blogs) fetched live on 2026-10-09/10; URLs are in
[Sources](#sources). Claims not verified against a live primary source are marked in
[Verification notes](#verification-notes).

## Context

Moe is a keyboard-first macOS command palette (Tauri 2, Rust backend, vanilla TS UI). Its AI
today is **plain streaming chat, not an agent**: `ai_client.rs` builds
`POST {base_url}/chat/completions` bodies and parses SSE deltas by hand; `ai::run_stream` owns the
HTTP/SSE loop, cancellation, persistence, and reasoning-wrapper stripping; there is **no tool
calling and no agent loop** anywhere (ADR-0005, ADR-0024, ADR-0036).

The goal being researched: agentic capability for **daily work** — researching, answering,
querying, explaining — explicitly **not** for long-form writing (the owner uses Zed and DeepSeek
Harness for that) and **not** for coding (Zed). The decision on the table:

> Build a small agent loop in-house on the existing pipeline, **or** become an ACP (Agent Client
> Protocol) client and connect existing agents (Zed's externals, pi, Codex, DeepSeek Harness, …)?

**Outcome (2026-10-10):** the staged hybrid below was adopted — **ADR-0039**, issue **MOE-0017** —
with two amendments from the design grilling: search is **self-hosted SearXNG** (not a free-tier
API; see `free-web-search-options.md`) and agentic history **persists faithfully** for DeepSeek's
prefix cache (see `deepseek-context-caching-and-dsh.md`).

Five areas were researched in parallel against primary sources: ACP + Zed, pi, OpenAI Codex,
DeepSeek (official harness + API), and Raycast (the product Moe's UX is modeled on).

## Findings

### ACP (Agent Client Protocol)

- **Ownership/status.** Created by Zed Industries, now **jointly governed by Zed and JetBrains**
  under the `agentclientprotocol` GitHub org, with a stated goal of moving to an independent
  foundation. Apache-2.0, no CLA. Current **stable protocol version is `1`** (a single integer
  negotiated at `initialize`); **v2 was published as a draft on 2026-07-20** (breaking: ID-based
  patch semantics, structured diffs, richer permissions, "beyond the turn" background updates and
  steering) with guidance to support v1 and keep v2 behind flags.
  (https://agentclientprotocol.com/community/governance.md,
  https://agentclientprotocol.com/announcements/acp-v2-draft.md)
- **Transport.** JSON-RPC 2.0; local agents run as **subprocesses of the client over stdin/stdout**;
  remote HTTP/WS transports are explicitly work-in-progress. Conventions: camelCase keys, absolute
  paths, 1-based lines, Markdown default content format, MCP JSON types reused.
  (https://agentclientprotocol.com/get-started/architecture.md,
  https://agentclientprotocol.com/protocol/v1/overview.md)
- **Session lifecycle.** `initialize` (version + capability negotiation, agent returns
  `authMethods`) → optional `authenticate` → `session/new` (**requires `cwd`**, takes the client's
  `mcpServers` config) → `session/prompt` → agent streams `session/update` notifications → turn
  ends with a `stopReason`. Optional capability-gated methods: `session/load` (history replay),
  `session/resume`, `session/list`, `session/delete`, mode switching, `logout`.
  (https://agentclientprotocol.com/protocol/v1/session-setup.md, .../prompt-turn.md)
- **Update types (complete v1 list).** `user_message_chunk`, `agent_message_chunk`,
  `agent_thought_chunk`, `tool_call`, `tool_call_update`, `plan`, `available_commands_update`,
  `current_mode_update`, `config_option_update`, `session_info_update`, `usage_update` (tokens +
  cumulative cost), `notice`, compaction updates.
  (https://agentclientprotocol.com/protocol/v1/prompt-turn.md)
- **Tool calls & permissions.** Tool calls carry `kind` (`read`/`edit`/`delete`/`search`/`execute`/
  `think`/`fetch`/…), `status`, content (text/image/**diff**/embedded terminal), `locations`,
  raw input/output. Before executing, the agent may call **`session/request_permission`** with
  `allow_once`/`allow_always`/`reject_once`/`reject_always` options; clients may auto-approve per
  user settings. (https://agentclientprotocol.com/protocol/v1/tool-calls.md)
- **Client capabilities an app may provide.** Baseline is `session/request_permission`. Optional,
  advertised: **`fs.readTextFile`/`fs.writeTextFile`**, **`terminal/*`** (create/output/wait/kill —
  the heaviest surface), `elicitation`, `auth.terminal` (relaunch the agent binary interactively
  for login). Capabilities not advertised must not be called by conformant agents — a client can
  legitimately ship with permissions-only.
  (https://agentclientprotocol.com/protocol/v1/initialization.md, .../file-system.md,
  .../terminals.md)
- **Auth.** Agents advertise `authMethods`; type `agent` = in-protocol `authenticate`, type
  `terminal` = client relaunches the agent's own program in an interactive terminal. Otherwise
  **auth, API keys, billing, and model config are the agent's business, not the protocol's**.
  (https://agentclientprotocol.com/protocol/v1/authentication.md,
  https://zed.dev/docs/ai/external-agents)
- **Rust SDK is production-grade.** Crate **`agent-client-protocol` 3.2.0** (2026-10-08),
  Apache-2.0, **1.0.0 since 2026-06-24**, ~5.07M downloads, "powers the integration with external
  agents in the Zed editor." Implement the `Client` trait; features include `process`
  (subprocess spawning) and `stdio`; `examples/client.rs` is the starting point. TS SDK
  `@agentclientprotocol/sdk` 1.7.0 also exists (Gemini CLI is its reference production agent).
  (https://crates.io/api/v1/crates/agent-client-protocol,
  https://agentclientprotocol.com/libraries/rust.md,
  https://agentclientprotocol.com/announcements/sdk-1-0-releases.md)
- **Ecosystem (~40 agents, 30+ clients).** Agents: **Gemini CLI (native**, Google's reference
  implementation), **Claude** via `zed-industries/claude-agent-acp` adapter (~2.6k stars),
  **Codex** via `agentclientprotocol/codex-acp` adapter, **GitHub Copilot CLI** (native, public
  preview since 2026-01-28), **Cursor** (native), OpenCode, Goose, Junie (JetBrains), Cline, Qwen
  Code, Mistral Vibe, Kimi CLI, Factory Droid, Poolside, **DeepSeek Harness** (ACP server profile),
  **pi** (via community `pi-acp` adapter). Clients: Zed, JetBrains AI Assistant, Neovim
  (CodeCompanion, agentic.nvim, avante.nvim), Emacs (agent-shell.el), marimo, Qt Creator, Sublime
  Text, VS Code extensions, Obsidian plugins, Unity, Visual Studio (Poolside), a Microsoft
  Windows-Terminal fork, and **Tauri-based desktop apps (Jockey, OctoShell)** — direct existence
  proofs that a small Tauri app can be an ACP client. A curated **ACP Registry** JSON
  (`cdn.agentclientprotocol.com/registry/v1/latest/registry.json`) carries distribution metadata
  for one-click agent install. (https://agentclientprotocol.com/get-started/agents.md,
  .../clients.md, .../registry.md)

### Zed's own architecture (the ACP author's product)

- Zed runs **three agent paths side by side**: (1) **Zed Agent** — native, in-editor loop with
  built-in tools, MCP servers, tool permissions, checkpoints, profiles; (2) **External Agents** —
  ACP subprocesses installed from the ACP Registry or configured as custom
  `agent_servers` entries (`{command, args, env}`); (3) **Terminal Threads** — raw CLI/TUI.
  (https://zed.dev/docs/ai/agents.md, https://zed.dev/docs/ai/external-agents)
- Even Zed keeps some features **native-only because ACP v1 can't express them** — e.g. steering
  queued messages, since "Zed can't detect turn boundaries for external agents" (a v2 target).
  Zed can import threads from external agents and forward its MCP config to them.
  (https://zed.dev/docs/ai/agent-panel,
  https://agentclientprotocol.com/announcements/acp-v2-draft.md)
- Lesson: the protocol's author still found an in-house loop necessary for its core UX; ACP is
  the **interop layer for other people's agents**, not a replacement for a native one.

### pi (Earendil; formerly badlogic/pi-mono)

- **Moved and productized.** `github.com/badlogic/pi-mono` redirects to
  **`github.com/earendil-works/pi`**; Mario Zechner joined Armin Ronacher's company Earendil
  (2026-04-08). MIT core "forever, non-negotiable"; future value-add tiers may be Fair Source
  (none exist yet). Latest **v1.1.0 (2026-10-07)**; 113.8k stars; pushed daily; `pi-ai` ~24.7M
  npm downloads/month. **OpenClaw** (Peter Steinberger's personal assistant) is powered by pi;
  Vercel ships an official `@ai-sdk/harness-pi` adapter.
  (https://github.com/earendil-works/pi, https://mariozechner.at/posts/2026-04-08-ive-sold-out/)
- **Positioning (README):** "a minimal, extensible agent harness that you can make your own…
  ships with powerful defaults but skips features like sub-agents and plan mode." Built-in tools:
  `read`, `write`, `edit`, `bash` — system prompt + tool defs < 1000 tokens; optional read-only
  tools (`grep`/`find`/`ls`) selectable via `--tools`. TypeScript/Node ≥ 22.19; **standalone
  platform binaries ship with each release** (Bun-compiled — no Node needed on user machines).
  Providers include DeepSeek and **any OpenAI-compatible endpoint** via `models.json`.
  (https://github.com/earendil-works/pi, https://mariozechner.at/posts/2025-11-30-pi-coding-agent/,
  https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/providers.md)
- **Embedding surfaces for a Rust app:** ① **`pi --mode rpc`** — long-lived subprocess controlled
  via strict JSONL on stdin/stdout (id-correlated commands `prompt`/`steer`/`abort`/`set_model`/…,
  streamed events `message_update`/`tool_execution_*`/`agent_settled`), documented for "other
  languages, isolated processes, IDEs, custom clients" with a minimal Python client example;
  ② one-shot `pi --mode json`; ③ TypeScript SDK (Node-only, not usable from Rust in-process);
  ④ **no native ACP** — the community `pi-acp` adapter (svkozak, MIT, ~252k weekly downloads,
  listed in Zed's ACP registry) spawns `pi --mode rpc` underneath, with known fidelity gaps (no
  thought stream, no fs/terminal delegation).
  (https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/rpc.md,
  https://github.com/svkozak/pi-acp)
- **Permission model is YOLO by design** — no prompts; safety comes from OS-level sandboxing or
  tool restriction. A host app exposing pi to end users must build its own gating.
  (https://github.com/earendil-works/pi README, "Permissions & Containerization")
- **Direct existence proof for Moe's use case:** Mario's kids' voice assistant "Pipi" runs a
  **daily-use, non-coding agent on the pi stack** — local models via llama.cpp, Rust STT/TTS
  workers over stdio, pi tools for web search, memory, Spotify, camera.
  (https://mariozechner.at/posts/2026-05-30-shitty-robot/)
- **Design principles from his writings worth stealing** (2025-11-30 flagship post;
  "What if you don't need MCP at all?" 2025-11-02): own your context completely (~1000-token
  system prompt, user-visible and editable); files are state (TODO.md/PLAN.md over built-in
  plan mode); progressive disclosure beats always-on tool catalogs (popular MCP servers burn
  7–9% of a context window before work starts); observability over orchestration magic (no
  black-box sub-agents); design the loop for production from day one (abort everywhere, streaming
  events, explicit "settled" semantics); minimalism is benchmark-validated (four tools were
  enough for a competitive Terminal-Bench 2.0 run).

### OpenAI Codex

- **Form.** Rust (`codex-rs/`, ~80 crates), Apache-2.0, **0.162.0** (2026-10-08); distributed via
  installer script, npm launcher, brew cask, platform binaries. Interfaces: TUI, **`codex exec`**
  (non-interactive, `--json` JSONL events, `--output-schema`, read-only sandbox by default),
  **`codex app-server`**, TS/Python SDKs, IDE extension, cloud.
  (https://github.com/openai/codex, https://learn.chatgpt.com/docs/codex/cli.md,
  https://learn.chatgpt.com/docs/non-interactive-mode.md)
- **Official embedding surface is `app-server`**: bidirectional JSON-RPC 2.0 over stdio
  (Thread/Turn/Item primitives, streamed `item/*` deltas, approvals as server-initiated requests,
  auth RPCs, `generate-json-schema`/`generate-ts`). But the docs label it **"experimental… not
  supported for production workloads"** even though the VS Code extension and both SDKs run on it.
  `codex proto` is gone; **`codex mcp-server` was removed** ("not a drop-in replacement… update
  your integration to use the app-server protocol"); Codex remains an MCP **client**.
  (https://learn.chatgpt.com/docs/app-server.md, https://learn.chatgpt.com/docs/mcp-server.md)
- **No native ACP.** The ecosystem path is the **`agentclientprotocol/codex-acp`** adapter (stdio
  ACP agent that starts Codex app-server and translates; `npx -y @agentclientprotocol/codex-acp`;
  originally `zed-industries/codex-acp`, now archived; community-maintained, Apache-2.0; supports
  `INITIAL_AGENT_MODE=read-only|workspace-write|agent|agent-full-access`).
  (https://github.com/agentclientprotocol/codex-acp, https://zed.dev/docs/ai/external-agents)
- **Provider flexibility narrowed sharply:** `wire_api = "chat"` was **removed** (announced
  2025-12-09, landed ~Feb 2026) — only the **Responses API** wire works now, so plain
  chat-completions endpoints (DeepSeek's native API) need a translating proxy… except DeepSeek
  now ships a **native Responses API "specifically adapted for Codex"** with an official one-click
  setup script (see below). (https://github.com/openai/codex/discussions/7782,
  https://raw.githubusercontent.com/openai/codex/main/codex-rs/model-provider-info/src/lib.rs,
  https://api-docs.deepseek.com/guides/responses_api)
- **Auth policy is the sharpest edge for third-party apps:** "App-server authentication has never
  been permitted for commercial or hosted services" — the sanctioned route for **open-source and
  locally hosted apps** is Sign in with ChatGPT token sharing (OAuth DCR, preview limitations:
  `store:false`, no `temperature`, restart-on-token-refresh). API-key mode sidesteps policy but
  loses plan-credit economics. (https://learn.chatgpt.com/docs/app-server.md,
  https://developers.openai.com/siwc/token-sharing-open-source.md)
- **Daily non-coding fit:** workable but not the center of gravity — `sandbox=read-only` +
  `approvalPolicy=never` + `web_search=live|indexed` (a hosted OpenAI tool with cached-index mode
  for lower prompt-injection exposure) gives a safe research loop, but every path drags in
  repo-centric scaffolding (git-repo check, AGENTS.md, worktrees, review), and no documented
  pure chat-only mode exists. (https://learn.chatgpt.com/docs/web-search.md,
  https://learn.chatgpt.com/docs/agent-approvals-security.md)

### DeepSeek: official harness ("dsh") + API agent capabilities

- **DeepSeek Harness exists and is official.** `deepseek-ai/deepseek-harness` ("dsh"), MIT,
  TypeScript on the Cordis plugin framework, ~246k stars, pinned in their org. Ships as **Web UI**
  (`npx @deepseek-ai/dsh web`), **Electron desktop app (macOS/Windows)**, headless runner,
  **TS + Python SDKs**, and an **ACP server profile** (`@agentclientprotocol/sdk` in the dependency
  tree). Positioned for **daily knowledge work first**: "整理资料、分析表格、撰写文档、制作汇报",
  sourced research, scheduled/background tasks, plugin ecosystem, Creator mode. Status:
  **developer preview** — "THERE WILL BE COMPATIBILITY-BREAKING CHANGES"; releases from
  v0.1.0-rc.7 (2026-08-17) to v0.2.1-alpha.2 (2026-10-09); their own note: "has not been
  security-audited, and sandboxing, approvals, and permissions do not guarantee isolation."
  It can also host **Claude Code and Codex as subagents**.
  (https://github.com/deepseek-ai/deepseek-harness, https://www.deepseek.com/harness,
  https://registry.npmjs.org/@deepseek-ai%2Fdsh/latest,
  https://github.com/deepseek-ai/deepseek-harness/releases)
- **dsh tool catalog** (daily-work oriented): `web_search`/`web_fetch`, bash/PTY, fs incl.
  `read_image`, glob/grep (bundled ripgrep), `todo_write`, goals, **`schedule_*` (cron/daily
  reminders)**, background jobs, subagents/agent-teams, workflows, `run_code`, skills, MCP,
  `present` (deliverable files), `ask_user_question`, plan mode, browser use (experimental),
  bundled Python with python-docx/pptx/openpyxl/pandas for Office documents.
  (https://raw.githubusercontent.com/deepseek-ai/deepseek-harness/master/docs/tool-catalog.md)
- **API: everything needed for an in-house tool loop is documented.** Tool calling on both
  current models (`deepseek-flash` = V4.1-Flash, `deepseek-v4-pro`) in thinking **and**
  non-thinking mode; **streaming tool calls** (`delta.tool_calls` fragments, OpenAI-style);
  `strict` JSON-schema tool mode (Beta); `reasoning_content` returned alongside `content` —
  **critical quirk: when the request carries `tools`, prior `reasoning_content` must be echoed
  back on every subsequent request or the API returns 400**; `tool_choice: required`/named tools
  400 in thinking mode. Three wire formats: Chat Completions, **native Anthropic Messages**
  (`/anthropic`, with Claude model-name mapping), **native Responses API** (Codex-adapted,
  stateless). 1M-token context; flash pricing $0.15/$0.30 per M input off-peak/peak (cache hit
  $0.003/$0.006), $0.6/$1.2 output — an in-house agent loop on flash is economically trivial.
  Official **Agent Integrations** docs cover Claude Code, Codex (one-click script), OpenCode, pi,
  Qwen Code, Copilot, Cline, and more.
  (https://api-docs.deepseek.com/guides/tool_calls, https://api-docs.deepseek.com/guides/thinking_mode,
  https://api-docs.deepseek.com/api/create-chat-completion, https://api-docs.deepseek.com/quick_start/pricing,
  https://api-docs.deepseek.com/quick_start/agent_integrations/claude_code)

### Raycast (the UX Moe is modeled on)

- **v2 (GA 2026-08-25) repositioned the whole product around an in-house agent.** Changelog
  (v2.3, 2026-09-11): "We rebuilt the core of Raycast AI. You can now give AI Chat a task and it
  works out a plan, uses your extensions, writes and runs code, checks the results, and keeps
  going until it's finished… when it needs a decision from you, it stops, asks you, and then
  continues." (https://www.raycast.com/changelog, https://www.raycast.com/ai)
- **Tools = built-in "AI Extensions" aimed at daily life, not coding:** `@terminal`,
  `@finder`/`@file-search`, `@browser` (reads the open page via Browser Companion), `@clipboard`,
  `@calendar`, `@reminders`, `@raycast-notes`, `@snippets`, `@memory`, `@screen-awareness`,
  `@selected-text`, Web Search, image generation. **MCP client** (stdio + HTTP, full OAuth).
  **Skills**: auto-discovers `SKILL.md` from `~/.claude/skills`, `~/.agents/skills`, etc. — the
  open Agent Skills standard. **Agents** = renamed v1 "Presets": saved name/instructions/model/
  scoped-tools setups, one built-in ("Deep Research"), each a Root Search item with its own
  hotkey/alias. **Projects** = shared instructions + memory + a working directory (honors
  AGENTS.md). **Automations** = scheduled agent runs.
  (https://manual.raycast.com/ai/ai-extensions, .../model-context-protocol, .../skills,
  .../agents, .../projects, .../automations)
- **Screen Awareness**: hotkey-captured context bundles (a11y text, selection, screenshot, browser
  tab, Finder files) sent to AI — and exposed as `@screen-awareness` so **the model can decide to
  look at your screen mid-run**. (https://manual.raycast.com/ai/screen-awareness)
- **Permissions**: global Ask/Auto/Always-Allow per tool (Auto default on paid plans: read-only,
  working-dir edits, routine git auto-approved; credentials/destructive always ask); approval card
  with plain-language description + "Show Command", **Allow `↵` / Always allow `⌘↵` / Deny `Esc`**,
  batch Allow-All/Deny-All, user-tunable auto-review rules. An always-on **"Ask User Question"**
  tool renders a multiple-choice card inline mid-run.
  (https://manual.raycast.com/ai/ai-extensions, https://manual.raycast.com/ai/ai-chat)
- **Provider strategy**: one interface over many providers (OpenAI, Anthropic, Google, Perplexity,
  Mistral, xAI, DeepSeek V4.x, Qwen, Kimi, …) + Auto model; **BYOK and custom OpenAI-compatible
  providers exist but now require Pro** (changed 2026-09-08); **"Bring Your Own Subscription"**
  (2026-09-10) literally **shells out to the user's local Claude Code / Codex CLI** for auth +
  inference while Raycast stays the tool host — subscription reuse without an agent protocol.
  (https://manual.raycast.com/ai/bring-your-own-key, .../bring-your-own-subscription,
  .../custom-providers, https://www.raycast.com/changelog)
- **Raycast did NOT adopt ACP.** Zero mentions across the full manual index, developer docs tree,
  v2 changelog, and blog (checked 2026-10-09). Its interop bets are **MCP (as client) + Agent
  Skills + AGENTS.md**; the agent loop stays in-house. All AI features are Pro-gated
  ($10/mo + usage credits; free tier has no AI at all).
  (https://manual.raycast.com/ai, https://www.raycast.com/pricing,
  https://manual.raycast.com/ai/ai-credits)
- **UX patterns that map 1:1 onto Moe's existing vocabulary:** Quick AI answers **inside the
  launcher window** (type → `Tab` → stream → `↵` pastes back into the previous app — Moe's
  Quick Ask + Write Back) with `⌘J` escalating to the full chat window preserving history
  (Moe's Materialize/Side View); `@`-mentions as the universal context bus (Moe's `@path`
  attachments, ADR-0010); agents as root-search citizens with hotkeys; one-shot prompts as
  first-class commands (Moe's AI Commands, ADR-0024).
  (https://manual.raycast.com/ai/quick-ai, https://manual.raycast.com/ai/ai-chat)

## Fit for Moe

### The three real options

| | **A. In-house minimal loop** (tool calling on `run_stream`) | **B. ACP client** (connect external agents) | **C. Embed one harness** (pi RPC / dsh subprocess) |
| --- | --- | --- | --- |
| Effort to first value | Medium: parse streaming `delta.tool_calls`, loop orchestration, 2–4 tools, permission card. Reuses stream/history/attachments/thinking-split | Medium: `Client` trait + subprocess plumbing is SDK-provided; the work is permission UI, agent install/manage UX, cwd concept, terminal-auth handoff | Low-medium: JSONL bridge to a documented protocol (pi `--mode rpc`) or dsh's ACP profile |
| Latency/weight for "explain this error" | Minimal — same HTTP path as today | Heavy: spawn Node agent subprocess, login state, `cwd` required per session | Heavy-ish: subprocess + Node ecosystem |
| Distribution | Zero new deps (ADR-0011 macOS-first story intact) | Users must install agent CLIs (npx/brew) + create accounts/keys | Ship/download a third-party binary; pi ships standalone binaries |
| Auth/economics | Own API key; DeepSeek flash ≈ free ($0.15–0.30/M in) | Agent-owned; unlocks **subscription reuse** (Claude/ChatGPT/Gemini free tier, dsh) | Agent-owned (pi BYO key incl. any OpenAI-compatible endpoint) |
| UX control | Total | Rendering is yours (typed notifications incl. thoughts/tool-calls/plans/usage), loop behavior is the agent's | Loop behavior is the harness's; pi extensions allow hooks in TS |
| Daily-life tools (selection, clipboard, Finder, frontmost app) | Built natively — **no external agent has these** | Not covered by ACP's fs/terminal capabilities; must expose via an MCP server passed at `session/new` | Same — via pi extensions / dsh plugins |
| Provider flexibility | Any OpenAI-compatible chat-completions endpoint (ADR-0005 intact); DeepSeek tool-loop quirks documented | Per-agent: Codex = Responses-only (+ policy limits); Gemini/Claude = own accounts; dsh/pi = flexible | pi: very flexible; dsh: DeepSeek-first |
| Ecosystem risk | None but your own code | Protocol stable (v1) with v2 churn ahead; adapters community-maintained (pi-acp is one-maintainer MVP; codex-acp moved orgs once) | pi: single-vendor, fast-moving, Fair Source signals; dsh: self-declared breaking-change developer preview, not security-audited |
| Heavy tasks (deep research, doc work) | Weak — you'd rebuild what dsh/Zed already do | **Strong — that's exactly what external agents are for** | Medium |

### Reading the tea leaves

1. **Both products that matter here converged on the same split.** Zed (ACP's author) keeps a
   *native* loop for its core UX and uses ACP for *other people's* agents; Raycast (Moe's UX
   model) rebuilt an *in-house* loop with daily-life tools and adopted MCP/Skills but **not**
   ACP. Nobody ships a great daily-use palette agent purely by proxying coding agents.
2. **Moe's differentiation is system context + immediacy, not agent horsepower.** Selection,
   Finder files, clipboard, frontmost app, a summon key — no external agent has these, and ACP's
   client capabilities (fs/terminal) don't cover them. For the stated daily needs
   (research/answer/query/explain), the required tool set is tiny: `web_search`, `web_fetch`,
   `read_file` (attachments already exist), optionally a gated `run_command`. pi's
   benchmark-validated minimalism (4 tools) and Raycast's tool list both say the same thing.
3. **The heavy tasks the owner already outsources.** Writing → Zed/dsh; coding → Zed. Moe should
   not try to out-agent those; it should stay the fast keyboard surface. That undercuts the main
   argument for ACP-first.
4. **But ACP is the cheap escape hatch, and it's personal.** The owner already runs DeepSeek
   Harness — **dsh ships an ACP server profile**, so a future Moe ACP client could summon the
   same agent for deep research from the palette. The Rust SDK is 1.0+, Apache-2.0, and
   battle-tested by Zed; Tauri apps already exist in the client list. Pin v1, implement
   permissions (+ optionally fs), skip terminal, and the plumbing is bounded.
5. **Phase 1 UI work is not throwaway.** Tool-call rows (reuse the Thinking-row visual language,
   ADR-0024 amendments), a permission card (Raycast's Allow `↵` / Always `⌘↵` / Deny `Esc` maps
   onto Moe's keymap), and plan rows are exactly what ACP's `tool_call` /
   `session/request_permission` / `plan` notifications render into in Phase 2.

### Recommendation: staged hybrid — build the minimal loop now, adopt ACP as the external-agent door later

- **Phase 1 — in-house daily agent (do this).** Extend the existing pipeline with a tool-calling
  loop over chat-completions: parse streaming `delta.tool_calls`; tools `web_search` (a search
  API — Raycast v2.7 uses Tavily/Exa/Firecrawl/Brave/Perplexity/Jina for its custom providers),
  `web_fetch`, `read_file` (generalize attachments), later a permission-gated `run_command`;
  render tool calls as collapsible rows above the answer (Thinking-row pattern); one permission
  card component; keep ADR-0005's single-endpoint model. Honor DeepSeek's quirks: echo
  `reasoning_content` back whenever `tools` is present (else 400), never send
  `tool_choice: required` in thinking mode. Stay on pi's principles: ~1k-token visible system
  prompt, files as state, no sub-agents, abort everywhere.
- **Phase 2 — ACP client as a separate capability ("External Agents").** Add
  `agent-client-protocol` (Rust, v1), spawn registry agents as subprocesses, implement
  `session/request_permission` (+ `fs` optionally; **no terminal** initially), give each
  conversation a working directory (the `cwd` requirement), install agents from the ACP Registry
  JSON, and surface `authMethods` with a Terminal.app handoff for `terminal`-type login. First
  targets: **dsh** (owner already uses it), **Gemini CLI** (free tier, native ACP), Claude via
  `claude-agent-acp`. Optionally ship a tiny Moe MCP server exposing system context
  (selection/clipboard) and pass it via `session/new` — that is how external agents get Moe's
  superpower. Re-evaluate at ACP v2 stabilization (steering/queueing fits a palette).
- **Do not:** embed pi's RPC or dsh as the *primary* engine (proprietary/preview protocols,
  vendor drift, TS-extension customization in a Rust shop; pi stays reachable via `pi-acp` in
  Phase 2); talk to Codex `app-server` directly (experimental label + commercial-use auth policy
  + the ACP adapter exists); build coding-agent features (diffs, checkpoints, worktrees) — out of
  scope by the owner's own statement.

## Sources

**ACP**
- https://agentclientprotocol.com/overview/introduction , /get-started/architecture.md , /get-started/agents.md , /get-started/clients.md , /get-started/registry.md
- https://agentclientprotocol.com/protocol/v1/overview.md , /session-setup.md , /prompt-turn.md , /tool-calls.md , /initialization.md , /file-system.md , /terminals.md , /authentication.md
- https://agentclientprotocol.com/community/governance.md , /announcements/acp-v2-draft.md , /announcements/sdk-1-0-releases.md , /libraries/rust.md , /libraries/typescript.md
- https://github.com/zed-industries/agent-client-protocol , https://crates.io/api/v1/crates/agent-client-protocol , https://registry.npmjs.org/@agentclientprotocol/sdk
- https://github.com/zed-industries/claude-agent-acp , https://github.com/agentclientprotocol/codex-acp , https://github.com/svkozak/pi-acp

**Zed**
- https://zed.dev/docs/ai/agents.md , /docs/ai/zed-agent.md , /docs/ai/agent-panel , /docs/ai/external-agents

**pi**
- https://github.com/earendil-works/pi (README, packages/, docs: how-pi-works.md, rpc.md, rpc-commands.md, json.md, sdk.md, extensions.md, providers.md, session-format.md)
- https://api.github.com/repos/earendil-works/pi , https://api.github.com/repos/earendil-works/pi/releases/latest , https://registry.npmjs.org/@earendil-works%2fpi-coding-agent/latest
- https://mariozechner.at/posts/2025-11-30-pi-coding-agent/ , /posts/2026-04-08-ive-sold-out/ , /posts/2026-05-30-shitty-robot/ , /posts/2025-11-02-what-if-you-dont-need-mcp/ , /posts/2025-08-15-mcp-vs-cli/

**Codex**
- https://github.com/openai/codex , https://api.github.com/repos/openai/codex/contents/codex-rs , https://registry.npmjs.org/@openai/codex/latest
- https://learn.chatgpt.com/docs/codex/cli.md , /docs/non-interactive-mode.md , /docs/app-server.md , /docs/codex-sdk.md , /docs/mcp-server.md , /docs/auth.md , /docs/web-search.md , /docs/agent-approvals-security.md , /docs/config-file/config-advanced.md , /docs/open-source.md , /docs/developer-commands.md?surface=cli
- https://github.com/openai/codex/discussions/7782 , https://raw.githubusercontent.com/openai/codex/main/codex-rs/model-provider-info/src/lib.rs , https://raw.githubusercontent.com/openai/codex/main/codex-rs/cli/src/main.rs
- https://developers.openai.com/siwc/token-sharing-open-source.md , /siwc/token-sharing-open-source/codex-app-server.md , /siwc/token-sharing-open-source/preview-limitations.md

**DeepSeek**
- https://github.com/deepseek-ai/deepseek-harness (+ /releases, docs/architecture.md, docs/tool-catalog.md) , https://www.deepseek.com/harness , https://registry.npmjs.org/@deepseek-ai%2Fdsh/latest
- https://api-docs.deepseek.com/ , /updates , /guides/tool_calls , /guides/thinking_mode , /guides/anthropic_api , /guides/responses_api , /api/create-chat-completion , /quick_start/pricing , /quick_start/agent_integrations/claude_code (and /codex, /opencode, /openclaw, /hermes, /qoder, /reasonix, /workbuddy)
- https://github.com/deepseek-ai/awesome-deepseek-agent , https://openrouter.ai/deepseek

**Raycast**
- https://www.raycast.com/ai , /pricing , /changelog , /core-features/ai/models
- https://www.raycast.com/blog/the-new-raycast (2026-05-14) , /blog/changing-how-raycast-ai-is-priced (2026-09-10)
- https://manual.raycast.com/ai (index), /ai/quick-ai , /ai/ai-chat , /ai/ai-commands , /ai/ai-extensions , /ai/agents , /ai/skills , /ai/projects , /ai/automations , /ai/screen-awareness , /ai/model-context-protocol , /ai/bring-your-own-key , /ai/bring-your-own-subscription , /ai/custom-providers , /ai/ai-credits , /ai/raycast-ai-privacy-security.md
- https://developers.raycast.com/ai/create-an-ai-extension , /ai/learn-core-concepts-of-ai-extensions , /ai/getting-started , /api-reference/tool

**Repo files (Moe)**
- `crates/moe-extensions/src/ai_client.rs`, `crates/moe-extensions/src/ai.rs`,
  `docs/adr/0005-openai-compatible-endpoints-only-local-history.md`,
  `docs/adr/0024-ai-commands-extension.md`, `docs/adr/0036-quick-ask-is-a-panel-conversation-page.md`,
  `docs/research/ai-provider-libraries-rust.md`

## Verification notes

- **Fast-moving targets.** All version/status claims (ACP v1/v2, Rust SDK 3.2.0, pi v1.1.0 +
  Earendil move, Codex 0.162.0 + `wire_api="chat"` removal, dsh v0.2.1-alpha.2, Raycast v2.7,
  DeepSeek V4.x catalog/pricing) were fetched live on 2026-10-09/10 and will drift; dsh and
  Codex app-server are self-declared preview/experimental.
- **ACP.** Zellij as a client: not on the official list, unconfirmed. `claude-code-acp` →
  `claude-agent-acp` being a rename (vs successor) is inferred. Exact `session/set_mode` wire
  name inferred. Whether any external agent ships preinstalled in fresh Zed: unverified.
  Kotlin/Java/Python SDK maturity: existence only. Remote transports: draft/RFD only. Copilot
  CLI ACP preview details: via the ACP agents list, not GitHub's page.
- **pi.** Terminal-Bench 2.0 placement unverified (charts are images). Several blog posts known
  only via the flagship post's summaries. `pi-web-ui` dropped from the monorepo: inferred from
  absence + stale npm version. "Herdr" (v1.1.0 notes): unverified. Whether pi's own docs endorse
  `pi-acp`: not found.
- **Codex.** That `codex proto` once existed: training data (verified absent today). Release date
  2026-10-08: inferred from "08 Oct" on the latest release. DeepSeek lacking a *native*
  `/v1/responses` is stale — DeepSeek docs now document a Responses API; the Codex↔DeepSeek path
  is officially documented. No exhaustive "acp" string search inside openai/codex source (GitHub
  code search needs auth); absence verified against docs, CLI command enum, and crate list.
  codex-acp's TS rewrite/governance: inferred from file listings. `shell_tool` feature flag:
  unverified stage/effect.
- **DeepSeek.** The "DeepSeek Harness Guide" link target on api-docs quick-start: SPA, not
  extractable; the harness docs site returned no text when fetched. dsh first-public-release date:
  earliest release *with notes* is 2026-08-17. OpenRouter's displayed V4.1-Flash price
  ($0.016/M in) is inconsistent with DeepSeek's direct price — re-check before quoting.
  deepseek.com/harness desktop download links not directly verified (desktop app verified via
  release notes + architecture doc instead). npmjs.com pages were Cloudflare-blocked; registry
  JSON used instead.
- **Raycast.** Exact v1-era ship dates (AI Extensions, MCP, Ray-1): v1 manual undated,
  releases.raycast.com 404. "One interface, many LLMs" (2024-04-10): title/date via blog index,
  post body 404'd. **ACP absence**: verified across the full manual index (llms.txt), developer
  docs tree, v2 changelog, raycast.com/ai, and blog index — cannot prove absence on unfetched
  surfaces (X/Slack/community). Per-model credit rates: not documented. The "bottom line"
  reading of Raycast's strategy in Fit-for-Moe is inference from the cited facts.
- **Bench/behavior claims.** Nothing was executed or benchmarked for this note; all statements
  are documentation/source reads. Effort characterizations in the options table are assessments,
  not measurements.
