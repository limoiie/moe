# M9d: providers can be removed — and activating an unkeyed one is gated

- **ID**: MOE-0032
- **State**: done
- **Labels**: ready-for-agent, bug, ux
- **Created**: 2026-10-11
- **Updated**: 2026-10-11
- **Assignee**: agent
- **Commit**: 8b06425

The owner asked: "why is alibaba activated by default, even though there is no key for it? how can
I remove it or other ai provider having been added?"

Two product gaps sat behind the question:

1. **"Activated" is just the config pair.** `[ai] provider` + `[ai] model` select the active
   endpoint (ADR-0040); nothing checks a key until a request runs. Alibaba became active because
   *something applied it* — any click on a model row in the settings window (every row was a
   pair switch) or Enter on the picker's first row. The pair then shows up as an "Active" group
   with a `key missing` chip — by design, but nothing said so, and nothing prevented the click.
2. **There was no way to remove a provider.** Disconnecting meant hand-editing config.toml and
   deleting `keys/<id>`: no UI path existed for the key file, the block, or the dangling pair.

## Decisions

- **Activation requires a key in the settings window**: clicking a model row of a provider whose
  key does not resolve no longer writes the pair — the row opens the provider's key entry instead
  and toasts "save a key first". (The palette picker keeps MOE-0028's flow: it applies and the
  confirmation card explains the key state — that surface is explicitly the catalog browser.)
- **Remove is one group action, two-step confirmed**: a trash button in the provider header swaps
  the header's controls to "Remove {name}? [Remove] [Cancel]". Remove deletes the provider's key
  file, removes its whole `[ai.providers.<id>]` block, and — when the active pair pointed at it —
  clears `[ai] provider` / `model` (the emptied `[ai]` section loses its header with them; the AI
  surfaces fall back to the setup state or the legacy `base_url`). The synthetic legacy `default`
  group without a block hides the button — there is nothing to remove but the key.
- **One comment-preserving surgery**: `remove_provider_in_toml` (section removal with the
  separator blank line, plus optional selection clearing through the same editor the settings
  window's writes use); a missing provider is a no-op, so Remove is idempotent.
- The settings DTO gains `hasBlock` (the UI needs to know whether config surgery is involved),
  and the pure function is factored as `remove_provider_with(config, remove_key, store)` for
  tests, following the module's injected-store pattern.

## Delivery

- `crates/moe-platform/src/config.rs`: `remove_section`, `remove_provider_in_toml`,
  `store_provider_removal` + tests (block removal keeps comments/other sections byte for byte;
  selection clearing only on request; idempotent).
- `crates/moe-platform/src/keychain.rs`: `remove_provider_key` (missing file is success).
- `crates/moe-extensions/src/ai_models.rs`: `remove_provider` / `remove_provider_with`,
  `ProviderSettings.has_block` + tests.
- `crates/moe-app/src/main.rs`: the `ai_remove_provider` IPC.
- `ui/src/settings.ts`: the header trash + inline confirmation, the activation gate, and the
  `hasBlock` field.

## Acceptance

- Clicking an unkeyed provider's model row opens its key row and says so; the pair is not written.
- The trash button removes MiniMax CN (or any added provider): its key file, block and, when
  active, the pair — verified by config round-trip tests; the pane reloads without the group and
  the toasts name the provider.
- `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test --workspace` green
  (app 6, core 52, extensions 117, platform 69); `pnpm -C ui typecheck` + `build` green.

## Comments

- 2026-10-11 (agent): filed and delivered in one pass answering the owner's question.