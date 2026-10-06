//! frecency：命令的「频率 + 最近使用」排序权重。
//!
//! 平台级数据，不属于任何 Extension 的 Namespace（ADR-0003 的隔离域留给扩展自己的数据）。

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

    /// 频率 × 新鲜度分桶：一小时内 ×4、一天 ×2、一周 ×1、更久 ×0.25。
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

    /// 最近一次使用时间（unix 秒）；没记录过为 None。「建议」按它倒序（IIE4AD-395）。
    pub fn last_used(&self, id: &str) -> Option<u64> {
        self.entries.get(id).map(|entry| entry.last_used_unix)
    }
}

/// 搜索排序查询 frecency 的接缝：Registry 只依赖它，测试可注入固定值。
pub trait FrecencyLookup {
    fn frecency(&self, command_id: &str) -> f64;

    /// 最近一次使用时间（unix 秒）；没有记录为 None（默认）。
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

/// 无 frecency 场景（测试、轻量调用方）。
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
        assert!(once > 0.0, "首次使用后应有分数");

        f.record("a", at(0));
        assert!(f.score("a", at(0)) > once, "用两次应高于一次");

        // 同样的使用次数，越久远分数越低（衰减分桶）
        let mut g = Frecency::default();
        g.record("b", at(0));
        g.record("b", at(0));
        let fresh = g.score("b", at(0));
        let day_old = g.score("b", at(90_000));
        let month_old = g.score("b", at(30 * DAY));
        assert!(fresh > day_old, "一天后应低于刚用过");
        assert!(day_old > month_old, "一个月后应低于一天前");
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
            "刚用过一次应压过一个月前用过三次"
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

    /// 建议（IIE4AD-395）依赖 last_used：记录过才有时间，未记录为 None。
    #[test]
    fn last_used_is_only_set_after_recording() {
        let mut f = Frecency::default();
        assert_eq!(f.last_used("a"), None);
        f.record("a", at(100));
        assert_eq!(f.last_used("a"), Some(100));
        f.record("a", at(200));
        assert_eq!(f.last_used("a"), Some(200), "重复使用更新最近时间");
    }
}
