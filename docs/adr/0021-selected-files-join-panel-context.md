# 选中文件作为面板上下文：Finder 起步（Selection 扩展为文字 + 文件）

原始需求是「选中**文字或文件**后呼出面板」。文字选区早已落地（ADR-0002/0019），
文件选择一直缺位。Raycast 里选中文件后呼出，会拿到文件列表作为上下文；
本 ADR 把同一语义接进来，先覆盖最常见的来源——Finder。

## 决定

- **契约新增 `Selection { text, files }`**（serde camelCase），替换原先在
  `invoke` / `invoke_streaming` / `fallback_command` 之间传递的裸 `Option<&str>`：
  面板上下文从「文字选区」升级为「文字选区 + 选中文件」，两者可以同时存在。
  文件是绝对路径（POSIX）列表。
- **抓取范围只到 Finder**：前台应用是 `com.apple.finder` 时，用 AppleScript
  （`/usr/bin/osascript`）读 `selection` 的 POSIX 路径；其它应用没有统一的
  文件选择 AX 协议，逐个应用扩（下一步候选：访达外的常见「文件窗口」）。
- **best-effort、绝不拖呼出**：osascript 限时 2.5s（TCC「自动化」授权弹窗期间
  它会挂起等待应答），超时杀掉、本次当作没有文件；失败/被拒一律静默降级并日志一条。
  输出用 **linefeed 分隔**（路径可能含逗号，换行不会），`parse_finder_paths` 逐行解析。
- **落地到 AI 附件**：`ai.quick-ask` 把选中文件与 `@path` mention 合并成
  `AttachmentRef`（按路径去重）；fallback 副标题注明「已附上 N 个附件」，
  与「选中文字作为上下文」可以叠加。纯文件选择 + 空问题时不直接发请求，
  提示「写下问题（已附上选中文件）」。
- **Linux 恒为空**：`files::finder_selection()` 在非 macOS 返回空 Vec，
  `Selection.files` 由呼出侧装配，Linux 构建不受影响。

## 代价

- 首次在 Finder 选中文件呼出会弹一次 TCC「自动化」授权（moe 想控制 Finder）；
  拒绝后功能静默失效（日志可见），不影响其余呼出。
- AppleScript 每次呼出 ~100–300ms（仅 Finder 前台时触发），已在限时内消化。
- 只有 Finder 有文件上下文；其它应用的「选中文件」仍未覆盖（契约已就位，只需加采集端）。
