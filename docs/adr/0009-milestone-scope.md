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

## 增补：设置窗口（MOE-0029）

「v1 不做设置 UI」再收窄一步：Moe 有了一个**设置窗口**（普通窗口，侧栏分节、每节一 pane），
AI 节管理本机在用的 provider——key（各自的 0600 key 文件）、模型在 picker 中的可见性
（`hidden_models`）、活跃 provider/model 对——以及 General 节的主题与配置文件入口。
理由：多 provider 工作已经把配置面推到 key 文件 + provider 块 + 目录事实三处，纯文件管理
正是 MOE-0027/0028 要消除的摩擦的最后一块；窗口只管理「已连接」的 provider（目录浏览仍归
MOE-0028 的 picker），不承担通用设置面板的职责。其余排除条款不变：provider 块的其余字段、
catalog 属性编辑、OAuth 仍留在配置文件与后续 issue。
