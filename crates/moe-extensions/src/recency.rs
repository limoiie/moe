//! Recency buckets for grouped lists (ADR-0018 amendment).
//!
//! The list–detail page template groups items by `Item.group`; a history-style list labels those
//! groups by age. The buckets are duration-based (no calendar/timezone handling — the stored
//! timestamps are unix seconds), and the UI mirrors these labels in `ui/src/chat.ts` for the Side
//! View's history card, which cannot call into Rust for a label.

/// Today / Yesterday / Previous 7 Days / Previous 30 Days / Older.
pub fn group_label(updated_unix: u64, now_unix: u64) -> &'static str {
    match now_unix.saturating_sub(updated_unix) {
        0..=86_399 => "Today",
        86_400..=172_799 => "Yesterday",
        172_800..=691_199 => "Previous 7 Days",
        691_200..=2_678_399 => "Previous 30 Days",
        _ => "Older",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 86_400;

    #[test]
    fn buckets_cover_the_edges() {
        let now = 10_000_000u64;
        assert_eq!(group_label(now, now), "Today");
        assert_eq!(group_label(now - 86_399, now), "Today");
        assert_eq!(group_label(now - DAY, now), "Yesterday");
        assert_eq!(group_label(now - 2 * DAY + 1, now), "Yesterday");
        assert_eq!(group_label(now - 2 * DAY, now), "Previous 7 Days");
        assert_eq!(group_label(now - 8 * DAY + 1, now), "Previous 7 Days");
        assert_eq!(group_label(now - 8 * DAY, now), "Previous 30 Days");
        assert_eq!(group_label(now - 31 * DAY + 1, now), "Previous 30 Days");
        assert_eq!(group_label(now - 31 * DAY, now), "Older");
    }

    /// A clock skew (an update "in the future") must not panic or mislabel.
    #[test]
    fn future_timestamps_read_as_today() {
        assert_eq!(group_label(10_000_100, 10_000_000), "Today");
    }
}
