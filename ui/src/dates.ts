// The platform's relative-time renderer (ADR-0041). A `date` accessory carries a unix timestamp on
// the wire, not a formatted string, so the age of a row is decided here. This is the formatter that
// replaced `ai.rs`'s `relative_time` when the history subtitle became a date accessory; the buckets
// are ported one-for-one from it (and its `relative_time_buckets` test, below in `dates.test.ts`).

/**
 * "just now" / "5m ago" / "2h ago" / "3d ago" — the exact buckets the Rust formatter used:
 * under a minute, then whole minutes, hours, and days, each boundary inclusive of its lower bound.
 */
export function relativeTime(unixSeconds: number, nowSeconds: number): string {
  const secs = Math.max(0, nowSeconds - unixSeconds);
  if (secs < 60) return "just now";
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  return `${Math.floor(secs / 86400)}d ago`;
}
