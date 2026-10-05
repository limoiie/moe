# Moe

键盘优先的桌面命令盘：任意应用中双击 ⌘ 呼出居中面板，搜索 Command，
把结果回写到 Selection 或光标处。产品语言见 `CONTEXT.md`，全部设计决策
见 `docs/adr/0001–0009`。

## 结构

| 路径 | 职责 |
|---|---|
| `crates/moe-core` | Extension / Command / Item 契约（ADR-0006）、统一键位表、Registry 与搜索 |
| `crates/moe-platform` | `TextTarget`、呼出监听（ADR-0002/0008 的平台边界；目前为 trait + stub） |
| `crates/moe-extensions` | 内置 Extension（Echo 契约演示、AI 壳） |
| `crates/moe-app` | Tauri 2 壳：面板窗口与 IPC |
| `ui/` | 原生 TS + Tailwind 薄视图层（ADR-0007） |

## 开发

```sh
pnpm -C ui install
cargo test                                     # 契约测试
cd crates/moe-app && cargo tauri dev           # 面板开发运行（推荐：自动起 Vite + 热更新）
```

⚠️ 直接 `cargo run -p moe-app` 是 **dev 构建**：WebView 会去连 `devUrl`（http://localhost:1420），
没有 Vite dev server 时得到的是一个「全透明空窗口」（看起来像没启动）。可用跑法：

1. `cd crates/moe-app && cargo tauri dev`（推荐）
2. 两个终端：`pnpm -C ui dev` + `cargo run -p moe-app`
3. 嵌入产物：`cargo tauri build`（生产构建，经 `custom-protocol` 特性嵌入 `ui/dist`）

## 当前状态：M1 骨架

- 呼出键：**双击 ⌘**（ADR-0008）。macOS 首次运行会请求**「输入监控」授权**
  （listen-only 键盘 tap 的门槛；辅助功能留待 M2 回写类功能），未授权时面板内
  常显引导。已实现于 `moe-platform::mac`（listen-only CGEventTap + 可测的双击
  状态机）；启动日志会打印权限状态与 tap 挂载结果，方便排查。
- 面板在 macOS 上是 **NSPanel**（tauri-nspanel）：不激活应用、不抢菜单栏、
  可浮在全屏应用的 Space 之上；呼出时自动居中到鼠标所在显示器。
- 菜单栏常驻：tray 菜单（显示面板 / 打开配置文件 / 退出）；macOS 无 Dock 图标、
  不参与 ⌘-Tab；**面板失焦自动收起**。设置面是命令「Moe: 打开配置文件」（ADR-0009）。
- 面板隐藏/重现之间**保留输入与结果**；`Esc` 分层回退（详情→结果层→清空输入→关闭）依旧可用。
- `config.toml` 可改呼出键（macOS 为 `~/Library/Application Support/moe/config.toml`，
  Linux 为 `~/.config/moe/config.toml`）：

  ```toml
  [summon]
  key = "double-cmd"   # double-cmd | double-option | double-ctrl | 组合键如 cmd+shift+space
  double_tap_ms = 400  # 100..=1000
  ```

- Linux：双击监听未实现（X11 随 M4 Linux 验证落地；Wayland 无全局键盘拦截，
  只能 WM 绑定或改用组合键模式—组合键走 global-shortcut 插件，各平台可用）。
- WriteBack 目前只在面板内展示文本；真正的选区抓取/回写是 M2。
- 搜索为朴素子串打分；fuzzy（nucleo）+ frecency 另行排期（IIE4AD-346）。

里程碑验收标准见 `docs/adr/0009-milestone-scope.md`。
