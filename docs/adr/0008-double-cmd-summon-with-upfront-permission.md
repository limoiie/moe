# 唯一默认呼出键：双击 ⌘，接受首启即要权限

修饰键双击检测必须依赖键盘事件监听（CGEventTap / global monitor），macOS 上无论选 ⌘/⌥/⌃ 都需辅助功能或输入监控授权；免授权的组合键（RegisterEventHotKey）形态已被明确否决——产品只保留一个默认呼出键。决定：默认双击 ⌘（config.toml 可换 ⌥/⌃ 双击或组合键），首次启动即在面板内一次性引导授权，而非延迟到执行回写命令时；AI 问答类命令在未授权时照常可用。X11 下修饰键双击可免授权实现；Wayland 无全局键盘拦截协议，用户需在 WM 配置手动绑定呼出命令。

## Linux 落点（IIE4AD-350）

- **X11**：XRecord（`moe-platform::x11`）监听全局 KeyPress/KeyRelease，把 keycode 按键盘映射翻译回修饰键，再复用同一个 `DoubleTapDetector`；不需任何授权，`⌘` 在 Linux 键盘上的对应物是 Super。
- **Wayland**：XRecord 只能看到 XWayland 客户端，因此不启动监听，状态报 `Unsupported`，由面板引导两条替代路径：组合键（global-shortcut 插件）或 WM 绑定 `moe --toggle`（单实例插件把第二次启动转发为「切换面板」）。
- 降级顺序固定为：**WM 绑定 → 组合键 → tray 菜单**；三者都不依赖平台授权。
