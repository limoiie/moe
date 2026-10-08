# Appearance: two themes (light / dark) behind one semantic token layer

The view layer was **dark-only**: Tailwind utilities and component CSS named zinc shades directly
(`bg-zinc-900/90`, `text-zinc-500`, `hover:ring-zinc-600/60`), so a light appearance was not a
setting away — it was a sweep through every class string. The palette also carried contrast debt:
section headers and badges rendered zinc-500 on zinc-900 (≈3.6:1) and the panel is used daily, in
every light condition there is.

## Decision

- **Two themes, one role vocabulary.** Light and dark palettes live as raw `--moe-*` variables in
  `styles.css` under `:root[data-theme="light"]` / `:root[data-theme="dark"]`; `@theme inline`
  maps them to Tailwind color utilities (`bg-surface`, `bg-surface-overlay`, `bg-surface-float`,
  `bg-surface-hover`, `bg-surface-selected`, `text-fg`, `text-fg-muted`, `text-fg-subtle`,
  `text-fg-faint`, `border-line`, `border-line-strong`, `bg-accent-fill`, `text-on-accent`,
  `bg-warning-soft`, …). View code names roles, never hues; a third theme is one attribute block
  in `styles.css`, never a sweep through the markup. Component CSS (chip, pill, Kbd, badges,
  Markdown) reads the same raw variables.
- **OS first, config override second.** The inline `<head>` script in both windows seeds
  `data-theme` from `prefers-color-scheme` **before the first paint**; `theme.ts` then applies the
  config preference — `[ui] theme = "system" | "light" | "dark"` (default `system`) — served by the
  new `ui_theme` command, and keeps re-resolving the OS appearance live while the preference is
  `system`. Config changes apply at restart, like the rest of `config.toml`.
- **Fills carry their own on-color.** Filled controls (Send, Stop) are not "white text on a hue":
  light theme = sky-700 / amber-700 with white text; dark theme = sky-500 / amber-500 with dark
  ink. Both themes keep ≥4.5:1 on the fill, so the fill can be vibrant without failing text.
- **Every text step ≥4.5:1** in both themes (`fg` / `fg-muted` / `fg-subtle`); `fg-faint` is
  decorative-only (muted fallback icons, separators) and is never the sole carrier of information.
  Selected rows are an accent tint, not a heavier gray; hover is a neutral wash (the old hover
  ring is gone — the wash carries the state alone).
- **Theme-aware chrome**: `color-scheme` per theme (native scrollbars/form controls keep the macOS
  overlay style and follow the appearance), `::selection`, a shared `.moe-focus-ring` for buttons
  (the chip no longer eats `focus-visible`), 100–150ms color transitions on interactive states, and
  `prefers-reduced-motion` collapsing transitions and the generating-dots bounce.

## Cost

- The dark palette is not byte-identical to the old zinc one (accent-tinted selection, neutral
  hover wash without the ring, slightly deeper window surface). Deliberate: one role vocabulary
  buys both themes; the old look is not kept as a third mode.
- Contrast is now part of the token definitions: changing a hue means re-measuring the text steps
  against the composed surfaces (the panel is translucent, so surfaces are measured with their
  blur-composed background).
- The chat window and the panel resolve the theme independently at startup; there is no cross-
  window theme broadcast (both read the same config value).

## Amendment: the About card switches the appearance at runtime

The preference lived only in `config.toml` (restart to apply) — fine for a file-first app, wrong
for the one setting a user tries first. The About card gains a **Theme** section — System / Light /
Dark, the active row carrying a trailing check — and the card stays open so a second option can be
tried immediately.

- **`set_theme`** (new command) parses the value, persists it, updates `AppState.theme` and
  broadcasts `theme-changed`; both windows listen (theme.ts) and re-theme live — the Side View
  switches with the panel instead of converging at the next restart. The Cost note above is
  superseded for runtime switches.
- **The config file is edited surgically, never re-serialized.** `set_theme_in_toml` (pure,
  unit-tested) replaces the `theme` key inside `[ui]`, inserts the key under the header, or appends
  the section; every other line — comments included — survives byte for byte, and a commented-out
  `# theme = …` is not the key. A missing file is created from the default template first.
- **`AppState.theme` is the live preference** `ui_theme` serves from then on, so a reloaded window
  agrees with the switch; `config.toml` stays the startup seed and the record the user reads
  (Open Config File shows the stored choice).

## Amendment: the macOS material, and one corner ladder

Two refinements after living with the themes:

