# 通用删除槽：⌃X 删当前记录、⌃⇧X 删全部

ADR-0014 把 Browse / Actions / New 统一成跨 Command/子应用的通用动作。用户接着要
「AI 历史能删当前会话、删全部会话」，并要求**删除当前 / 删除全部也是平台级语义槽**——
任何 Command/子应用的列表视图共用同一套键位与路由，不按子应用记忆操作。

## 决定

- **键位表新增两个语义**：`Delete`（⌃X，删除当前记录）、`DeleteAll`（⌃⇧X，删除全部记录），
  与 Browse/New 同级进 `default_keymap`（语义唯一，Extension 不得覆写）。
- **Extension 钩子**：`delete_item(command_id, item)` / `delete_all(command_id)`，
  默认 `Err(NotFound)`（没记录可删的扩展不实现）；返回**实际删除条数**供 UI 反馈。
- **Registry 按命令所属 Extension 路由**（`delete_item` / `delete_all`，同 `run_item_action`）；
  IPC 同名两个端点。
- **AI 落地**：`ai.search-history` 的 `delete_item` 按 `item.payload.conversationId` 删会话
  （消息级联，DB `delete_conversation`）；`delete_all` 清空 `ai` Namespace
  （`delete_all_conversations`，不影响其它扩展）。其它命令 NotFound。
- **面板语义**：只在**结果层**拦截（命令层放行，⌃X 保留为输入框原生剪切）；
  删除成功后重跑当前列表（Live 用当前输入、非 Live 重 invoke），toast 反馈条数。
- **侧栏同一语义两个落点**：历史卡开着 → 作用于卡内焦点行 / 全部（删完刷新卡片）；
  关着 → 当前会话 / 全部（删掉当前会话后退回空态新对话）。
  走同一个 `delete_item` / `delete_all` IPC，「当前会话」用合成 Item（payload 带 id）表达，
  编辑器聚焦时放行原生剪切、不误删。
- **不做确认步**：⌃X/⌃⇧X 本身就是刻意的组合；删除即时生效 + toast。需要确认/回收站再议。

## 代价

- ⌃X 与 macOS 文本剪切共用键位：命令层/编辑器里放行剪切，只在列表上下文接管删除；
  面板结果层的输入框（Live 过滤）里无法用 ⌃X 剪切——过滤查询很短，可接受（Raycast 同取舍）。
- 删除不可撤销（无回收站）；`delete_all` 清空整个 Namespace 前没有任何二次确认。
- 侧栏的删除走 `ai.search-history` 命令路由：将来侧栏承载其它扩展的 Side View 时，
  需要把「当前记录」的合成 Item 换成对应扩展的命令 id（契约已就位）。
