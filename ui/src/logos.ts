// Provider brand marks (MOE-0030 revision 8): the Lobe Icons set (MIT — lobehub/icons), vendored
// as SVGs so the app stays offline and needs no React dependency. Monochrome marks carry
// `fill="currentColor"`, so injecting them inline lets the surrounding tile tint them per theme;
// color marks keep their brand fills. Providers without a vendored mark get a monogram tile.

// Raw SVG sources, keyed by file path (Vite inlines them as strings at build time).
const FILES = import.meta.glob("./logos/*.svg", {
  eager: true,
  query: "?raw",
  import: "default",
}) as Record<string, string>;

/** Catalog provider id → vendored mark (see `logos/README.md` for the source list). */
const PROVIDER_MARKS: Record<string, string> = {
  alibaba: "alibaba",
  anthropic: "anthropic",
  cohere: "cohere",
  deepseek: "deepseek",
  google: "google",
  groq: "groq",
  lmstudio: "lmstudio",
  meta: "meta",
  minimax: "minimax",
  "minimax-cn": "minimax",
  mistral: "mistral",
  moonshotai: "moonshotai",
  openai: "openai",
  openrouter: "openrouter",
  perplexity: "perplexity",
  togetherai: "togetherai",
  xai: "xai",
  zai: "zai",
  zhipuai: "zhipuai",
};

/** The raw SVG markup for a catalog provider id, or null when no mark is vendored. */
function markSource(providerId: string): string | null {
  const name = PROVIDER_MARKS[providerId];
  return name ? (FILES[`./logos/${name}.svg`] ?? null) : null;
}

/**
 * The provider's brand mark in a rounded tile: [mark 18px] … The mark is injected inline (bundled
 * assets, never user input) so mono marks inherit the tile's color; a provider without a vendored
 * mark falls back to its initial in the same tile, so every row keeps the same rhythm.
 */
export function providerMark(providerId: string, providerName: string, size = 18): HTMLElement {
  const tile = document.createElement("span");
  tile.className =
    "inline-flex shrink-0 items-center justify-center rounded-[6px] border border-line bg-surface-overlay text-fg";
  tile.style.width = `${size}px`;
  tile.style.height = `${size}px`;
  const source = markSource(providerId);
  if (source) {
    const holder = document.createElement("span");
    holder.className = "inline-flex items-center justify-center";
    holder.style.width = `${Math.round(size * 0.66)}px`;
    holder.style.height = `${Math.round(size * 0.66)}px`;
    // SAFETY: a bundled SVG asset (see logos/README.md), never user input.
    holder.innerHTML = source;
    const svg = holder.querySelector("svg");
    if (svg) {
      svg.setAttribute("width", "100%");
      svg.setAttribute("height", "100%");
    }
    tile.append(holder);
    tile.title = providerName;
    return tile;
  }
  tile.style.fontSize = `${Math.round(size * 0.5)}px`;
  tile.className += " font-medium text-fg-muted";
  tile.textContent = (providerName.trim()[0] ?? "?").toUpperCase();
  tile.title = providerName;
  return tile;
}
