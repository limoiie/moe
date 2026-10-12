// When the palette's loading hairline is shown — a timing question, not a rendering one (MOE-0055).
//
// The hairline used to appear and disappear on every keystroke: the palette's search is local and
// finishes in a few milliseconds, so a flag that tracked the load exactly flashed on and off, and
// because the bar was a flex sibling it also moved the list by its own height each time. The layout half
// is CSS (the bar is positioned out of flow now); this is the other half — two rules that make the bar
// mean "this is taking a moment" instead of "a keystroke happened":
//
//   * a load shorter than `delayMs` never shows it at all, which is the common case;
//   * once shown it stays for `minVisibleMs`, so a load that lands just after the delay does not blink.
//
// Real timers: `vi.useFakeTimers()` drives them in the test, so nothing here needs injecting.

export interface LoadingHairline {
  /** The operation started (`true`) or finished (`false`). Safe to call repeatedly with either. */
  set(on: boolean): void;
}

export interface LoadingOptions {
  /** How long a load may run before the hairline appears at all. */
  delayMs: number;
  /** How long it stays once it has appeared. */
  minVisibleMs: number;
  /** Called only when the *visible* state changes — never for a load that never showed. */
  onChange: (visible: boolean) => void;
}

export function createLoadingHairline(options: LoadingOptions): LoadingHairline {
  const { delayMs, minVisibleMs, onChange } = options;
  let showTimer: ReturnType<typeof setTimeout> | undefined;
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  let visible = false;
  let shownAt = 0;

  const show = () => {
    showTimer = undefined;
    if (visible) return;
    visible = true;
    shownAt = Date.now();
    onChange(true);
  };

  const hide = () => {
    hideTimer = undefined;
    if (!visible) return;
    visible = false;
    onChange(false);
  };

  return {
    set(on) {
      if (on) {
        // A new load while the bar is on its way out keeps it up: the work never stopped.
        if (hideTimer !== undefined) {
          clearTimeout(hideTimer);
          hideTimer = undefined;
        }
        if (visible || showTimer !== undefined) return;
        showTimer = setTimeout(show, delayMs);
        return;
      }
      if (showTimer !== undefined) {
        // Finished before it ever appeared: nothing to undo, and nothing to flash.
        clearTimeout(showTimer);
        showTimer = undefined;
        return;
      }
      if (!visible) return;
      const remaining = minVisibleMs - (Date.now() - shownAt);
      if (remaining > 0) hideTimer = setTimeout(hide, remaining);
      else hide();
    },
  };
}
