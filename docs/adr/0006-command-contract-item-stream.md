# Command 契约：输入种类 × Item 流输出，回写由平台统一执行

v1 冻结 Extension 暴露 Command 的最小契约。输入声明：None（直接执行）/ Query（依赖 Input Bar）/ Selection（呼出时自动抓取，缺失降级到光标模式）。输出不是一族封闭类型，而是"Item 流"：Command 的根动作产出 List\<Item\>、WriteBack(text)、OpenSideView 或 Silent 之一；每个 Item 自带主操作与具名副操作，且其动作可再次产出 Item 流或 WriteBack（词典范式：搜词→候选词 Item→Apply 出详情→⌥Enter 把释义回写）。由此"输出类型"不需穷举——可组合性代替了枚举。

## Selection 范围（v1）

v1 的 Selection 仅指文字；文件（如 Finder 选区）是契约后续的 payload 扩展（`Selection` 加枚举即可，不破坏任何 Command），不进 M2——M2 已是全项目风险最高的一块，不塞第二个集成面。

Selection 的抓取与 WriteBack 的交付（有选区替换、无选区插光标、剪贴板降级）一律由平台经 TextTarget 执行，Command 只产出文本、不感知平台手段。这保证键位语义（Enter=Apply、⌥Enter=第一副操作、⌘K=展开全部动作）跨 Extension 一致。

## Live 列表（IIE4AD-360 增补）

Command 可选声明 `live`（Live List，见 `CONTEXT.md`）：进入该 Command 的结果层后，Input Bar 的每次变化都用新查询重跑本 Command，而不是回到命令盘检索。这正是"词典范式"的交互面：进入子应用后输入即筛选列表。进入 Live 命令时输入栏清空（用于搜到它的文字不当作参数），重跑不计入 frecency。
