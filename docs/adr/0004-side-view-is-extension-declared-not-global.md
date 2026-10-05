# 侧栏是 Extension 声明的形态，不是全局的第二种窗口

AI 问答需要长对话 + 历史，直觉做法是给应用加一个全局"侧边栏模式"。我们决定不这样做：平台默认且只默认提供居中的 Command Panel；每个 Extension 可以声明自己的 Side View（常驻呈现形态），当前只有 AI 问答声明。会话可从 Command Panel 中"实体化"（Materialize）为 Side View。好处：形态归属清晰（侧栏的历史属于该 Extension 的 Namespace），平台一致性不破（Input Bar 与统一键位在两种形态中语义不变），未来任何 Extension 都可复用此声明。
