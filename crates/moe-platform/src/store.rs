//! 平台级 KV：命令 frecency 与窗口位置尺寸的持久化。
//!
//! 命令使用记录与窗口状态不属于任何 Extension 的 Namespace（隔离域留给扩展自己的数据），
//! 因此落在平台自己的数据目录（IIE4AD-346 / IIE4AD-369）。

use std::collections::HashMap;
use std::path::PathBuf;

use moe_core::frecency::Frecency;
use serde::{Deserialize, Serialize};

/// `data_dir/moe/frecency.json`（macOS: ~/Library/Application Support；Linux: ~/.local/share）。
pub fn frecency_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("moe").join("frecency.json"))
}

pub fn load_frecency() -> Frecency {
    let Some(path) = frecency_path() else {
        return Frecency::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|err| {
            eprintln!("moe: {} 解析失败（{err}），frecency 重置", path.display());
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
                eprintln!("moe: frecency 保存失败: {err}");
            }
        }
        Err(err) => eprintln!("moe: frecency 序列化失败: {err}"),
    }
}

/// 窗口位置与尺寸（逻辑像素，顶左原点）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowFrame {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// `data_dir/moe/windows.json`：窗口名 → 帧。
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

/// 读某个窗口的记忆帧；没有则 None（调用方走默认摆放）。
pub fn load_window_frame(name: &str) -> Option<WindowFrame> {
    load_frames().remove(name)
}

/// 记住某个窗口的位置与尺寸（拖拽/缩放后调用）。
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
                eprintln!("moe: 窗口状态保存失败: {err}");
            }
        }
        Err(err) => eprintln!("moe: 窗口状态序列化失败: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_frames_round_trip_in_memory() {
        // 直接验证 JSON 形状（落盘路径依赖 data_dir，不在此测）
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
