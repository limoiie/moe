//! frecency: "frequency + recency" ordering weight for commands.
//!
//! Platform-level data, outside every Extension's Namespace (ADR-0003 leaves the isolated domain to extension-owned data).

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const HOUR: u64 = 3_600;
const DAY: u64 = 86_400;
const WEEK: u64 = 7 * DAY;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Frecency {
    entries: HashMap<String, Entry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Entry {
    count: u32,
    last_used_unix: u64,
}

impl Frecency {
    pub fn record(&mut self, id: &str, now: SystemTime) {
        let entry = self.entries.entry(id.to_string()).or_insert(Entry {
            count: 0,
            last_used_unix: 0,
        });
        entry.count += 1;
        entry.last_used_unix = unix_secs(now);
    }

    /// frequency × freshness buckets: ×4 within an hour, ×2 within a day, ×1 within a week, ×0.25 beyond.
    pub fn score(&self, id: &str, now: SystemTime) -> f64 {
        let Some(entry) = self.entries.get(id) else {
            return 0.0;
        };
        let age = unix_secs(now).saturating_sub(entry.last_used_unix);
        let freshness = if age <= HOUR {
            4.0
        } else if age <= DAY {
            2.0
        } else if age <= WEEK {
            1.0
        } else {
            0.25
        };
        entry.count as f64 * freshness
    }

    /// Most recent use time (unix seconds); None if never recorded. "Suggestions" sort by it descending (IIE4AD-395).
    pub fn last_used(&self, id: &str) -> Option<u64> {
        self.entries.get(id).map(|entry| entry.last_used_unix)
    }

    /// Forget one command's usage history (⌃X on a suggestion, ADR-0025).
    /// The command itself stays listed in its source section; only its recency is gone.
    pub fn remove(&mut self, id: &str) -> bool {
        self.entries.remove(id).is_some()
    }

    /// Forget ALL usage history (⌃⇧X on suggestions, ADR-0025): suggestions vanish
    /// and search ordering falls back to plain matching order.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of commands with recorded usage (feedback for "clear all").
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when no usage has ever been recorded.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Seam for search ordering to query frecency: the Registry depends only on it, so tests can inject fixed values.
pub trait FrecencyLookup {
    fn frecency(&self, command_id: &str) -> f64;

    /// Most recent use time (unix seconds); None if never recorded (default).
    fn last_used(&self, _command_id: &str) -> Option<u64> {
        None
    }
}

impl FrecencyLookup for Frecency {
    fn frecency(&self, command_id: &str) -> f64 {
        self.score(command_id, SystemTime::now())
    }

    fn last_used(&self, command_id: &str) -> Option<u64> {
        self.entries
            .get(command_id)
            .map(|entry| entry.last_used_unix)
    }
}

/// No-frecency scenario (tests, lightweight callers).
pub struct NoFrecency;

impl FrecencyLookup for NoFrecency {
    fn frecency(&self, _command_id: &str) -> f64 {
        0.0
    }
}

fn unix_secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn unrecorded_commands_have_no_score() {
        let f = Frecency::default();
        assert_eq!(f.score("echo.write-back", at(0)), 0.0);
    }

    #[test]
    fn score_grows_with_use_and_decays_with_age() {
        let mut f = Frecency::default();
        f.record("a", at(0));
        let once = f.score("a", at(0));
        assert!(once > 0.0, "first use should have a score");

        f.record("a", at(0));
        assert!(
            f.score("a", at(0)) > once,
            "two uses should score higher than one"
        );

        // Same use count, older scores lower (decay buckets)
        let mut g = Frecency::default();
        g.record("b", at(0));
        g.record("b", at(0));
        let fresh = g.score("b", at(0));
        let day_old = g.score("b", at(90_000));
        let month_old = g.score("b", at(30 * DAY));
        assert!(fresh > day_old, "day-old should score lower than fresh");
        assert!(
            day_old > month_old,
            "month-old should score lower than day-old"
        );
    }

    #[test]
    fn recent_use_beats_many_old_uses() {
        let mut f = Frecency::default();
        for _ in 0..3 {
            f.record("old", at(0));
        }
        f.record("new", at(30 * DAY));
        assert!(
            f.score("new", at(30 * DAY)) > f.score("old", at(30 * DAY)),
            "one recent use should beat three uses a month ago"
        );
    }

    #[test]
    fn serde_round_trip() {
        let mut f = Frecency::default();
        f.record("a", at(123));
        let json = serde_json::to_string(&f).unwrap();
        let back: Frecency = serde_json::from_str(&json).unwrap();
        assert_eq!(f, back);
    }

    /// Suggestions (IIE4AD-395) depend on last_used: only recorded commands have a time; unrecorded ones are None.
    #[test]
    fn last_used_is_only_set_after_recording() {
        let mut f = Frecency::default();
        assert_eq!(f.last_used("a"), None);
        f.record("a", at(100));
        assert_eq!(f.last_used("a"), Some(100));
        f.record("a", at(200));
        assert_eq!(
            f.last_used("a"),
            Some(200),
            "repeated use updates the latest time"
        );
    }

    /// Forget usage (ADR-0025): removing one entry only affects that command;
    /// clear() empties everything (suggestions + ordering weights).
    #[test]
    fn remove_and_clear_forget_usage() {
        let mut f = Frecency::default();
        assert!(f.is_empty());
        f.record("a", at(1));
        f.record("b", at(2));
        assert_eq!(f.len(), 2);
        assert!(f.remove("a"), "first removal reports success");
        assert!(!f.remove("a"), "removing again is a no-op");
        assert_eq!(f.last_used("a"), None);
        assert_eq!(f.last_used("b"), Some(2));
        f.clear();
        assert!(f.is_empty());
        assert_eq!(f.last_used("b"), None);
        assert_eq!(f.score("b", at(2)), 0.0);
    }
}
