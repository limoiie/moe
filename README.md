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
cargo test                 # 契约测试
cd crates/moe-app && cargo tauri dev   # 面板开发运行（首次会编译 Tauri 全家桶）
```

## 当前状态：M1 骨架

- 呼出键暂时为 `⌥Space`（临时手段）；默认方案「双击 ⌘」需要 CGEventTap +
  辅助功能授权（ADR-0008），是 M1 的下一个 ticket。
- 搜索为朴素子串打分；fuzzy（nucleo）+ frecency 另行排期。
- WriteBack 目前只在面板内展示文本；真正的选区抓取/回写是 M2。

里程碑验收标准见 `docs/adr/0009-milestone-scope.md`。
