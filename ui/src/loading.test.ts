// The loading hairline's timing rules (MOE-0055). The failure this exists to prevent is a bar that
// flashes on every keystroke, so the cases are all about *not* showing it.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createLoadingHairline } from "./loading";

const DELAY = 150;
const MIN_VISIBLE = 250;

describe("createLoadingHairline", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  function hairline() {
    const onChange = vi.fn();
    return { onChange, line: createLoadingHairline({ delayMs: DELAY, minVisibleMs: MIN_VISIBLE, onChange }) };
  }

  it("never shows for a load that finishes inside the delay", () => {
    // The common case: the palette's search is local and lands in a few milliseconds.
    const { onChange, line } = hairline();
    line.set(true);
    vi.advanceTimersByTime(40);
    line.set(false);
    vi.advanceTimersByTime(1000);
    expect(onChange).not.toHaveBeenCalled();
  });

  it("shows once the load passes the delay", () => {
    const { onChange, line } = hairline();
    line.set(true);
    vi.advanceTimersByTime(DELAY - 1);
    expect(onChange).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it("holds for the minimum once it is up, then hides", () => {
    const { onChange, line } = hairline();
    line.set(true);
    vi.advanceTimersByTime(DELAY); // visible at 150
    line.set(false); // the load lands immediately after
    vi.advanceTimersByTime(MIN_VISIBLE - 1);
    expect(onChange).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1);
    expect(onChange).toHaveBeenLastCalledWith(false);
  });

  it("does not extend the minimum for a long load", () => {
    // A load that outlasts the minimum hides as soon as it lands.
    const { onChange, line } = hairline();
    line.set(true);
    vi.advanceTimersByTime(DELAY + MIN_VISIBLE + 100);
    line.set(false);
    expect(onChange).toHaveBeenLastCalledWith(false);
  });

  it("keeps one bar across a burst of overlapping loads", () => {
    // Typing: a re-run starts before the previous one settles. One show, one hide.
    const { onChange, line } = hairline();
    line.set(true);
    vi.advanceTimersByTime(200); // visible
    line.set(true); // a new keystroke's re-run
    line.set(false); // …and the first one settles
    vi.advanceTimersByTime(1000);
    line.set(false);
    expect(onChange.mock.calls).toEqual([[true], [false]]);
  });

  it("ignores repeated calls with the same state", () => {
    const { onChange, line } = hairline();
    line.set(true);
    line.set(true);
    vi.advanceTimersByTime(DELAY);
    line.set(false);
    line.set(false);
    vi.advanceTimersByTime(MIN_VISIBLE);
    expect(onChange.mock.calls).toEqual([[true], [false]]);
  });
});
