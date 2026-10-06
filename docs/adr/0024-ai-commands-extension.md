# AI Commands 扩展：批量文本变换命令 + 流式完成自动回写

Raycast Pro 的 AI Commands（Improve Writing / Make Shorter / Translate…）是
「选中文字 → 一条命令 → 结果自动替换选区」。把它作为**新的内置 Extension** 复刻进来，
同时把聊天问答与单次变换共用的一套流式管线抽成共享件。

## 决定

- **新扩展 `AiCommands`**（id `ai-commands`，标题「AI Commands」）：12 条命令
  （润色 / 修语法 / 缩短 / 扩写 / 简化 / 总结 / 翻译成英文 / 翻译成中文 /
  语气更专业 / 语气更友好 / 提取要点 / 续写），每条 = 固定系统指令 + 输入文本，
  「只输出处理后的文本」写进指令，避免回写被解释性前言污染。
- **输入 = 选区优先**，没有选区用输入框文字；两者皆空给引导卡。单次变换**不落库、不建会话**。
- **自动回写**：流式完成（自然结束）后发出新事件 `CommandEvent::WriteBack { text }`，
  平台层（moe-app 的 emitter）拦截执行回写 + 收起面板（`deliver_writeback` 已有此语义），
  **不转发给 UI**。被 Esc 停止时**不**自动回写——用户可 ⏎ 手动回写、⌥⏎ 复制。
- **共享管线重构**：`ai::run_stream` 从聊天专用泛化为 `StreamRequest { base_url, key, body,
  command_id, stream_key, item_of, persist, on_done }`——帧构造、落库、完成回调三处注入。
  聊天流（会话落库 + 回答卡）与 AI 命令流（noop 落库 + 结果卡 + WriteBack 回调）共用
  同一套 HTTP/SSE/取消机制；取消登记键在命令流用每次运行唯一键（无会话可挂靠，Esc 仍可停止）。
- **invoke（阻塞）路径**：`stream: false` 单发请求（`chat_body_with_stream` 新开关），
  解析 `choices[0].message.content` 后走 `ActionResult::WriteBack`；面板实际走 invoke_streaming。
- **图标**：UI 注册 10 个 Lucide 图标（wand-2 / spell-check / shrink / expand / pen-line /
  list-checks / languages / briefcase / smile / lightbulb / arrow-right）。

## 代价

- 12 条命令全部注册进命令盘：空输入页会多出一个「AI Commands」section（按来源分组，ADR-0020）。
- 自动回写依赖「辅助功能」授权（与现有回写同一门槛）；选区在生成期间被用户改动时回写落点以当时为准。
- AI 命令不读附件/文件（v1 只处理文字）；将来「总结这个文件」可复用同一管线加附件展开。
- `CommandEvent` 多了一个变体：所有 match 处需显式覆盖（编译器兜底）。
