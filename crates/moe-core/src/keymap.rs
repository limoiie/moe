//! 平台级统一键位表（Keymap）。Extension 不得覆写，只能为自己的动作附带快捷键。
//!
//! 通用动作跨所有 Command / 子应用共用同一套键位语义（ADR-0014/0022）：
//! Browse（⌘P，记录列表）、Actions（⌘⇧P，动作清单）、New（⌘N，新建记录）、
//! Delete（⌃X，删除当前记录）、DeleteAll（⌃⇧X，删除全部记录）。
//! 平台只定键位与路由，具体入口由 Extension 声明。

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SystemKey {
    NavDown,
    NavUp,
    /// Apply：Focused Item 的主操作。
    Apply,
    /// 第一副操作（默认语义：复制纯文本）。
    SecondaryCopy,
    /// Show All Actions：展开当前上下文的主/副操作清单（面板 ⌘K，子应用 ⌘⇧P）。
    ShowAllActions,
    /// Browse：打开当前 Extension 的记录列表（AI = 历史会话）。通用动作，默认 ⌘P。
    Browse,
    /// New：新建一条记录（AI = 新会话）。通用动作，默认 ⌘N。
    New,
    /// Delete：删除当前记录（AI = 当前会话）。通用动作，默认 ⌃X（ADR-0022）。
    Delete,
    /// DeleteAll：删除全部记录（AI = 全部会话）。通用动作，默认 ⌃⇧X（ADR-0022）。
    DeleteAll,
    /// Esc：分层回退（有输入→清空；有结果层→回上层；否则关面板）。
    Back,
    /// Materialize：转为该 Extension 的 Side View。
    Materialize,
    /// Attach：为该次提问添加附件（AI 语义：`@path` mention，ADR-0010）。
    Attach,
}

/// (展示串, 语义)。UI 与键盘事件按语义绑定，展示串进 Hints Bar。
pub fn default_keymap() -> Vec<(&'static str, SystemKey)> {
    use SystemKey as K;
    vec![
        ("↓ / ⌃N", K::NavDown),
        ("↑ / ⌃P", K::NavUp),
        ("⏎", K::Apply),
        ("⌥⏎", K::SecondaryCopy),
        // 同一语义两个键位：⌘K 是命令盘惯例，⌘⇧P 是子应用通用键（ADR-0014）
        ("⌘K / ⌘⇧P", K::ShowAllActions),
        ("⌘P", K::Browse),
        ("⌘N", K::New),
        ("⌃X", K::Delete),
        ("⌃⇧X", K::DeleteAll),
        ("Esc", K::Back),
        ("⌘M", K::Materialize),
        ("⌘⇧A", K::Attach),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个语义在键位表里只出现一次（展示串可含多个键位，如 "⌘K / ⌘⇧P"）。
    #[test]
    fn every_semantic_has_exactly_one_row() {
        let table = default_keymap();
        let mut seen: Vec<SystemKey> = Vec::new();
        for (display, key) in &table {
            assert!(!display.trim().is_empty(), "{key:?} 缺展示串");
            assert!(
                !seen.contains(key),
                "{key:?} 出现多次：语义应唯一，键位可多个",
            );
            seen.push(*key);
        }
        assert_eq!(seen.len(), 12, "新增语义时同步 Hints Bar 的分组");
    }

    /// 通用动作的默认键位是跨 Command/子应用的契约（ADR-0014/0022）。
    #[test]
    fn general_actions_keep_their_default_bindings() {
        let table = default_keymap();
        let display = |key: SystemKey| {
            table
                .iter()
                .find(|(_, k)| *k == key)
                .map(|(d, _)| *d)
                .unwrap_or_else(|| panic!("{key:?} 未登记"))
        };
        assert_eq!(display(SystemKey::Browse), "⌘P");
        assert_eq!(display(SystemKey::New), "⌘N");
        assert_eq!(display(SystemKey::ShowAllActions), "⌘K / ⌘⇧P");
        assert_eq!(display(SystemKey::Delete), "⌃X");
        assert_eq!(display(SystemKey::DeleteAll), "⌃⇧X");
    }
}
