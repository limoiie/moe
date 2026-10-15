# 唯一默认呼出键：双击 ⌘，接受首启即要权限

修饰键双击检测必须依赖键盘事件监听（CGEventTap / global monitor），macOS 上无论选 ⌘/⌥/⌃ 都需辅助功能或输入监控授权；免授权的组合键（RegisterEventHotKey）形态已被明确否决——产品只保留一个默认呼出键。决定：默认双击 ⌘（config.toml 可换 ⌥/⌃ 双击或组合键），首次启动即在面板内一次性引导授权，而非延迟到执行回写命令时；AI 问答类命令在未授权时照常可用。X11 下修饰键双击可免授权实现；Wayland 无全局键盘拦截协议，用户需在 WM 配置手动绑定呼出命令。

## Linux 落点（IIE4AD-350）

- **X11**：XRecord（`moe-platform::x11`）监听全局 KeyPress/KeyRelease，把 keycode 按键盘映射翻译回修饰键，再复用同一个 `DoubleTapDetector`；不需任何授权，`⌘` 在 Linux 键盘上的对应物是 Super。
- **Wayland**：XRecord 只能看到 XWayland 客户端，因此不启动监听，状态报 `Unsupported`，由面板引导两条替代路径：组合键（global-shortcut 插件）或 WM 绑定 `moe --toggle`（单实例插件把第二次启动转发为「切换面板」）。
- 降级顺序固定为：**WM 绑定 → 组合键 → tray 菜单**；三者都不依赖平台授权。

## 增补：组合键的引导与双击分离（MOE-0064）

把呼出键换成组合键后，面板仍会显示「… to summon is unavailable in this session（Wayland and
similar cannot globally intercept the keyboard）」的引导——那是把 `UnsupportedSummon` 误读成故障。
组合键由 global-shortcut 插件持有，`SummonListener` 本就不参与（`main.rs` 把 combo 映射为
`UnsupportedSummon`，这是设计而非降级），所以监听器的状态对「能不能呼出」没有发言权。增补：
`summon_status` 在呼出键是组合键时只报告插件注册的结果（成功 `ready`，失败 `unsupported`），
并新增 `combo` 字段；面板据此把「组合键注册失败」单独成文（另选一个组合键，或在 WM 里绑定
`moe --toggle`），不再复用面向双击的 Wayland 文案。降级顺序不变，仍为 WM 绑定 → 组合键 → tray。

macOS 有一个检测不到的盲区，记在这里：系统快捷键占用该组合键时（Spotlight 的 ⌘Space 即默认
一例），`RegisterEventHotKey` 仍返回 `noErr`——注册「成功」、按键却先被系统吃掉，后端无从判别，
只能由用户先在「系统设置 → 键盘 → 键盘快捷键… → Spotlight」里解除占用。
