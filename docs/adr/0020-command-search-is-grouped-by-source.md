# 命令盘结果按来源分组：每个 Extension 一个 Section

命令盘根页的结果原本是一张扁平列表（ADR-0003 的 `Registry::search` 返回 `Vec<CommandMeta>`）。
Command 变多以后（Moe 设置命令、AI 问答、Echo 演示……），用户看不出每条命令属于哪个子应用，
不符合 Raycast 的一致性：Raycast 根页的 section（Commands / Apps / Files……）本质是
**来源即分组**——`Files` 组来自文件搜索扩展，不是一条单独的分组规则。

## 决定

- `Registry::search` 返回 `Vec<CommandSection>`；契约新增 `CommandSection { title, items }`
  （serde camelCase），`moe-app` 的 `search_commands` IPC 同步改返回类型。
- **分组键 = `extension_id`**（唯一），组头显示名 = `ext.title()`。两个扩展即使同名也分两组。
- **组序 = 组内最优项的先后**：查询时即匹配分序（前缀加成 > 匹配分 > frecency，IIE4AD-346），
  空查询时即 frecency 序；组内保持原有得分顺序。分组只在排序后做一遍，不改排序契约。
- **fallback 命令落进它所属 Extension 的 section**（如 AI 的「提问「…」」落在 AI 组），不是特殊组。
- **UI 侧拍平渲染**：`sections` 只负责组头；拍平成 `commands` 继续承担焦点与键盘导航
  （focus 索引在拍平列表上）。每组前插一行**不可聚焦的组头**（小号大写灰字），
  不参与导航、不响应 hover、不偷焦点。
- **不做「文件搜索」section**：将来要搜文件就是加一个 File Extension，它的结果自然成为一个新来源组，
  不需要为它引入任何分组规则。这正是「来源即分组」的扩展性所在。

## 代价

- 单一扩展时也会显示组头（Raycast 亦如此），视觉上多一行；换来的是所有页面分组行为一致。
- UI 同时维护 `sections`（渲染）与 `commands`（导航）两份数据；将来若组头要可折叠或可聚焦，
  焦点模型需要升级（目前组头与焦点完全解耦，改动点是 `render()` 的拍平循环）。

## Amendment: the source order is declared, not derived

The empty query grouped sources by **first encounter in the frecency-sorted stream**, so a heavily
used command could reorder whole groups between sessions — the page's shape drifted with usage,
while the pinned Favorites/Suggestions sections already cover "most used on top". The order is now
**declared**: `Registry::set_source_order` takes extension ids, most prominent first, and
`install()` states the app's order — today **AI Commands → AI → Moe → Echo**. Unlisted sources rank
after the declared ones, keeping their relative order (stable sort); items inside a group keep
frecency order, and the searching page stays one scored "Results" section (ADR-0033).

Registration order is untouched and keeps its own job: **fallback capture precedence** (`key …`
must reach Moe's save item before AI's generic ask). Display order and capture precedence are now
separate decisions that happen to coincide today.
