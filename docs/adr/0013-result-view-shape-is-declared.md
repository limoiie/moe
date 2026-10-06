# 结果视图形态由 Extension 声明，UI 不按条数猜

结果层有两种形态：**左列表 + 右详情**（历史搜索、动作清单）与**详情整屏**（AI 回答、系统通知——唯一一条结果本身即内容）。前者是多数 Command 的形态，后者只适合"结果即正文"的 Command。

曾用「`items.len() == 1` 就整屏」这条 UI 启发式代替声明，两个方向都会出错：AI 历史搜索命中唯一一条会话时列表被整屏顶掉（用户看不到那一行，也无法确认自己筛到了什么）；而将来任何返回单条结果的列表型 Command 都会同样被误判。条数是数据，形态是意图，两者不该混同。

## 决定

`ActionResult::List` 增加 `detail_full: bool`（serde `detailFull`，`#[serde(default)]` 缺省 false），并提供两个构造器表达意图：

- `ActionResult::list(items)`：左列表 + 右详情；
- `ActionResult::detail(items)`：唯一一条结果的详情占满面板，生成中在正文末尾追加行内指示。

UI 只读 `detailFull`（并仍要求 `items.len() == 1` 作为防御），不再看条数。AI 的 `quick-ask` / `set-ai-key` 等"结果即内容"的 Command 用 `detail`，`search-history`、`echo.items` 等用 `list`。

## 代价

- 每个 `ActionResult::List` 构造点都要显式表态（用构造器后是一行）。
- 形态与单个 Item 的 `detail` 字段正交，读代码时要区分「有没有详情」与「详情是否整屏」。
- 与 Raycast 的 `List` / `Detail` 对应；跨 Extension 的一致性由平台的这一字段保证，而不是各 Extension 给 UI 提示。
