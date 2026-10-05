//! 平台级 KV：命令 frecency 的持久化。
//!
//! 命令使用记录不属于任何 Extension 的 Namespace（隔离域留给扩展自己的数据），
//! 因此落在平台自己的数据目录（IIE4AD-346）。

use std::path::PathBuf;

use moe_core::frecency::Frecency;

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
