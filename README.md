# Moe

键盘优先的桌面命令盘：任意应用中双击 ⌘ 呼出居中面板，搜索 Command，
把结果回写到 Selection 或光标处。产品语言见 `CONTEXT.md`，全部设计决策
见 `docs/adr/0001–0010`。

## 安装（macOS）

1. 从 GitHub Actions 的 **Release** 工作流下载 `Moe_<版本>_<arch>.dmg`
   （Actions → Release → 对应 run → Artifacts；推 `v*` 标签或手动触发）。
2. 打开 dmg，把 **Moe.app** 拖进「应用程序」。
3. **未签名/未公证**：首次启动请右键点 Moe.app →「打开」→ 再点「打开」
   （之后可正常双击；这是 Gatekeeper 对未公证应用的一次性确认）。
4. 首次运行：菜单栏出现 Moe 图标；面板内会引导授予**「输入监控」**（双击 ⌘ 的门槛）
   与**「辅助功能」**（抓选区/回写文本的门槛）。授权后无需重启即生效。

## 使用

双击 ⌘ 呼出面板，输入即搜 Command；一套键位语义贯通所有 Extension：

| 键位 | 语义 | 说明 |
|---|---|---|
| `↓` / `⌃N`、`↑` / `⌃P` | 导航 | 移动 Focused Item |
| `⏎` | Apply | 对 Focused Item 执行主操作 |
| `⌥⏎` | 副操作 | 默认语义：复制（如复制 AI 回答全文到剪贴板） |
| `⌘K` | 展开全部动作 | 当前 Item 的主/副操作清单 |
| `⌘M` | Materialize | 把当前会话转入该 Extension 的 Side View |
| `⌘⇧A` | 附件 | 输入/粘贴文件路径，插入 `@"path"`（ADR-0010） |
| `Esc` | 分层回退 | 生成中→停止；否则 预览 → 结果层 → 清空输入 → 关闭面板 |

- **面板**：失焦自动收起；隐藏/重现之间保留输入与结果。菜单栏图标提供
  显示面板 / AI 对话 / 开机自启 / 打开配置文件 / 退出。
- **AI 问答**：直接输入问题回车（无匹配时自动出现「AI: 提问「…」」），回答流式渲染为
  Markdown 卡片；`⌥⏎` 复制全文，`⌘M` 转入右侧栏续聊。
- **侧栏（AI 对话）**：右侧常驻窗口（可与全屏应用共存），历史自上而下、输入框在底部，
  `⏎` 发送 / `⇧⏎` 换行 / `Esc` 收起；📎 添加附件；`＋` 开新对话；
  历史会话从命令「AI: 搜索历史会话」找回（输入即筛标题）。
- **配置**：命令「Moe: 打开配置文件」或直接编辑
  `~/Library/Application Support/moe/config.toml`：

  ```toml
  [summon]
  key = "double-cmd"   # double-cmd | double-option | double-ctrl | 组合键如 cmd+shift+space
  double_tap_ms = 400  # 100..=1000

  [ai]
  base_url = "https://api.deepseek.com/v1"  # 任意 OpenAI 兼容端点
  model = "deepseek-chat"
  ```

  API key 存系统 keychain：面板输入 `key <你的key>` 回车（不回显），或设 `MOE_AI_API_KEY`。

## 结构

| 路径 | 职责 |
|---|---|
| `crates/moe-core` | Extension / Command / Item 契约（ADR-0006）、统一键位表、Registry 与搜索 |
| `crates/moe-platform` | 平台边界（ADR-0002/0008）：TextTarget、剪贴板、呼出监听（macOS CGEventTap / Linux X11 XRecord） |
| `crates/moe-extensions` | 内置 Extension（Moe 设置命令、Echo 契约演示、AI 问答） |
| `crates/moe-app` | Tauri 2 壳：面板/侧栏窗口、tray、IPC |
| `ui/` | 原生 TS + Tailwind 薄视图层（ADR-0007） |

## 开发

```sh
pnpm -C ui install
cargo test                                     # 契约与平台测试
cd crates/moe-app && cargo tauri dev           # 面板开发运行（推荐：自动起 Vite + 热更新）
```

⚠️ 直接 `cargo run -p moe-app` 是 **dev 构建**：WebView 会去连 `devUrl`（http://localhost:1420），
没有 Vite dev server 时得到的是一个「全透明空窗口」（看起来像没启动）。可用跑法：

1. `cd crates/moe-app && cargo tauri dev`（推荐）
2. 两个终端：`pnpm -C ui dev` + `cargo run -p moe-app`
3. 生产构建：`cd crates/moe-app && cargo tauri build`（嵌入 `ui/dist`，产出 .app/.dmg）

图标源图：`python3 crates/moe-app/icons/gen-app-icon.py`（应用图标，再跑 `cargo tauri icon`）
与 `gen-tray-icon.py`（菜单栏模板图）。

## 功能现状

- **呼出**：双击 ⌘（macOS CGEventTap，需输入监控授权；Linux/X11 用 XRecord 免授权）。
- **文本**：呼出前抓取 Selection；`WriteBack` 经 AX 替换选区/插光标，被拒时降级
  「剪贴板快照 → 合成 ⌘V → 恢复」（演示命令 `Echo: Shout`）。
- **搜索**：nucleo 模糊匹配（精确前缀 > 匹配位置）+ frecency 平分决胜；空查询按 frecency 排序。
- **AI**：OpenAI 兼容端点流式问答；会话与消息存本地 SQLite（`data_dir/moe/moe.db`，`ai` Namespace）；
  附件（文本内联 / 图片多模态）；侧栏续聊把整段历史作为上下文。
- **常驻**：菜单栏 tray（含开机自启，LaunchAgent）；macOS 无 Dock 图标、不参与 ⌘-Tab。

## 常见问题

- **双击 ⌘ 没反应**：看面板顶部引导条——多半是「输入监控」未授权（系统设置 → 隐私与安全性）。
  个别系统版本授权后需重启 Moe 一次。
- **回写没生效 / 提示权限**：需要「辅助功能」授权；部分应用（如某些 Electron 应用）AX 只读，
  会自动走剪贴板降级。
- **首次打开提示「无法验证开发者」**：未签名应用，右键 →「打开」即可（见「安装」）。
- **Wayland**：无法全局拦截键盘（XRecord 只能看到 XWayland 客户端）。两条替代路径：
  1）`config.toml` 改用组合键（`[summon] key = "cmd+shift+space"`）；
  2）窗口管理器绑定 `moe --toggle`（单实例转发：已在运行则切换面板，未运行则启动并亮面板）：

  ```conf
  # Hyprland (~/.config/hypr/hyprland.conf)
  bind = SUPER, M, exec, moe --toggle
  ```

  ```conf
  # i3 (~/.config/i3/config) / sway (~/.config/sway/config)
  bindsym $mod+m exec moe --toggle
  ```

- **AI 报错**：回答卡片里会显示端点返回；确认 `[ai] base_url/model`、key 已保存、
  端点支持所选模型（图片需要 vision 能力）。

里程碑验收标准见 `docs/adr/0009-milestone-scope.md`。
