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
- 选区与回写（M2，IIE4AD-356）：呼出面板前抓取选区；`WriteBack` 经 AX 写入
  （有选区替换 / 无选区插光标），AX 被目标应用拒绝时自动降级「剪贴板快照 → 合成 ⌘V → 恢复」；
  演示命令 `Echo: Shout`。写回需要「辅助功能」授权（引导条第二档）。
- 搜索：nucleo 模糊匹配（**精确前缀 > 匹配位置**）+ **frecency 平分决胜**；
  命令用一次就更靠前，空查询也按 frecency 排序。使用记录在平台级 KV
  （`data_dir/moe/frecency.json`），不占扩展的 Namespace。
- AI 问答（M3a）：OpenAI 兼容端点**真实流式**（`[ai]` 的 base_url/model，即改即用无需重启）；
  key 走 keychain——面板输入 `key <你的key>` 回车保存（不回显），或设 `MOE_AI_API_KEY`；
  回答以 Markdown 详情卡片边流边渲染；无匹配时自动出现「AI: 提问「…」」捕获项。

里程碑验收标准见 `docs/adr/0009-milestone-scope.md`。
