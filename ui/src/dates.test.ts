// Ported one-for-one from `ai.rs`'s `relative_time_buckets` (ADR-0041): the tested logic moved from
// Rust to the UI, so its cases move with it — this is the coverage the ticket said to port once a
// UI runner existed (MOE-0050 landed it).

import { describe, expect, it } from "vitest";
import { relativeTime } from "./dates";

describe("relativeTime", () => {
  it("matches the Rust formatter's buckets", () => {
    expect(relativeTime(100, 100)).toBe("just now");
    expect(relativeTime(100, 159)).toBe("just now");
    expect(relativeTime(100, 160)).toBe("1m ago");
    expect(relativeTime(100, 100 + 3599)).toBe("59m ago");
    expect(relativeTime(100, 100 + 3600)).toBe("1h ago");
    expect(relativeTime(100, 100 + 86400)).toBe("1d ago");
    expect(relativeTime(100, 100 + 3 * 86400)).toBe("3d ago");
  });

  it("clamps a future timestamp to 'just now'", () => {
    // The Rust formatter used `saturating_sub`; a clock that runs backwards must not read negative.
    expect(relativeTime(200, 100)).toBe("just now");
  });
});
