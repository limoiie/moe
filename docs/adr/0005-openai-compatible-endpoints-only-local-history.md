# AI 只接 OpenAI 兼容端点，历史全本地，密钥进系统 keychain

不做各家 SDK 的多 provider 适配层：单一 base_url + model 名即可覆盖 DeepSeek/通义/OpenRouter/本地 llama.cpp，适配成本全部转嫁给用户配置。Conversation 历史只存本地 SQLite（在 AI 问答 Extension 自己的 Namespace 内），不做云同步账号。附件 v1 限文本文件与图片（本地读取，多模态走 base64）。API key 存系统 keychain，绝不入配置文件明文。

## 增补：工具回路放宽了网络边界，「只存回答」被修订（ADR-0039 / MOE-0026）

「只接 OpenAI 兼容端点」约束的是 LLM 推理请求本身，此条不变。Agentic Conversation 额外触达两类
网络目标：用户自建的 SearXNG 实例（`[ai] searxng_url`，无密钥；未配置则 web_search 工具根本不出
现在请求里），以及 web_fetch 抓取的目标 URL（本地 GET + 正文提取；失败时可选回退 r.jina.ai，
`[ai] fetch_fallback` 可关）。历史仍然全本地，但「只存回答」对 Agentic Conversation 修订为
**忠实全量、只追加落库**（问题、回答、reasoning、工具轮次），且每次请求全量重放、附带 `tools`
并回传每条 assistant 消息的 `reasoning_content`——为的是命中 DeepSeek 的前缀缓存（hit 价为 miss
的 1/50；塌缩重写的尾巴反而每轮按 miss 计费，还会踩跨轮 reasoning 回传的 400）。功能上线前的
旧会话永远保持 plain（不带 tools，无 400 风险）。密钥仍只有 AI key 一把，照旧进 keychain。
依据见 `docs/research/deepseek-context-caching-and-dsh.md` 与 `docs/research/free-web-search-options.md`。

## 增补：协议面扩为三种，provider 可配（ADR-0040 / MOE-0027）

「只接 OpenAI 兼容端点」被 ADR-0040 取代：LLM 推理请求扩为三种协议——OpenAI Chat Completions、
OpenAI Responses、Anthropic Messages——官方 provider 默认值取自 model catalog，自定义 provider 走
`[ai.providers.*]` 配置；密钥随之按 provider 分文件存储（ADR-0040 修订 ADR-0019；旧 `api-key` 作为
隐式 default provider 的密钥继续有效）。本地历史（本条第二句）、ADR-0039 的工具回路与网络边界、
以及 Agentic Conversation 的忠实落库决定均不变；工具回路继续以 OpenAI Chat 为首发协议（其余协议
暂为 plain 会话）。
