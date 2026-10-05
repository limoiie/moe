# AI 只接 OpenAI 兼容端点，历史全本地，密钥进系统 keychain

不做各家 SDK 的多 provider 适配层：单一 base_url + model 名即可覆盖 DeepSeek/通义/OpenRouter/本地 llama.cpp，适配成本全部转嫁给用户配置。Conversation 历史只存本地 SQLite（在 AI 问答 Extension 自己的 Namespace 内），不做云同步账号。附件 v1 限文本文件与图片（本地读取，多模态走 base64）。API key 存系统 keychain，绝不入配置文件明文。
