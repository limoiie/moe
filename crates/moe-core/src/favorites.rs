//! Favorites (ADR-0027): a user-curated, ordered set of command ids pinned into their own
//! section above Suggestions on the root page. Platform-level data (like frecency): any
//! Extension's commands can be favorited, and the set is stored outside every Namespace.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Favorites {
    ids: Vec<String>,
}

impl Favorites {
    /// Ids in user order (oldest addition first) — the section renders in this order.
    pub fn ids(&self) -> &[String] {
        &self.ids
    }

    pub fn contains(&self, id: &str) -> bool {
        self.ids.iter().any(|known| known == id)
    }

    /// Toggle one command; returns true when it is favorited afterwards.
    pub fn toggle(&mut self, id: &str) -> bool {
        if self.remove(id) {
            false
        } else {
            self.ids.push(id.to_string());
            true
        }
    }

    /// Remove one command; returns whether it was present.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.ids.len();
        self.ids.retain(|known| known != id);
        self.ids.len() != before
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_adds_then_removes_without_duplicates() {
        let mut favorites = Favorites::default();
        assert!(favorites.is_empty());
        assert!(favorites.toggle("ai.quick-ask"), "first toggle favorites");
        assert!(favorites.contains("ai.quick-ask"));
        // Toggling again removes (it flips)
        assert!(!favorites.toggle("ai.quick-ask"));
        assert!(!favorites.contains("ai.quick-ask"));
        // Re-adding never duplicates
        assert!(favorites.toggle("ai.quick-ask"));
        assert_eq!(favorites.ids(), ["ai.quick-ask"], "no duplicate entries");
        // Removing something absent is a no-op
        assert!(!favorites.remove("nope"));
        assert_eq!(favorites.len(), 1);
    }

    #[test]
    fn order_is_insertion_order_and_stable() {
        let mut favorites = Favorites::default();
        favorites.toggle("b");
        favorites.toggle("a");
        favorites.toggle("c");
        assert_eq!(favorites.ids(), ["b", "a", "c"]);
        // Re-adding after removal appends at the end
        favorites.remove("b");
        favorites.toggle("b");
        assert_eq!(favorites.ids(), ["a", "c", "b"]);
    }

    #[test]
    fn serde_round_trip() {
        let mut favorites = Favorites::default();
        favorites.toggle("moe.open-config");
        let json = serde_json::to_string(&favorites).unwrap();
        assert!(json.contains("moe.open-config"));
        let back: Favorites = serde_json::from_str(&json).unwrap();
        assert_eq!(back, favorites);
    }
}
