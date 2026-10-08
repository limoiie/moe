// Pointer intent: hover is a pointer affordance, not a render artifact. While the keyboard drives
// the UI, every render replaces the row DOM under a stationary cursor, and CSS :hover recomputes
// on the fresh nodes — the hover wash flickered on arbitrary rows while navigating with ↑↓/⌃N/⌃P
// or while typing. The root carries data-pointer="live" after real pointer activity (mousemove or
// mousedown) and "stale" again on any keydown (capture phase, so card-owned keys count); the hover
// styles of re-rendered surfaces are gated on "live" in styles.css. Persistent controls keep their
// plain :hover — they never re-render under the cursor.

export function initPointerIntent(): void {
  const root = document.documentElement;
  let live = false;

  const sync = () => {
    root.dataset.pointer = live ? "live" : "stale";
  };
  sync();

  const wake = () => {
    if (!live) {
      live = true;
      sync();
    }
  };
  window.addEventListener("mousemove", wake);
  window.addEventListener("mousedown", wake);
  window.addEventListener(
    "keydown",
    () => {
      if (live) {
        live = false;
        sync();
      }
    },
    { capture: true },
  );
}