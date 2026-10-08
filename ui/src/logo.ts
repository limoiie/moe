// The Moe brand mark, single source: ui/src/moe.svg (radiant diamond — a rotated-square core with
// twelve orbiting wedges, white background removed, currentColor, cropped viewBox). It lives in src/
// because Vite cannot ?raw-import from public/. Imported raw and inlined so it inherits the theme
// color of whichever scene renders it (input row: text-fg; chip avatar: fg-muted) — a bitmap or
// <img> could not do that. The tray raster (gen-tray-icon.py) parses this file directly, so the
// geometry cannot drift.
import LOGO_SVG from "./moe.svg?raw";

/** The brand mark as an inline SVG element; `className` carries the size + color tokens. */
export function logoEl(className = ""): SVGElement {
  const template = document.createElement("template");
  template.innerHTML = LOGO_SVG.trim();
  const svg = template.content.firstElementChild as SVGElement;
  svg.setAttribute("aria-hidden", "true");
  for (const cls of className.split(/\s+/).filter(Boolean)) svg.classList.add(cls);
  return svg;
}