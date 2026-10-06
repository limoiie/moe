//! 平台级统一键位表（Keymap）。Extension 不得覆写，只能为自己的动作附带快捷键。

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
    /// Show All Actions：展开 Focused Item 的全部动作。
    ShowAllActions,
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
        ("⌘K", K::ShowAllActions),
        ("Esc", K::Back),
        ("⌘M", K::Materialize),
        ("⌘⇧A", K::Attach),
    ]
}
