# 与宿主应用的文字互动统一走 TextTarget 边界，剪贴板粘贴为全平台降级

"读选区/写回文本"是整个产品的技术风险最高点，且各平台能力悬殊：macOS 有 Accessibility API（需用户授权），X11 有成熟方案，Wayland 沙箱下读不到任何应用的选区，Windows 走 UIA。决定抽象 TextTarget 边界（读选区、替换、插光标），macOS 主路径用 AX API，所有平台保留统一的降级路径：暂存剪贴板 → 写入结果 → 模拟粘贴 → 恢复剪贴板。Linux 首版只承诺 X11 + Wayland 降级；Windows 后置。Command 层不感知具体平台手段。
