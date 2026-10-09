// Stream frames arrive once per model delta — many per second. Rendering each one re-parses the whole
// answer (marked + DOMPurify) and thrashes the scroll container, which is what "not smooth" means.
// A surface pipes its frames through a coalescer instead: the newest value per key is kept and applied
// at most every ~50 ms, so the DOM sees ~20 updates a second regardless of the model's token rate
// (ADR-0038). The final frame is a frame like any other — it is the newest value of its key, so the
// settled answer always lands, at most one flush later.

/** Flush cadence (ms): ~20 updates a second is below the flicker threshold and far below SSE rates. */
export const STREAM_FLUSH_MS = 50;

export interface FrameCoalescer<K, V> {
  /** Record the newest value for a key; each flush applies the latest value per key. */
  push(key: K, value: V): void;
}

export function streamCoalescer<K, V>(apply: (frames: Map<K, V>) => void): FrameCoalescer<K, V> {
  let latest = new Map<K, V>();
  let scheduled: ReturnType<typeof setTimeout> | undefined;
  return {
    push(key, value) {
      latest.set(key, value);
      scheduled ??= setTimeout(() => {
        scheduled = undefined;
        const frames = latest;
        latest = new Map();
        apply(frames);
      }, STREAM_FLUSH_MS);
    },
  };
}