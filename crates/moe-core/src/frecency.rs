//! frecency: "frequency + recency" ordering weight for commands.
//!
//! Each use adds 1 to a command's score and the stored score decays exponentially (half-life
//! [`HALF_LIFE_SECS`], ADR-0023 amendment): consistently used commands stay ahead of one-off fresh
//! uses, while abandoned commands fade over months instead of pinning a suggestion slot.
//!
//! Platform-level data, outside every Extension's Namespace (ADR-0003 leaves the isolated domain to extension-owned data).

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const DAY: u64 = 86_400;
/// Score half-life: without use, a command's accumulated score halves every 30 days — the current
/// Firefox frecency default (`halfLifeDays 30`, docs/research/frecency-ranking.md).
const HALF_LIFE_SECS: u64 = 30 * DAY;
/// Below this live score a command reads as forgotten (score 0, out of Suggestions) and is pruned
/// on the next `record`. Mirrors Firefox's adaptive-history deletion threshold 0.975^90 ≈ 0.10.
const MIN_SCORE: f64 = 0.1;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Frecency {
    entries: HashMap<String, Entry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Entry {
    /// Decayed frecency as of `last_used_unix`; `count` is the pre-MOE-0008 field name, kept as a
    /// deserialization alias so existing files seed their score with the old lifetime count.
    #[serde(alias = "count")]
    score: f64,
    last_used_unix: u64,
}

impl Frecency {
    /// Record one use: fold every stored score's decay forward and drop the faded entries (lazy
    /// aging, as Redis's LFU does on access), then add this use.
    pub fn record(&mut self, id: &str, now: SystemTime) {
        let now = unix_secs(now);
        self.entries
            .retain(|_, entry| decayed_score(entry, now) >= MIN_SCORE);
        let entry = self.entries.entry(id.to_string()).or_insert(Entry {
            score: 0.0,
            last_used_unix: now,
        });
        entry.score = decayed_score(entry, now) + 1.0;
        entry.last_used_unix = now;
    }

    /// Live frecency score: uses decay with a 30-day half-life, so frequency and recency are one
    /// number. 0.0 for never-used commands and for faded ones (below `MIN_SCORE`).
    pub fn score(&self, id: &str, now: SystemTime) -> f64 {
        let Some(entry) = self.entries.get(id) else {
            return 0.0;
        };
        let live = decayed_score(entry, unix_secs(now));
        if live >= MIN_SCORE { live } else { 0.0 }
    }

    /// Most recent use time (unix seconds); None if never recorded. "Suggestions" break frecency
    /// ties by it, descending (IIE4AD-395).
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

/// The stored score decayed to `now_secs`: `s · 2^(−(now − last_used) / half-life)` — the standard
/// time-decayed counter (docs/research/frecency-ranking.md).
fn decayed_score(entry: &Entry, now_secs: u64) -> f64 {
    let age = now_secs.saturating_sub(entry.last_used_unix);
    entry.score * 2f64.powf(-(age as f64) / HALF_LIFE_SECS as f64)
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

        // Same use count, older scores lower (exponential decay)
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

    /// Half-life decay: a score halves every 30 days of no use (ADR-0023 amendment).
    #[test]
    fn score_halves_each_half_life() {
        let mut f = Frecency::default();
        f.record("a", at(0));
        let fresh = f.score("a", at(0));
        let after = f.score("a", at(HALF_LIFE_SECS));
        assert!(
            (after - fresh / 2.0).abs() < 0.01,
            "30 days without use should halve the score"
        );
    }

    /// The MOE-0008 acceptance case: a command used regularly (≈1/day) stays clearly ahead of a
    /// lightly used command whose latest use is more recent — frequency dominates raw recency.
    #[test]
    fn heavy_recent_use_beats_a_fresh_but_light_command() {
        let mut f = Frecency::default();
        // 1000 uses, one a day, the last one a day ago.
        for day in 0..1000 {
            f.record("heavy", at(day * DAY));
        }
        // 10 uses over the last ten days, the latest one an hour ago (more recent than heavy's).
        for day in 990..1000 {
            f.record("light", at(day * DAY));
        }
        f.record("light", at(1000 * DAY - 3_600));

        let now = at(1000 * DAY);
        let heavy = f.score("heavy", now);
        let light = f.score("light", now);
        assert!(
            heavy > light * 3.0,
            "heavy={heavy} should stay clearly ahead of light={light}"
        );
    }

    /// MOE-0008: abandoned commands fade — below `MIN_SCORE` they read as forgotten, and the next
    /// record prunes them, so a stale fossil cannot pin a Suggestions slot.
    #[test]
    fn faded_entries_read_as_gone_and_are_pruned_on_record() {
        let mut f = Frecency::default();
        f.record("once", at(0));
        assert!(
            f.score("once", at(95 * DAY)) > 0.0,
            "still warm before the threshold"
        );
        assert_eq!(
            f.score("once", at(110 * DAY)),
            0.0,
            "a use ~3.7 half-lives old reads as forgotten"
        );
        assert_eq!(f.last_used("once"), Some(0), "until a record prunes it");
        f.record("other", at(110 * DAY));
        assert_eq!(
            f.last_used("once"),
            None,
            "lazy aging pruned the faded entry"
        );
        assert_eq!(f.score("other", at(110 * DAY)), 1.0);
    }

    /// Pre-MOE-0008 files stored a lifetime `count`; the serde alias loads it as the initial
    /// decayed score, so existing usage keeps its standing (and then ages normally).
    #[test]
    fn legacy_count_field_loads_as_the_initial_score() {
        let json = r#"{"entries":{"a":{"count":1000,"last_used_unix":0}}}"#;
        let f: Frecency = serde_json::from_str(json).unwrap();
        assert_eq!(f.score("a", at(0)), 1000.0);
        assert_eq!(f.score("a", at(30 * DAY)), 500.0);
        assert_eq!(f.last_used("a"), Some(0));
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