- **The material is macOS glass, not a flat translucent fill.** The window card, the floating cards,
  the chip and the pill blur with `blur(32 | 40 | 24px) saturate(180%)` — blur plus saturation is
  the vibrancy recipe — under a per-theme top sheen (`--moe-glass-sheen`); the shadow tokens now
  carry an inset top highlight, and the hairline border stays. Surface opacity stays honest: the
  panel floats over arbitrary apps, so the text steps were re-measured against the worst composed
  surface (translucent panel over a white app in dark, over black in light) — the subtle step moved
  to `#9a9aa3` (dark) / `#65656d` (light) to keep every step ≥4.5:1. Where the engine has continuous
  corners (Safari 26 / macOS 26), `corner-shape: squircle` renders Apple's superellipse on the big
  surfaces and the capsules; older engines keep circular corners.
- **One corner ladder, anchored on the capsules.** The window corner is **20px** — exactly the
  capsule radius (chip, pill) and the collapsed About avatar's circle — instead of the old 16px
  that sat between the capsules and everything else. Each nesting step then subtracts its inset:
  8px inset (floating cards, list rows) → 12px; 4px inset (rows inside a card) → 8px; key blocks and
  badges keep their 5–6px. One token (`--radius-window: 20px`); the rest is Tailwind's `rounded-xl`
  / `rounded-lg` used per the ladder.

## Amendment: two material layers, per the Apple HIG

The previous amendment made every surface glass. The HIG (Materials) is explicit that this is the
wrong split: **Liquid Glass is for controls and navigation, floating above the content layer, and
the content layer itself uses a standard material** — and glass is to be used *sparingly*. Moe's
panel is a text-heavy content surface, so:

- **Content layer = standard material.** The window card (panel and Side View) blurs with mild
  saturation (`blur(28px) saturate(150%)`) and carries **no** specular sheen; it is also the
  thicker surface (dark 0.92 / light 0.95), because thicker materials hold text contrast better.
- **Functional layer = Liquid Glass, regular variant.** The floating cards (actions / About /
  history) and the control clusters (avatar chip, action pill, Side View toolbar) get heavy blur +
  saturation (`blur(40 | 28px) saturate(180%)`), the top sheen (`--moe-glass-sheen`), and the inset
  top-highlight / bottom-shade that reads as glass thickness. The *regular* variant, not clear:
  these surfaces carry text (the HIG's rule for popovers and sidebars).
- **The Side View's toolbar became a glass capsule cluster** with borderless circular buttons — the
  same component as the panel's action pill, so both windows speak one control language. The
  composer is now a bordered field that brightens to the accent hairline on focus, and the filled
  Send / Stop buttons are capsules.
- **The accessibility fallbacks the HIG requires**: `prefers-reduced-transparency: reduce` swaps
  every surface for an opaque color and drops the blur and sheen; `prefers-contrast: more`
  strengthens the hairlines and the subtle text step; `prefers-reduced-motion` was already handled.
- Superseded from the previous amendment: the window card's sheen and inset highlight (glass optics
  now live only on the functional layer) and the surface alphas, which were re-measured for the new
  split — still ≥4.5:1 per text step against the worst composed backdrop.

Not done, deliberately: native `NSVisualEffectView` / Liquid Glass views. The CSS recipe keeps one
code path for macOS and Linux and needs no SDK or OS-version gating; the native material is the
option to revisit if the CSS glass stops matching the platform.

## Amendment: Siri-window grade glass (the macOS 27 reference)

The reference is the macOS 27 Siri window: a clear-ish glass card with no hairline border (the edge
is a specular rim), blur heavy enough that the backdrop reads through as soft forms, and standalone
circular glass buttons floating at the corners. Applied within what CSS can do:

