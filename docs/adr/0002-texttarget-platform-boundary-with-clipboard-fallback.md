# 与宿主应用的文字互动统一走 TextTarget 边界，剪贴板粘贴为全平台降级

"读选区/写回文本"是整个产品的技术风险最高点，且各平台能力悬殊：macOS 有 Accessibility API（需用户授权），X11 有成熟方案，Wayland 沙箱下读不到任何应用的选区，Windows 走 UIA。决定抽象 TextTarget 边界（读选区、替换、插光标），macOS 主路径用 AX API，所有平台保留统一的降级路径：暂存剪贴板 → 写入结果 → 模拟粘贴 → 恢复剪贴板。Linux 首版只承诺 X11 + Wayland 降级；Windows 后置。Command 层不感知具体平台手段。

## 增补：剪贴板也是平台能力（IIE4AD-364）

"复制"类动作（SecondaryCopy 的默认语义）与 WriteBack 是两件事：前者只写系统剪贴板、**不动宿主应用**，后者才走 TextTarget 回写。因此平台层提供独立的 `clipboard::copy`（macOS 复用回写降级的同一 NSPasteboard 实现）；扩展返回 `ActionResult::Silent`，由面板自己给一次轻量 toast 作为反馈。这样剪贴板能力不污染 Command 契约，也不需要新枚举变体。

## 增补：选区也是 AI 提问的上下文（IIE4AD-37x）

呼出面板前抓到的 Selection 不只用于回写，也作为提问上下文：AI 扩展把选区拼进发给模型的提示
（问题在前、选区以代码块附后；问题里已含这段文字则不重复；超长截断并注明），历史里仍只存问题本身。
平台只负责把选区递给扩展（`invoke`/`fallback_command` 的 `selection` 参数），拼不拼由扩展决定。
