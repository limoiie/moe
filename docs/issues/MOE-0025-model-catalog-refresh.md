# M8g: the model catalog stays fresh — scheduled snapshot PR + runtime ETag refresh

- **ID**: MOE-0025
- **State**: done
- **Labels**: feature, ready-for-agent
- **Created**: 2026-10-10
- **Updated**: 2026-10-10
- **Commit**: 5b42e34

The provider/model picker must show current models with limits, capabilities and prices without
the app shipping a new build for every model release, and without hand-curating provider lists.
Research (`docs/research/ai-model-catalogs.md`) chose **models.dev** (MIT) as the primary catalog:
one JSON with per-provider base URL/env/docs plus per-model limits, modalities, tool/reasoning
flags, cost and dates; `api.json` revalidates via ETag (verified: conditional request → `304`,
0 bytes).

## Design

Two layers keep the catalog fresh; both are free:

1. **Snapshot pipeline (delivered in this change).** `scripts/refresh-models-catalog.sh` fetches
   `https://models.dev/api.json`, keeps the official providers and the picker's fields, and writes
   `crates/moe-platform/data/models-catalog.json` (sorted, pretty-printed for stable diffs; seeded
   at 17 providers / 730 models). It fails loudly if a provider id disappears upstream or the model
   count collapses. `.github/workflows/refresh-models-catalog.yml` runs it daily (and on
   `workflow_dispatch`) and opens/updates a PR touching only the snapshot. `MOE_MODELS_SRC` points
   the script at a local file for offline regeneration.
2. **Runtime refresh (this issue's remaining work).** A `moe-platform` module
   (`src/model_catalog.rs`) loads the embedded snapshot, overlays the cached copy at
   `{data_dir}/moe/models-catalog.json`, and — when the cache is older than 24 h — issues a
   conditional `GET` with the stored ETag on app start. `304` refreshes the timestamp; `200`
   replaces the cache; errors keep the previous state. Precedence: cache → embedded (provider-native
   discovery comes with the provider-layer work).

Attribution: models.dev is MIT (Copyright (c) 2025 models.dev) — recorded next to the data
(`crates/moe-platform/data/README.md`); surface it in the About/licences work later.

## Acceptance

- Script: rerunning against the same upstream is byte-identical; provider loss or schema shrink
  fails the run (no silent empty catalog).
- Workflow: `workflow_dispatch` opens a PR containing only the snapshot; schedule prerequisites are
  documented (repo must allow GitHub Actions to create PRs; GitHub auto-disables idle schedules
  after ~60 days).
- Runtime: offline first run shows the embedded snapshot; a stale cache triggers one conditional
  request (`304` ≈ free); a corrupt cache falls back to embedded; unit tests cover freshness,
  parse, and precedence.
- No UI change in this issue.

## References

- `docs/research/ai-model-catalogs.md` — catalog choice, schema, sizes, licences.
- `docs/research/ai-provider-libraries-rust.md` — the client-layer decision this feeds into.
- `docs/research/zed-ai-provider-layer.md` — capability-schema reference (Zed settings checklist).
- The provider-layer ADR that supersedes ADR-0005 is separate work; this issue only feeds it data.

## Comments

- 2026-10-10 (agent): filed. Snapshot pipeline delivered in the working tree (uncommitted): script,
  workflow, seeded snapshot (17 providers / 730 models from the 2026-10-09 upstream data), and the
  attribution README. Runtime refresh specified above, not yet implemented.
- 2026-10-10 (agent): runtime refresh delivered in the working tree (uncommitted):
  `moe-platform/src/model_catalog.rs` (embedded snapshot → cache overlay → 24 h ETag-conditional GET;
  atomic cache replace; 304 bumps only the stamp; HTTP/parse errors keep the previous state),
  startup hook in `moe-app/src/main.rs`, and the `reqwest` dependency. Validation:
  `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo test --workspace` green
  (58 platform tests, including 6 new model-catalog tests: parse/round-trip, staleness boundaries,
  overlay precedence, corrupt-cache fallback, a mock-server 200 → cache write → fresh-cache no-request
  → 304 stamp-bump sequence, and error retention). Pending commit — no GUI run made (would trigger a
  real refresh request).
- 2026-10-10 (agent): delivered in `8bb4441` (pipeline, runtime refresh, tests — one commit).
  GitHub-side activation remains user action: enable "Allow GitHub Actions to create and approve
  pull requests", then run the workflow once via `workflow_dispatch`; the daily cron keeps the
  snapshot fresh afterwards (auto-disables after ~60 days idle — documented in the workflow).
- 2026-10-10 (agent): renumbered to **MOE-0025 / M8g** after rebasing this branch onto main (main
  had concurrently allocated its own MOE-0016–0024 to the UI workstream); the sweep covered the
  ADRs, the research notes, code comments, and the config template. The rebase also rewrote the
  delivery commit: `8bb4441` → `5b42e34` (the sha in the comment above predates the rebase).