- **Clear-ish glass over a dimming scrim.** The window card is a thin tint over a per-theme dimming
  layer (the HIG's legibility scrim for clear glass) over `blur(64px) saturate(180%)`; the floating
  cards and the controls reuse the same optics at 48 / 32px. The scrim is what keeps the extra
  translucency honest: every text step still measures ≥4.5:1 against the worst composed backdrop.
- **No hairline borders — the edge is a rim.** Window, cards, capsules and circular buttons drop
  their 1px border; `inset 0 0 0 1px` rings inside the shadow tokens draw the glass edge, with the
  specular top/bottom insets. This removed the card's border from the panel's row grid:
  `CARD_BORDER` 1 → 0 and the first-frame height 512 → 510 (tauri.conf follows); the measured-height
  path (ADR-0026) is untouched.
- **Standalone circular glass buttons.** The Side View's header controls became four circular glass
  buttons (Siri-style), superseding the previous amendment's capsule cluster; the panel keeps its
  capsules because they carry labels. The user's question bubble became an accent-tinted glass
  bubble instead of a solid fill.
- **Radius ladder, Siri proportions:** window 24 → cards, list rows and bubbles 16 → rows inside a
  card 12 → key blocks and badges 6; controls stay capsules or circles.
- **Scroll edge effect:** the list, the detail pane and the message pane fade into the glass at
  their edges (mask gradient), so content dissolves under the floating controls instead of being
  cut by the container.
- **Newer targets:** `minimumSystemVersion` 11 → 15 (Safari 18 brings unprefixed `backdrop-filter`
  and `prefers-reduced-transparency`); `corner-shape: squircle` stays progressive for Safari 26+.
- **Shadows stay contact-only.** The window is just 12px bigger than the card (ADR-0016) and the
  floating cards and chips sit 8px inside it, so any wide CSS shadow is clipped by the window
  bounds — which is exactly how the first cut of this amendment read: a rounded rectangle ghosting
  below the panel, its blur cut off at the window edge. The drops are now 1–3px contacts plus the
  inset rim and specular layers; the lift stays AppKit's window shadow.

Still CSS, still one code path for macOS and Linux. What CSS cannot do is the Siri window's
*luminance adaptation* — its glass lightens over light backdrops — which needs an
`NSVisualEffectView` / glass view behind the webview. If we take that native step, the effect view
must be masked to the rounded card and its `NSAppearance` kept in sync with `[ui] theme`.

## Amendment: figure/ground — a tonal ladder, and dark hairlines in light mode

Clear-ish glass failed its first real test: summoned over a white desktop in light mode, the
window, the cards and the capsules all composited to the same white, and the light-mode rim (a
white specular ring) was invisible on white — components dissolved into the background. The
palette now carries separation by construction:

- **A four-step tonal ladder per theme**: desktop → window (cool gray glass in light, near-black in
  dark) → floating cards (near-white, a lighter gray) → controls and capsules (gray glass, lighter
  still). Each step is a visible tone delta against the layer under it, on any backdrop.
- **Rims are dark hairlines in light mode** (`inset 0 0 0 1px` at 12–14% black, above the white top
  highlight) and light hairlines in dark mode. The specular highlight stays; the *edge* is what
  separates.
- The dim layer is recalibrated per theme (light 0.72 white, dark 0.55 black) so the ladder holds
  over both extremes — a white app behind dark glass, black behind light glass — and every text
  step still measures ≥4.5:1 against its own layer's worst composite; the light subtle step moved
  to `#5f5f67`.
- `prefers-reduced-transparency` now swaps to the ladder's opaque tones rather than pure
  white/black, so the layers keep separating with transparency off.

## Amendment: light theme = pale-blue field, white glass components

The gray ladder read "too dark": gray-on-gray lost the pop of the white components and the whole
panel sank into mud on a white desktop. The light theme now inverts the dark theme's logic: **the
window is the tinted layer** — a pale-blue glass (`rgb(224 239 252 / 0.88)` over a white dim, ≈
`#e0effc` on a white desktop) — and **every component is white glass floating on it**: cards at
0.90, capsules / bubbles / toasts at 0.82, key caps solid white. Ink and hairlines went cool
blue-gray (`#182230 / #4c5a6b / #556274`; rims and dividers at 12–22% of `rgb(23 49 89)`) so edges
read on the blue field without going black. Selection stays an accent tint (it must read on white
cards too), hover a cool wash, and the shadows carry cool rims. Every step re-verified ≥4.5:1
against its own layer's worst composite — blue field, white card, selected tint — and the
`prefers-reduced-transparency` light fallbacks moved to the same ladder (`#e3f0fc / #ffffff /
#f4f9ff`). The dark theme is unchanged.

## Amendment: hierarchy by contrast gap, not by uniform darkening

Darkening every text step at once compressed the hierarchy — titles, extension names and timestamps
all read as one ink, and key information stopped popping. The gap is restored: the body/title step
goes near-black in light (`#0b1220`) and near-white in dark (`#fbfbfd`), while secondary
(`--moe-fg-muted`) and tertiary (`--moe-fg-subtle`) return to their pre-change values
(`#4c5a6b / #556274` and `#a1a1aa / #9a9aa3`). Glanceability comes from the ink gap between the
primary step and everything below it.