# Panel placement: the pointer's screen, near the top

Two reports: with a second display attached, summoning from that display always showed the panel on
the first one; and the panel sat vertically centered where Raycast sits near the top.

## Context

Placement went through tauri/tao's `cursor_position()` + `monitor_from_point()`. On macOS tao's
`cursor_position()` mixes units: it takes `NSEvent.mouseLocation` (logical points, bottom-left
origin), flips it against the **main display's pixel height**, and scales by the **primary
monitor's scale factor**. The result lands inside no `CGDisplayBounds` rectangle as soon as the
geometry is Retina (scale 2), so `monitor_from_point` misses, and the old code silently fell back
to the primary monitor. Single-display machines never noticed.

## Decision

- **macOS decides placement in AppKit, not in tauri's coordinate space.** The screen is picked by
  testing `NSEvent.mouseLocation()` against `NSScreen`s' frames (both points, bottom-left origin);
  the window is moved with `setFrameTopLeftPoint`. This bypasses tauri/tao's physical-pixel
  conversions, which cannot be made consistent across mixed-DPI arrangements.
- **The panel is anchored near the top.** Horizontally centered in the screen's *visible* frame
  (menu bar and dock excluded), its top edge `panel_top_drop` below the visible top: **20% of the
  visible height, floored at 48 points**. The math is `moe_platform::screen` (unit-tested); both
  the AppKit path and the tauri-monitor path compose it.
- **Showing follows the pointer; resizing keeps the screen.** Summoning anchors on the screen under
  the cursor (the user just summoned from there). A shape change (ADR-0018) re-anchors on the
  screen the panel already sits on — otherwise a cursor that wandered to another display while the
  user is typing would teleport the panel mid-keystroke.
- **The Side View's default dock uses the same lookup**: right edge of the pointer's screen, full
  visible height, when there is no remembered frame (IIE4AD-369). The tauri-monitor path remains
  the Linux/Windows implementation of both placements.

## Cost

- Two placement paths (AppKit-native on macOS, tauri-monitor elsewhere) must stay behaviorally in
  sync; the shared anchor math keeps them thin.
- AppKit calls require the main thread. Both call sites already run there (the show path and sync
  IPC command handlers); the helpers still guard on `MainThreadMarker` and no-op otherwise.
- macOS placement now works in points; anything that later needs physical pixels (window frame
  persistence does, `store::WindowFrame`) keeps using tauri's own conversions, unchanged.
- The 20% drop is a taste constant: it lives once, in `moe_platform::screen::panel_top_drop`.