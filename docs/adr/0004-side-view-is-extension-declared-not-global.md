# 侧栏是 Extension 声明的形态，不是全局的第二种窗口

AI 问答需要长对话 + 历史，直觉做法是给应用加一个全局"侧边栏模式"。我们决定不这样做：平台默认且只默认提供居中的 Command Panel；每个 Extension 可以声明自己的 Side View（常驻呈现形态），当前只有 AI 问答声明。会话可从 Command Panel 中"实体化"（Materialize）为 Side View。好处：形态归属清晰（侧栏的历史属于该 Extension 的 Namespace），平台一致性不破（Input Bar 与统一键位在两种形态中语义不变），未来任何 Extension 都可复用此声明。

## 落点（IIE4AD-360）

平台侧提供两块能力，形态仍由扩展声明：

- **开窗**：`ActionResult::OpenSideView { payload }` 由平台解释——收起面板，把 `chat` 窗口停到鼠标所在显示器右缘（同款 NSPanel：不激活应用、可进全屏 Space），把 payload 原样交给该窗口（AI 用 `{ conversationId }` 定位会话）。
- **续聊**：`Extension::side_continue(conversation_id, message, emitter) -> conversation_id`——空 id 表示新建会话；回复在后台流式产出，经 `CommandEvent` 增量送达（AI 的 `command_id` 约定为 `ai.side`，窗口按 payload 里的会话 id 过滤）。

平台不解释 payload，也不感知会话概念；扩展拥有会话与其历史（Namespace 隔离），平台只负责窗口与路由。
