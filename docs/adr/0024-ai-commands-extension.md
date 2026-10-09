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

## 增补：结果卡是决策点，不再自动回写（MOE-0007）

用户反馈：变换一跑完就自动替换选区、收起面板，看不到结果，也没机会再改。**自然完成不再回写**：
结果卡留在面板上，由用户在卡上决定——

- **应用** = 卡片的 primary 动作 `write-back`（⏎）：走 `ActionResult::WriteBack` → 平台
  `deliver_writeback`（写入 + 收起面板）。**复制** = ⌥⏎。**丢弃** = Back（Esc / 空 ⌫，ADR-0038），
  宿主文字原样不动——只有显式应用才会触碰宿主应用。
- **再改一次** = 新声明的 `rerun` 动作，标题「{命令} Again」（如「Make Shorter Again」）：把当前
  结果当作输入，用同一条命令再跑一遍，流式更新同一张卡。契约新增 `ActionResult::Rerun { text }`：
  扩展只表达意图，平台在 `run_item_action` 命令里拦截并代为调用（以 `text` 为输入、不带呼出时抓的
  Selection、不记 frecency），把那次调用的结果原样交回 UI——扩展不需要在 `run_item_action` 里拿到
  emitter，也不复制一遍配置/密钥管线。流式进行中点 rerun 是空操作：一张卡同时只有一条流。
- `CommandEvent::WriteBack` 保留在契约里（平台的拦截路径不变，别的扩展要自动回写仍可用），
  AiCommands 不再发出它；阻塞 `invoke` 路径同样返回评审卡而不是直接回写。

## 增补：思考块不算回答（MOE-0008）

有些模型（如 minimax-M3）把思维链直接流进正文（`…`…`…` 或 `…`…`…`，有的服务端还会把开标签
吞掉只留闭标签），旧实现把它当成结果：卡片正文、复制、回写里全是思考。现在 `ai::run_stream` 把
每一帧拆成 (reasoning, answer)：**回答才是正文**——流式、落库、复制、回写都只用它；**思考只进条目的
payload**（`reasoning` 字段），UI 在答案上方渲染成 ChatGPT 式的折叠行（生成中标签为带微光动画的
「Thinking」，完成后换成「Thought for N seconds」；默认折叠、不自动展开）。拆分规则：同分隔符样式
（`…`/`…`）只在文本**最开头**生效（正文里的
省略号是普通文字）；开标签缺失时，闭标签之前都算思考；未闭合的块视为「还在思考」（此期间回答为空，
指示器继续跑）；被停止时思考保留，正文是 "(generation stopped)"。聊天历史只存回答——思考是实时观感，
不是会话内容。

## 增补：包装标签按形状匹配，不按字面（MOE-0011）

精确字符串匹配（`<thinking>`/`</thinking>`）对 provider/模板细节太脆弱：MiniMax 一脉的包装会带
内部空格、大小写不同，或**开闭名字不同**（如 `<thinking>` 配 `</response>`）。改为按形状扫描
tag-like token：`<` [`/`] 名字 [空白] `>`，名字为 ASCII 字母、不接受属性；开标签名字
{thinking, think, reasoning} 在任意位置有效，{response} 只在消息最开头有效；闭标签名字上述四者皆可，
任意开标签可被任意闭标签收尾；落单的闭标签只在非歧义名字（thinking/think/reasoning）时视为
「开标签丢失」——正文里的 `<response>…</response>` 之类 XML 内容不受影响。另加一条 stderr 诊断：
完成后若 reasoning/answer 里仍残留 tag-like token，就以 `{:?}`（转义字节）打印一次——下次遇到新包装，
日志一行就能定位，不再靠截图猜测。

## 增补：Thinking 行采用 ChatGPT 的视觉语言（MOE-0012）

调研见 `docs/research/chatgpt-thinking-block.md`（一手来源：OpenAI o1/o3/GPT-5 公告原文里展示的
演示文本，以及平台「Reasoning models」文档；纯视觉细节在笔记中标为观察/推断）。落到实现：

- **标签**：生成中「Thinking」（灰度渐变微光；`prefers-reduced-motion` 下不出现动画）；结束后
  「Thought for N seconds」（不足一秒为 "a few seconds"，超过一分钟缩写为 `1m 19s`）。计时从首帧
  思考到收尾帧；文案形状是已验证的 ChatGPT 形状（"Thought for 5 seconds"/"Thought for 1m 19s"）。
- **默认折叠、不自动展开也不自动收起**：整行是点击目标；chevron 在行尾，展开时右→下旋转。结果卡
  每帧重建，因此跨帧保留用户的手动展开状态（rerun 时计时重新起算）。
- **视觉**：无边框无底色，`text-xs` 弱化灰（hover 整行浅底）；内容按 Markdown 渲染（与 ChatGPT 的
  摘要一致）；内联展开、无内部滚动。
- **无障碍**：保留原生 `<details>/<summary>`——调研笔记建议的 button + `aria-expanded` 本身是推断性
  建议，原生语义已覆盖同样的能力，不值得为此手写开关。
