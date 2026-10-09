# 侧栏是 Extension 声明的形态，不是全局的第二种窗口

AI 问答需要长对话 + 历史，直觉做法是给应用加一个全局"侧边栏模式"。我们决定不这样做：平台默认且只默认提供居中的 Command Panel；每个 Extension 可以声明自己的 Side View（常驻呈现形态），当前只有 AI 问答声明。会话可从 Command Panel 中"实体化"（Materialize）为 Side View。好处：形态归属清晰（侧栏的历史属于该 Extension 的 Namespace），平台一致性不破（Input Bar 与统一键位在两种形态中语义不变），未来任何 Extension 都可复用此声明。

## 落点（IIE4AD-360）

平台侧提供两块能力，形态仍由扩展声明：

- **开窗**：`ActionResult::OpenSideView { payload }` 由平台解释——收起面板，把 `chat` 窗口停到鼠标所在显示器右缘（同款 NSPanel：不激活应用、可进全屏 Space），把 payload 原样交给该窗口（AI 用 `{ conversationId }` 定位会话）。
- **续聊**：`Extension::side_continue(conversation_id, message, emitter) -> conversation_id`——空 id 表示新建会话；回复在后台流式产出，经 `CommandEvent` 增量送达（AI 的 `command_id` 约定为 `ai.side`，窗口按 payload 里的会话 id 过滤）。

平台不解释 payload，也不感知会话概念；扩展拥有会话与其历史（Namespace 隔离），平台只负责窗口与路由。

## 修正：侧栏的激活等级是动态的（MOE-0023）

「不激活应用」仍是默认：呼出侧栏（热键 / 胶囊命令 / tray）前把窗口恢复为 non-activating，才能浮在别的应用的全屏 Space 上、并直接取得 key 而不激活应用。

但 macOS 拒绝把 remap/合成点击（BetterTouchTool 的 Magic Mouse 轻拍）变成对 non-activating 面板的 key/focus 授予——点击送达到了，key 请求与显式 activate 都被拒（MOE-0023 的日志链路）。因此指针悬停在**未聚焦**的侧栏上时，窗口临时切回 activating（普通点击取焦路径，合成点击也能聚焦）；指针移出恢复 non-activating。已聚焦的窗口不动 mask（那正是打字的时候）；显示路径每次 reset。呼出面板保持始终 non-activating。
