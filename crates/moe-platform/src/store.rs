//! Platform-level KV: persisting command frecency and window position/size.
//!
//! Command usage records and window state don't belong to any Extension's Namespace (the isolation domain is reserved for extension-owned data),
//! so they live in the platform's own data directory (IIE4AD-346 / IIE4AD-369).

use std::collections::HashMap;
use std::path::PathBuf;

use moe_core::favorites::Favorites;
use moe_core::frecency::Frecency;
use serde::{Deserialize, Serialize};

/// `data_dir/moe/frecency.json` (macOS: ~/Library/Application Support; Linux: ~/.local/share).
pub fn frecency_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("moe").join("frecency.json"))
}

pub fn load_frecency() -> Frecency {
    let Some(path) = frecency_path() else {
        return Frecency::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|err| {
            eprintln!(
                "moe: {} parse failed ({err}), frecency reset",
                path.display()
            );
            Frecency::default()
        }),
        Err(_) => Frecency::default(),
    }
}

pub fn save_frecency(frecency: &Frecency) {
    let Some(path) = frecency_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_string(frecency) {
        Ok(json) => {
            if let Err(err) = std::fs::write(&path, json) {
                eprintln!("moe: saving frecency failed: {err}");
            }
        }
        Err(err) => eprintln!("moe: serializing frecency failed: {err}"),
    }
}

/// `data_dir/moe/favorites.json` — the user-curated command set (ADR-0027), platform-level
/// (outside every Namespace, like frecency).
pub fn favorites_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("moe").join("favorites.json"))
}

pub fn load_favorites() -> Favorites {
    let Some(path) = favorites_path() else {
        return Favorites::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|err| {
            eprintln!(
                "moe: parsing {} failed ({err}); favorites reset",
                path.display()
            );
            Favorites::default()
        }),
        Err(_) => Favorites::default(),
    }
}

pub fn save_favorites(favorites: &Favorites) {
    let Some(path) = favorites_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_string(favorites) {
        Ok(json) => {
            if let Err(err) = std::fs::write(&path, json) {
                eprintln!("moe: saving favorites failed: {err}");
            }
        }
        Err(err) => eprintln!("moe: serializing favorites failed: {err}"),
    }
}

/// Window position and size (logical pixels, top-left origin).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowFrame {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// `data_dir/moe/windows.json`: window name → frame.
pub fn windows_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("moe").join("windows.json"))
}

fn load_frames() -> HashMap<String, WindowFrame> {
    let Some(path) = windows_path() else {
        return HashMap::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return HashMap::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// Read a window's remembered frame; None when absent (the caller falls back to default placement).
pub fn load_window_frame(name: &str) -> Option<WindowFrame> {
    load_frames().remove(name)
}

/// Remember a window's position and size (call after drag/resize).
pub fn save_window_frame(name: &str, frame: WindowFrame) {
    let Some(path) = windows_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut frames = load_frames();
    frames.insert(name.to_string(), frame);
    match serde_json::to_string(&frames) {
        Ok(json) => {
            if let Err(err) = std::fs::write(&path, json) {
                eprintln!("moe: window state save failed: {err}");
            }
        }
        Err(err) => eprintln!("moe: window state serialization failed: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Favorites JSON shape (ADR-0027): a flat `ids` list, round-trippable.
    #[test]
    fn favorites_serialize_as_an_ordered_id_list() {
        let mut favorites = Favorites::default();
        favorites.toggle("ai.quick-ask");
        favorites.toggle("moe.open-config");
        let json = serde_json::to_string(&favorites).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["ids"][0], "ai.quick-ask");
        assert_eq!(parsed["ids"][1], "moe.open-config");
        let back: Favorites = serde_json::from_str(&json).unwrap();
        assert_eq!(back, favorites);
    }

    #[test]
    fn window_frames_round_trip_in_memory() {
        // Verify the JSON shape directly (the persist-to-disk path depends on data_dir and isn't tested here)
        let frame = WindowFrame {
            x: 100,
            y: 24,
            width: 860,
            height: 900,
        };
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains("\"x\":100") && json.contains("\"width\":860"));
        let back: WindowFrame = serde_json::from_str(&json).unwrap();
        assert_eq!(back, frame);

        let mut frames = HashMap::new();
        frames.insert("chat".to_string(), frame);
        let json = serde_json::to_string(&frames).unwrap();
        let parsed: HashMap<String, WindowFrame> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.get("chat"), Some(&frame));
    }
}
