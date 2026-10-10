# Free web search and extraction options for the agent's Tools

Research note, 2026-10-10. Every claim below cites a primary source (official docs/pricing pages,
official repos, registry APIs) fetched live on 2026-10-10; URLs are in [Sources](#sources).
Claims not verified against a live primary source are marked in
[Verification notes](#verification-notes).

## Context

MOE-0017 gives Agentic Conversations two web Tools: `web_search(query)` and `web_fetch(url)`
(HTML → readable markdown). The owner's hard constraint: **$0 ongoing cost** — no paid API, no
per-query spend, no card-gated free tiers; keyless or free-key only. The backend is a Rust/Tauri
desktop app making plain HTTPS calls for a single user. This note is the comparison behind the
decision (ADR-0039): **self-hosted SearXNG + local extraction with a keyless reader fallback**.

## Findings

### Dead end first: DeepSeek has no server-side web search

- The Responses API compatibility table lists `web_search` / `file_search` / `code_interpreter` /
  `computer_use` / `mcp` / other built-in tools as **"Ignored"** (silently)
  (https://api-docs.deepseek.com/guides/responses_api).
- On the Anthropic-compatible wire (`/anthropic`), `server_tool_use` and `web_search_tool_result`
  content blocks are "Supported" — but these are **content-block acceptance** rows: DeepSeek
  accepts *client-generated* search results in the history (which is what makes Claude Code's
  client-side web search work against DeepSeek). The `tools` table documents only
  `name`/`input_schema`/`description`/`cache_control` — no Anthropic-style `type: "web_search_*"`
  server tool is offered (https://api-docs.deepseek.com/guides/anthropic_api).
- The pricing page has no per-search charge of any kind — token billing only; the changelog
  through 2026-09-10 announces no search product
  (https://api-docs.deepseek.com/quick_start/pricing, https://api-docs.deepseek.com/updates).

Conclusion: search must be client-side regardless of endpoint; there is no free DeepSeek-hosted
search to adopt.

### Comparison table (verified 2026-10-10)

| Option | Genuinely $0? | Key | Card | Documented limits | Risk / notes | Search or extract |
| --- | --- | --- | --- | --- | --- | --- |
| **SearXNG self-hosted** | ✅ (+ your hardware) | none | no | JSON format **off by default** (`formats: [html]`; unset format → 403); docker image ≈143–265 MB | AGPL-3.0 (non-issue for local personal use); you scrape upstream engines yourself; limiter/bot-detection docs exist | Search |
| SearXNG public instances | ✅ | none | no | 71 online (+13 onion) on searx.space; JSON disabled on most | Official caveat: higher traffic → "higher chance of being blocked"; operators' rules; best-effort only | Search |
| **Tavily free** | ✅ | free key | **no** | 1,000 credits/mo; search basic = 1 credit; extract basic = 5 URLs/credit; map = 10 pages/credit; overflow $0.008/credit | none — official API | Both |
| Firecrawl free | ✅ | free key | no | 1,000 credits/mo; scrape 1 credit/page; search 2 credits/10 results; 10 req/min; 403/404 pages still cost 1 credit | none | Both |
| Exa free | ✅ | free key | no | $10 credits/mo + $10 onboarding, "no payment method required"; instant search $4/1k ⇒ ~1,400–2,500 searches/mo; 10 QPS | none | Both |
| Brave Search API | ~$5 free credits/mo | key | **yes** ("only used to confirm your identity and will not be charged") | $5/1k requests ⇒ ~1,000 free searches/mo; 50 QPS capacity | card-gated → violates the constraint in spirit | Search |
| **DuckDuckGo html/lite** | ✅ | none | no | no published limits; endpoints live today | **ToS-gray**: robots.txt `Disallow: /lite` + `Disallow: /html`; ToS permits suspension; no numeric rate limits published | Search |
| DDG Instant Answer API | ✅ | none | no | official keyless JSON endpoint | returns abstracts/related topics only — **not** a full web SERP | Search (weak) |
| **Jina Reader `r.jina.ai`** | ✅ | optional | not stated | keyless **20 RPM** per IP; free key: 500 RPM, 10M one-time tokens (token-metered); 5-min cache | honest docs: "does not actively circumvent" anti-bot; blocks respected | Extract |
| Jina Search `s.jina.ai` | free only until 10M tokens run out | yes (keyless blocked) | — | 100 RPM; **≥10,000 tokens per request** ⇒ ≈1,000 searches total, then paid | effectively paid-per-search | Search |
| Gemini `google_search` grounding | ✅ free tier | Gemini key | no | "500 RPD free (limit shared for Flash and Flash-Lite). Not available for Pro." — **2.5 generation only**; Gemini 3+: "Not available" on the API free tier; not exposed on the OpenAI-compat chat wire | answer synthesis with `url_citation`s, not a raw SERP tool; couples to Google | Search-as-grounding |
| Groq `browser_search` | unverified | key | no | server-side tool (powered by Exa) on gpt-oss models; Compound web-search systems decommissioned 2026-09-21; pricing page unfetchable (JS-only) | treat as probably token/tool-billed | Search (LLM-bound) |
| OpenRouter `:online` | ❌ | key | credits | "Using web search will incur extra costs, even with free models": Exa $0.007/req, Parallel $0.001–0.005, Perplexity $0.005 | **ruled out** | Search (paid) |
| Cloudflare AI Search (ex-AutoRAG) | ✅ free allowance | CF account | no | 1,000 semantic + 1,000 full-text queries/mo; billing starts 2026-11-01 | searches **your own indexed content** — not live web | Neither |
| Wikipedia/Wikimedia API | ✅ | none | no | 10 req/min unidentified; 200 req/min with compliant UA (contact info required "or you may be IP-blocked without notice") | encyclopedic scope only | Search + extract |
| Common Crawl | ✅ | none | no | "Crawl data is free to access by anyone from anywhere"; monthly crawls | historical snapshots, not freshness | Extract (archival) |
| Mojeek API | ❌ self-serve | key | via Stripe | £2 CPM startup; free trial "get in touch" | independent index, paid | Search |
| Marginalia | ✅ (non-commercial share-alike) | key | no | small/text-oriented independent index | key acquisition process unverified (old open-api page 404) | Search (niche) |

### Extraction: the $0 default needs no vendor

- **`htmd` 0.5.5** (2026-07-27, Apache-2.0, 6.12M downloads) — actively maintained
  turndown.js-inspired HTML→Markdown; the pick.
- **`readability` 0.3.0** (2023-12-20, MIT, 1.59M downloads) — arc90-readability port for article
  extraction; heavily used but no release in ~3 years: pin it, don't expect fixes.
- **`html2md` 0.2.17** (2026-08-20) — **GPL-3.0+**: licensing hazard, avoid.
- Anti-bot realities: JS-heavy SPAs return empty shells to a plain GET; edge protection returns
  403/429; even Jina's rendered fetcher "does not actively circumvent… any website defense
  mechanisms." **No free path reliably reads every page** — layer a chain and let the model see
  honest failures.

### DuckDuckGo crate ecosystem (why keyless scraping is fragile)

`duckduckgo` 0.3.3 (2026-05-20, MIT; uses lite + Instant Answer + i.js/news.js endpoints),
`ddgo` 0.5.0 (2026-09-04, "web search to clean markdown, no CAPTCHAs"), `ddg-rs` 0.2.0
(2026-06-05, ddgr port "with an interactive CAPTCHA solver"), `daedra` 0.5.7 (2026-09-02, MCP
server with "**14 unkeyed backends with automatic fallback. Works from any IP**") — the existence
of fallback-chain products is itself evidence that single-endpoint DDG scraping breaks often
(crates.io API).

## Fit for Moe (decision recorded in ADR-0039 / MOE-0017)

- **Chosen: self-hosted SearXNG** (owner's decision). Durable, private, $0, keyless; the JSON API
  is one settings line (`formats: [html, json]`); Moe consumes `GET {url}/search?q=…&format=json`
  and takes the top ~8 results (title/url/snippet). Moe never manages the service — the README
  documents the docker run. Tool advertisement is config-driven: no `searxng_url`, no
  `web_search` in the request. A 403 (JSON disabled — SearXNG's default) or connection failure
  becomes a tool error the model sees plus one stderr diagnostic.
- **Extraction chain**: `reqwest` GET (browser-like UA, redirect/timeout/byte caps) →
  `readability` → `htmd`; on 403/429/empty-shell, fall back to keyless `https://r.jina.ai/<url>`
  (20 RPM is ample for one user) — **default on**, `[ai] fetch_fallback = "none"` disables it
  (owner accepted the one-URL privacy surface; it only fires after a local failure).
- **Kept pluggable**: a `SearchProvider`-shaped seam leaves Tavily/Firecrawl/Exa free keys (all
  genuinely $0, no card) as drop-in alternates if running a container ever becomes annoying.
- **Rejected as default**: DDG scraping (ToS-gray + fragile), Brave (card-gated), OpenRouter
  `:online` (paid), Gemini grounding (synthesis not SERP; 2.5-gen-only free tier; foreign wire).

## Sources

- DeepSeek: https://api-docs.deepseek.com/guides/responses_api , /guides/anthropic_api , /guides/tool_calls , /quick_start/pricing , /updates
- SearXNG: https://github.com/searxng/searxng , https://docs.searxng.org/dev/search_api.html , /admin/settings/settings_search.html , /admin/installation-docker.html , https://searx.space
- Tavily: https://docs.tavily.com/documentation/api-credits
- Firecrawl: https://firecrawl.dev/pricing (effective 2026-09-04)
- Exa: https://exa.ai/pricing
- Brave: https://brave.com/search/api/
- DuckDuckGo: https://duckduckgo.com/robots.txt , https://duckduckgo.com/terms ; crates.io API for `duckduckgo`, `duckduckgo_search`, `ddg-rs`, `ddgo`, `daedra`, `websearch`
- Jina: https://jina.ai/reader/
- Gemini: https://ai.google.dev/gemini-api/docs/pricing , /docs/grounding , /docs/openai
- Groq: https://console.groq.com/docs/tool-use/built-in-tools , …/browser-search , deprecation page
- OpenRouter: https://openrouter.ai/docs/features/web-search
- Cloudflare: https://developers.cloudflare.com/ai-search/platform/limits-pricing/
- Wikipedia: https://mediawiki.org/wiki/API:Etiquette , https://mediawiki.org/wiki/Wikimedia_APIs/Rate_limits
- Common Crawl: https://commoncrawl.org/get-started
- Mojeek: https://mojeek.com/services/search/web-search-api/ ; Marginalia: https://marginalia-search.com , https://github.com/MarginaliaSearch/MarginaliaSearch
- Extraction crates: crates.io API for `htmd`, `readability`, `html2md`, `h2m`

## Verification notes

- **DDG policy text**: `duckduckgo.com/acceptable-use-policy`, `/aup`, `/robot` all redirect to the
  non-JS search (unfetchable); only the ToS reference to the AUP and the robots.txt `Disallow`
  lines are verified. No published numeric rate limits exist; bans are ad hoc (anomaly/CAPTCHA).
- **Groq `browser_search` pricing / free-tier availability**: docs defer to a JS-only pricing
  page; token prices only on the models page. Unconfirmed whether tool use is free on Groq's free
  tier. **Cerebras**: docs subpages JS-rendered; no built-in search found — provisional "not
  offered" only.
- **Jina free-key card requirement**: not stated on the page.
- **Marginalia key/license process**: old open-API page 404s; "non-commercial share-alike is
  always free" is from the project README.
- **Gemini free-tier RPM/RPD**: Google now shows limits only in AI Studio; the static published
  numbers are the tool limits (500 RPD grounding).
- **Brave overage behavior** when the $5 credits are exhausted (hard stop vs charge): not stated
  on the marketing page.
- **searx.space instance count (71)**: accurate at fetch time; fluctuates daily. Which instances
  allow JSON must be probed per instance.
- Endpoint liveness (DDG html/lite, api.duckduckgo.com, r.jina.ai) was verified by live fetch on
  2026-10-10; availability from any given network/region (esp. behind firewalls) is not
  guaranteed and SearXNG self-hosting is precisely the mitigation chosen.
