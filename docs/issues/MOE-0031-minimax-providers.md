# M9c: MiniMax (both regions) reaches the provider list

- **ID**: MOE-0031
- **State**: done
- **Labels**: ready-for-agent, bug, ai
- **Created**: 2026-10-11
- **Updated**: 2026-10-11
- **Assignee**: agent
- **Commit**: 8b06425

The owner configured MiniMax and asked why no MiniMax CN provider exists in the UI. Three gaps
stacked up:

1. **The embedded snapshot's allowlist never carried MiniMax.** The offline baseline
   (`scripts/refresh-models-catalog.sh`'s `PROVIDERS`) is 17 curated providers; `minimax` and
   `minimax-cn` were not among them. The runtime's daily refresh overlays the *full* models.dev
   catalog on top of the baseline, so a machine whose refresh has run (or the online path)
   eventually sees them — but the baseline, the picker and the settings search did not.
2. **The checked-in protocol table had no MiniMax entries** (ADR-0040). Even when the catalog
   carried them, resolving `provider = "minimax-cn"` failed with "does not say which protocol it
   speaks" — the catalog facts alone are not a request.
3. **The URL contract mismatch**: models.dev's MiniMax entries point at the vendor's
   **Anthropic-compatible surface** (`https://api.minimax.io/anthropic/v1`, npm hint
   `@ai-sdk/anthropic`), a base that already ends in its version root. The Anthropic adapter
   appended `/v1/messages` blindly, which would have produced `…/anthropic/v1/v1/messages`.

## Decisions

- **Both regions, Anthropic protocol.** `minimax` (minimax.io) and `minimax-cn` (minimax.cn) join
  the baseline allowlist and the protocol table with `Protocol::Anthropic`; the catalogued base
  URL is used as-is (table entries carry the same value as the fallback). The token-plan variants
  (`minimax-coding-plan`, `minimax-cn-coding-plan`) stay out — same endpoints, subscription
  packaging; a custom provider block covers them.
- **The Anthropic endpoint join respects an existing version root**: `/v1/messages` for a bare
  host (Anthropic itself), `/messages` when the base already ends in `/v1` (MiniMax). The
  alternative — overriding the catalogued URL — would have contradicted ADR-0040's precedence
  (catalog first, table as fallback) for a cosmetic path detail.
- **Verification** (network pass): models.dev carries `minimax`, `minimax-cn` (7 models each,
  M2 → M3, tools + reasoning); both hosts (`api.minimax.cn`, `api.minimaxi.com` — the CN docs'
  own host) answer `401` on `POST …/v1/messages` without a key; docs pages cited in the table
  answer `200`.

## Delivery

- `scripts/refresh-models-catalog.sh`: `minimax minimax-cn` in `PROVIDERS` (future runs keep them
  fresh).
- `crates/moe-platform/data/models-catalog.json`: the two providers merged in with the script's
  own reduction (additions only — every other provider byte-untouched).
- `crates/moe-extensions/src/ai_client/providers.rs`: the two table entries + citations, and
  `minimax_providers_resolve_over_their_anthropic_surface` (resolution + endpoint join).
- `crates/moe-extensions/src/ai_client/anthropic.rs`: the version-root-aware join +
  `endpoint_url_respects_an_existing_version_root`.
- `crates/moe-platform/src/config.rs`: the template's provider examples mention
  `minimax, minimax-cn` so the pair is discoverable in the config file too.

## Acceptance

- The settings window's "Connect a provider" search lists **MiniMax (minimax.io)** and
  **MiniMax (minimax.cn)**; saving a key connects either and its models appear in the picker and
  (as a keyed provider) in the chat switcher.
- `provider = "minimax-cn"` + a MiniMax model resolves to the Anthropic protocol at
  `https://api.minimax.cn/anthropic/v1/messages`; same for the global host.
- `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test --workspace` green
  (app 6, core 52, extensions 116, platform 68).

## Comments

- 2026-10-11 (agent): filed and delivered in one pass after the owner's "why no minimax cn ai
  provider?" question. Remaining manual: a live key + one streamed answer against
  `minimax-cn` (the resolution and URL join are unit-tested; the SSE parse path is the shared
  Anthropic adapter already exercised by its fixtures).