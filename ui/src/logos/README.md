# Provider brand marks

The provider logos the model surfaces show (model list rows, the model detail header, the settings
provider groups), vendored so the app works offline and stays free of a React dependency.

- **Source**: [lobehub/lobe-icons](https://github.com/lobehub/lobe-icons) — the
  `@lobehub/icons-static-svg` package, **MIT** licensed.
- **Version**: `1.95.1` (via `https://cdn.jsdelivr.net/npm/@lobehub/icons-static-svg@1.95.1/icons/`).
- **Files**: one SVG per catalog provider id, named after the catalog id (`minimax-cn` reuses
  `minimax.svg`). The `*-color` variant is used where upstream ships one (brand fills); the
  remaining marks are the monochrome variant, whose `fill="currentColor"` lets `logos.ts` tint
  them with the theme.
- **Refresh**: re-download the same file names from a newer package version and update the
  version above; the mapping table lives in `ui/src/logos.ts` (`PROVIDER_MARKS`).
- **Fallback**: a provider without a vendored mark renders a monogram tile with its initial, so
  new catalog providers (the runtime refresh can add any models.dev provider) still look right.
