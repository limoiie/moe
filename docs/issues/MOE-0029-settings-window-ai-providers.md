# M9a: the settings window — AI provider management reaches the UI

- **ID**: MOE-0029
- **State**: done
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-11
- **Updated**: 2026-10-11
- **Assignee**: agent

The owner's ask: configure AI providers and switch models amongst several providers from the Moe
UI itself. Until now every provider fact lived in config.toml and key files: MOE-0028 brought a
catalog browser + pair selector into the palette, but keys, visibility and provider blocks stayed
file-managed, and ADR-0009 excluded a settings UI from v1. This issue lifts that exclusion one
step further: a settings **window** whose AI section manages the providers this install uses —
the Raycast settings shape (sidebar sections, one pane each) in Moe's glass language.

## Decisions

- **Surface**: an ordinary window (`settings` label, `settings.html`), not a panel page and not a
  floating surface — it stays open until closed (Esc / traffic light), never floats over other
  apps, and joins no Spaces semantics. Sidebar: General (appearance rows, the config-file row)
  and AI ("Models & Providers"); the window opens on the AI section because that is why it
  exists. Entry points: the tray's "Settings…", the palette command "Open Settings"
  (`moe.open-settings`, `ActionResult::OpenSettings {}`), the Side View header's gear, and an
  "Open Settings" row in every page's actions card.
- **The provider groups**: the providers this install actually uses — configured
  `[ai.providers.*]` blocks, catalog providers whose key resolves, the active provider, and the
  legacy implicit `default` endpoint (`base_url` without a provider, one configured model). Not
  the whole catalog: 40 providers / 730 models is a catalog browser's job (MOE-0028), the
  settings window manages what is connected.
- **Per group**: the key state (saved / missing, its file path, the catalog env names) with an
  inline password row writing the provider's own 0600 key file (`keychain::set_provider_key` —
  the `key <value>` command's path, generalized); the model rows with the catalog's headline
  facts as chips and the context window as a column; a checkbox per model for the picker
  visibility; clicking a model row makes it the active pair (the MOE-0028 apply rule).
- **Picker visibility is a new config key**: `hidden_models = […]` inside `[ai.providers.<id>]`,
  subtracted from the effective list after any hand-written `models` restriction. The checkbox
  therefore never rewrites the catalog-sized restriction list (which would freeze daily catalog
  refreshes out of the picker), and an all-visible provider carries no line at all — the
  comment-preserving writer removes an emptied provider block again.
- **"Connect a provider"**: a search over the catalog providers the window does not manage yet;
  saving a key connects one (official providers need no config block, ADR-0040). Custom
  providers stay config-defined — the pane says so and links the config file.
- **Non-goals**: editing catalog properties, OAuth, local model discovery, per-conversation
  model overrides, provider block field editing beyond keys/visibility (the file remains the
  escape hatch, and the window links it).

## Delivery

- `crates/moe-platform/src/config.rs`: `ProviderConfig.hidden_models` (+ parse), the
  removal-capable section editor, `set_provider_hidden_models_in_toml` /
  `store_provider_hidden_models` + tests (empty list removes the line and an emptied block).
- `crates/moe-platform/src/keychain.rs`: `set_provider_key` (the active-provider save folds into it).
- `crates/moe-extensions/src/ai_models.rs`: `ActivePair` / `active_pair_with`, `ModelOption` /
  `model_options_with` (the MOE-0030 switcher rows), `ProviderSettings` / `settings_providers_with`,
  `CatalogProvider` / `catalog_providers_with`, `set_model_visibility_with`; `all_rows` skips
  hidden models; the picker's ranking factors into `rank_rows` + tests.
- `crates/moe-core/src/contract.rs`: `ActionResult::OpenSettings {}`.
- `crates/moe-extensions/src/moe.rs`: the `moe.open-settings` command.
- `crates/moe-app/src/main.rs`: the `settings` window, `open_settings`, the tray item, and the
  `ai_*` IPC family (`ai_active_pair`, `ai_model_options`, `ai_set_pair`, `ai_settings_providers`,
  `ai_catalog_providers`, `ai_set_model_visibility`, `ai_save_provider_key`,
  `open_settings_window`).
- `ui/settings.html` + `ui/src/settings.ts`: the window (sidebar + panes); `ui/vite.config.ts`
  gains the third entry point.
- Docs at delivery: ADR-0009 amendment (the settings window exception), ADR-0040 amendment
  (`hidden_models`), CONTEXT (**Settings Window** term), README (the settings bullet).

## Acceptance

- The tray / palette / side-view gear / actions card all open the settings window; Esc hides it.
- The AI pane lists exactly the providers in use, active first; each group shows protocol, key
  state and its models with facts, context and visibility checkboxes.
- Saving a key through a group's row (or the connect search) writes `keys/<id>` (0600) and the
  group's key state flips without touching any other config line.
- Unchecking a model writes `hidden_models` for that provider and the model leaves both the
  palette picker and the chat views' switcher; checking it again removes the id, and a provider
  whose list becomes empty loses the line (and a block that only carried it).
- Clicking a model row writes `[ai] provider` / `[ai] model` in place (comments preserved) and
  the chip in both chat views follows on their next refresh.
- `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo test --workspace` green
  (app 6, core 52, extensions 112, platform 68), `pnpm -C ui build` green.

## Comments

- 2026-10-11 (agent): filed and delivered with MOE-0030 in one pass — the owner's ask bundled
  provider configuration and model switching, and the switcher card (MOE-0030) consumes this
  issue's `model_options` rows. Remaining manual: a visual pass of the settings window in the
  running app (group density with a large official provider, the connect search against the
  full catalog) and a live key save; the config surgery is unit-tested but the IPC path has not
  been exercised end to end.
- 2026-10-11 (agent): revision note — the switcher's header chip from the first delivery is gone
  (MOE-0030 revision): "Set AI Model" is now a row inside both chat surfaces' actions cards, and
  the card it opens is unchanged. The settings window itself is untouched by the revision.
- 2026-10-11 (agent): revision note — the window's key entries (provider groups and the connect
  search's per-provider row) and the connect search itself now wear the composer row's capsule
  language (MOE-0030 revision): one-row glass capsule inputs with the inline error inside the
  capsule, saved through a round glass check button in the about-chip style. The bordered field
  style is gone from the window.
- 2026-10-11 (agent): follow-up — provider groups gained a Remove action (key file + config block
  + the active pair) and model-row activation is gated on a resolvable key (MOE-0032). The DTO
  carries `hasBlock` for the UI's confirmation copy; nothing else in the window changed.
- 2026-10-11 (agent): chrome revision — the settings window is now a **standard macOS window**
  (`decorations: true`, `transparent: false`, minimizable): the native title bar carries the
  close / minimize / maximize buttons the owner saw and questioned; the custom traffic-light dot
  and the fake rounded window card are gone (the page is a full-bleed two-pane layout). A custom
  close handler hides the singleton instead of destroying it, so reopening is instant and the
  section persists.
