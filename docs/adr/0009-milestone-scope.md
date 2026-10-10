# 里程碑边界：M1 面板活了 → M2 读写宿主 → M3 AI 形态完整 → M4 Linux 验证与打包

以演示为验收标准切分范围，并显式写下 v1 的"不做"：

- **M1**：菜单栏常驻，双击 ⌘ 弹出居中面板；≥2 个玩具 Extension（Echo、AI 壳）；模糊搜 Command；⌘N/⌘P、Enter、⌥Enter、⌘K、Esc 三段回退、Hints Bar——全程零鼠标。
- **M2**：macOS Selection 抓取 → WriteBack 三路（替换选区 / 插光标 / 剪贴板降级）演示全通；授权引导流程完成。
- **M3**：Quick Ask 流式走 Item 语义；⌘M Materialize → Side View；Conversation 存于 AI Namespace；搜历史 Command；附件（路径输入）；keychain 存 key。
- **M4**：Linux/X11 一等验证、Wayland 降级说明、macOS dmg + Linux deb/AppImage 打包。

显式排除（v1 不做）：设置 UI（用「Moe: 打开配置文件」Command 代替）、主题市场（默认深色 + 可跟系统）、Windows 实质实现（仅保留 cfg 桩）、运行时扩展加载、云同步账号。

## 增补：模型选择器（ADR-0040 / MOE-0028）

「v1 不做设置 UI」的范围收窄一个例外：**模型目录浏览器**（`ai.choose-model` / Set AI Model）允许在
面板内浏览 model catalog 的全部模型及其属性（上下文窗口、最大输出、模态、tools/vision/reasoning、
价格、文档链接），选中即写回 `[ai] provider` / `model`（注释保持的原地编辑，与主题切换同一机制）。
除此之外仍无设置 UI：key 继续用 `key <value>` 保存，provider 块继续手写 config.toml。理由：
catalog（MOE-0025）的事实只有被看见才有价值，而手抄 model id 与属性正是多 provider 工作要消除的
摩擦；选择器只读目录 + 单键切换，不承担管理职责，因此不构成通用设置面板的前例。
