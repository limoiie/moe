# Embedded model catalog

`models-catalog.json` is a reduced snapshot of [models.dev](https://models.dev/api.json)
(MIT License, Copyright (c) 2025 models.dev): the official providers Moe ships with, and the
model fields the provider/model picker needs (limits, modalities, tool/reasoning flags, cost,
dates, canonical id).

**Do not edit by hand.** Regenerate with:

```sh
./scripts/refresh-models-catalog.sh
```

or run the **Refresh model catalog** workflow, which regenerates it daily and opens a PR. Set
`MOE_MODELS_SRC` to a local `api.json` path to regenerate offline. Design and acceptance live in
`docs/issues/MOE-0025-model-catalog-refresh.md`.
