# M7z: the Side View window — chrome, a ChatGPT composer, a proper default width, legible rows

- **ID**: MOE-0018
- **State**: in-progress
- **Labels**: ux
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent

User feedback, four asks on the side-chat sub-app:

1. "it should be a window, that is, it should have the three buttons at the left end of the topbar;
   the title should center in the topbar; the several icon actions should be inner a capsule"
2. "i prefer the ui&ux of chatgpt's composer, please redesign ours to own the same style"
3. "reduce the initial window width to a proper size, preserving the size and position across app's
   launches"
4. "optimize the ui of browser-card/mod+p, currently, the items are so tiny which doesn't fit
   current design"

## Design

- **Window chrome**: the Side View is a borderless floating NSPanel (ADR-0004/0008), so the trio is
  drawn in the titlebar instead of turning on native decorations: custom traffic lights (the system
  colors, glyphs revealed on trio hover), a titlebar centered title, and the three icon actions
  moved into a glass capsule (the panel pill's rim, one size down). The lights are **Hide** (the
  standard close for a resident panel), **shrink width** and **expand width** — real size steps for
  a docked window. Native minimize/zoom are deliberately not used: the app has no Dock icon (a
  minimized window would have no way back) and the window is a non-activating panel.
- **Composer**: one rounded field (24 px radius, `surface-overlay`, rim shadow, the border brightens
  on focus) with the draft on top and the attachment and send buttons on a row inside it — ChatGPT's
  composer. The send button is a circular icon button (↥ send / ■ stop) that greys out while the
  draft is empty; the field grows with the draft up to 160 px. The shortcut stays in the tooltip.
- **Default width**: the chat window's default drops 860 → **460** (min 560 → **360**). The frame
  store (physical position+size, debounced after drag/resize) already persists the frame across
  launches — unchanged, and now also fed by the titlebar's width steps.
- **History card (⌘P / Browse)**: read as the floating history card (the panel's own Browse page
  already uses the panel's row metrics). Its rows adopt those metrics — text-sm, 15 px icon,
  2.5 gaps, 13 px time — instead of the shrunken text-xs scale; the two floating cards become
  responsive (`min(24rem, 100% - 2.5rem)`) so they fit the new narrow widths.

## Delivery

- `ui/chat.html`: the titlebar (traffic lights, centered title, action capsule), the ChatGPT
  composer (the attachment row moved inside the field), responsive card widths.
- `ui/src/styles.css`: `.moe-traffic-lights` / `.moe-traffic-light`, `.moe-window-actions`,
  `.moe-composer` / `.moe-composer-action`.
- `ui/src/chat.ts`: the window-control handlers (`setWindowWidth` keeps the docked right edge fixed;
  the preset buttons dim once their size is reached), the icon-only circular send/stop button with
  an empty-draft disabled state, the auto-growing composer, and the history card's row metrics.
- `ui/src/icons.ts`: `arrow-up`.
- `crates/moe-app/tauri.conf.json`: the chat window's default and minimum width.
- Docs: README Side View paragraph.

## Acceptance

- `pnpm -C ui build` green (tsc + vite).
- GUI check pending: the trio sits at the left (glyphs appear on trio hover), the title is centered,
  the capsule holds More Actions / History / New Chat; the composer is one rounded field with the
  attach and send buttons inside, the send button greys out on an empty draft and becomes ■ while
  generating; a fresh install opens 460 wide and remembers drags/resizes and the width steps across
  launches; the history card's rows read at the panel's scale.

## Comments

- 2026-10-09 (agent): filed from user feedback; delivered in the working tree (uncommitted).
- 2026-10-09 (agent): follow-up round — the reported "no send button / Enter does nothing / the close
  button is focused" traced to one real bug shipped in this issue: the rewritten `chat.html` dropped
  `#attach-path` and `#attach-msg` from `#attach-row`, so `chat.ts` threw at
  `attachPathEl.addEventListener(...)` at module scope and aborted before wiring the composer (no
  send icon, no Enter listener, no autofocus) — the first focusable element (the red light) took
  focus. Found with a throwaway DOM-shim probe (querySelector returning null for ids absent from
  the markup, which is what a real browser does). Restored both elements. Also fixed
  `autoGrowComposer` measuring 0 while the window is hidden (it collapsed the field to `0px` — no
  caret, nothing selectable); it now keeps the CSS height and re-grows on show. The traffic-light
  glyphs became stroked icons (lucide X / Minus / Plus at 8px, stroke 3, revealed on trio hover)
  instead of text glyphs, which rendered the middle "−" much smaller than its neighbours; the
  `titlebar-reveal` hover hiding was already in place (needs the rebuilt bundle to show).
- 2026-10-09 (agent): Raycast-alignment round — the Side View gets its own corner ladder (window
  card 16 px `rounded-2xl`, composer 12 px `rounded-xl`, attachment strip 8 px `rounded-lg`; the
  capsules stay fully rounded), a tighter topbar (40 px band; the trio 16 px in from the left, the
  capsule 8 px in from the right to match its 6 px vertical gaps, 24 px capsule buttons with 15 px
  icons), and the panel keeps its 24 px `--radius-window` token; ADR-0035's amendment records the
  split.
- 2026-10-09 (agent): titlebar chrome states, second pass — the first pass keyed visibility on the
  window's focus, which kept the controls up while the user typed with the pointer elsewhere. The
  pointer and the focus are now split duties: the pointer decides *whether* the chrome shows (over
  the window → trio and capsule muted at .55 opacity and the title lit; out → hidden and the title
  dimmed, even while the composer keeps keyboard focus), and the window's focus decides the weight
  (clicked into the window → fully lit). `body.moe-pointer-inside` tracks
  `mouseenter`/`mouseleave` on the document (reset on `visibilitychange` and on a re-show), and
  `body.moe-window-focused` still comes from Tauri's `isFocused` / `onFocusChanged`.