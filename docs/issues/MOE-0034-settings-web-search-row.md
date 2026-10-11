# M9f: the settings window grows a Web Search row — SearXNG reaches the UI

- **ID**: MOE-0034
- **State**: in-progress
- **Labels**: feature, ux
- **Created**: 2026-10-11
- **Updated**: 2026-10-11
- **Assignee**: agent

The owner's ask: "since we have settings window now, please add configuration for searxng entry
there." The SearXNG instance URL lived only in config.toml (`[ai] searxng_url`, MOE-0026) while
the settings window (MOE-0029) manages everything else AI. This adds the agent's search backend
to the UI — with a Test button that live-probes the instance, because the classic 403 footgun
(stock SearXNG serves HTML only) deserves an immediate, named diagnosis instead of a silent
model-side "search instance unreachable".

## Decisions

- **Place**: a **Web Search** card at the bottom of the **AI pane** (below Connect-a-provider) —
  it is an agent-tool setting, not General; no new sidebar section for one row.
- **The row**: a capsule URL input (prefilled from config, placeholder `http://localhost:8080`)
  plus round **Test** (search icon) and **Save** (check) buttons; Enter saves; the header badge
  reads configured/off. Saving an **empty** value removes the `searxng_url` line again — unset
  means `web_search` is never advertised at all (the `tools.rs specs` rule). Validation inline in
  the capsule (composer-row language, MOE-0030 revision): http(s) URLs only.
- **Test** = one real search round-trip (`GET {url}/search?q=moe&format=json`): success toasts
  "N results for «moe» — JSON search works"; a non-2xx carries the tool's own fix hint (enable
  the JSON format in `settings.yml`); unreachable/invalid-JSON report inline. Runs off the main
  thread (`#[tauri::command(async)]`; the tools' 20 s budget).
- **Persistence**: `store_searxng_url` through the comment-preserving section editor
  (`edit_section_opt` on `[ai]`, which creates the section when absent) — the file stays the
  user's, never re-serialized (the theme / hidden-models writer posture).
- **Non-goals**: UI for the `tools` kill-switch or `fetch_fallback` (config-file only — the ask
  was the SearXNG entry), instance lifecycle management (Moe never runs docker), multiple
  instances.

## Delivery

- `crates/moe-platform/src/config.rs`: `set_searxng_url_in_toml` / `store_searxng_url` + test
  (replace / add / blank-removes / section-creation, comments byte-preserved).
- `crates/moe-extensions/src/tools.rs`: `probe_searxng` + test (mock-instance summary, the 403
  hint, non-http rejection before any network).
- `crates/moe-app/src/main.rs`: IPC `ai_searxng_url` / `ai_set_searxng_url` / `ai_test_searxng`,
  registered in the handler list.
- `ui/src/settings.ts`: `webSearchCard()` appended in `renderAi`; no new icons (globe / search /
  check already registered).
- `README.md`: the settings-window bullet names the row.

## Acceptance

- Settings → AI shows the Web Search card; the badge reflects the config state; the input is
  prefilled with the saved URL.
- Saving a URL adds/replaces `searxng_url` inside `[ai]` in config.toml with every comment
  byte-preserved; the badge flips to configured; new conversations on a chat-protocol provider
  now advertise `web_search`.
- Test against a JSON-enabled instance → success toast with the result count; against a stock
  (HTML-only) instance → inline 403 + the `formats` hint; against a dead port → "unreachable".
- Saving an empty value removes the line; badge flips to off; the tool is no longer advertised.
- A non-http value ("localhost:8080") shows "http(s) URL" inline and writes nothing.
- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and
  `pnpm -C ui build` green.

## Comments

- 2026-10-11 (agent): implemented in the working tree (all five files above); validation runs
  recorded below before handing back for commit.
- 2026-10-11 (agent): validation green — `cargo test --workspace` (app 6, core 53, extensions 119
  incl. `probe_searxng_summarizes_and_hints`, platform 70 incl.
  `set_searxng_url_edits_the_ai_section`), `cargo clippy --workspace --all-targets -- -D warnings`
  clean, `pnpm -C ui build` (tsc + vite) green. Awaiting commit; GUI check pending: open Settings
  → AI, save a URL, watch the badge flip and a new chat advertise `web_search`; Test a stock
  instance to see the 403 `formats` hint inline.
- 2026-10-11 (agent): the owner's SearXNG instance now runs from `~/Projects/dockers/searxng`
  (compose + bind-mounted `config/settings.yml` with `search.formats: [html, json]`), host port
  **8888** (8080 is taken by OrbStack); the container proxies upstream engines through the Mac's
  system proxy (`host.docker.internal:33505`, env in the compose — update if the proxy app's port
  changes). Verified live: `GET :8888/search?q=…&format=json` returns real results (20 hits; DDG
  CAPTCHA / Brave 429 on the proxy exit IP are tolerated — other engines cover). `searxng_url =
  "http://localhost:8888"` inserted under `[ai]` in the app config. **Caveat found while
  wiring**: the active pair (minimax-cn / MiniMax-M3) resolves to the Anthropic protocol, and the
  tool gate (`ai.rs`) offers tools on `OpenAiChat` only — web search is live for chat-protocol
  providers (e.g. deepseek) and dormant until tools ride the anthropic/responses adapters too
  (candidate follow-up issue).
- 2026-10-11 (agent): the caveat above is resolved — MOE-0035 generalized the tool loop to the
  Anthropic protocol, so the active minimax-cn pair now advertises `web_search` too. The
  settings-card blurb dropped the "chat-protocol only" wording.
