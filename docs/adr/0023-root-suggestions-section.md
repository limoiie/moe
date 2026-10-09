# 空查询置顶「建议」：最近使用的命令

命令盘根页（空输入）原本只有「按扩展分组」的 section（ADR-0020）。Raycast 的空输入页
第一组是 **Suggestions**——最近用过的命令置顶，不用找也不用想。对齐它。

## 决定

- 空查询时，在其它 section **之前**加一个「建议」section：**最近使用的命令，最多 5 条，
  按最近使用时间倒序**；没有使用记录时整个 section 不出现（首启不显示空组）。
- 数据源是已有的 frecency（`Entry.last_used_unix` 早已持久化，无需迁移）：
  `FrecencyLookup` 新增 `last_used(id) -> Option<u64>`（默认 None，测试假实现不受影响）。
- 已进建议的命令**不再出现在后续分组里**（不重复）；其余命令照常按扩展分组、组序不变。
- 非空查询**没有**建议组：一打字就回到常规匹配（Raycast 同款）。
- 组头显示「建议」（与扩展名组头同一渲染路径，不可聚焦、不参与导航）；
  面板打开时焦点自然落在第一条建议上。
- UI 零改动：section 渲染（ADR-0020）直接复用，建议组就是第一个 section。

## 代价

- 「建议」与 frecency 分组共用同一份数据：频繁使用会同时抬高建议位次与搜索排序——
  这是一致的（常用即置顶），不是缺陷。
- 5 条上限是常量（`SUGGESTION_LIMIT`），将来要可配置再落 config。
- 空输入页不再显示「全部命令」：没进建议的命令仍可在后续分组里找到（只是被建议抢了置顶位）。

## Amendment: Suggestions rank by frecency, not raw recency

Pure recency proved unfair to consistently heavy use: a command used 1000 times lost the first slot
to a command used 10 times once, merely because the latter ran most recently — while the heavy
command was still in regular use. `docs/research/frecency-ranking.md` surveyed the field against
primary sources (zoxide's count × last-use buckets plus global aging, Firefox Places' frecency —
today a per-page exponential decay with `halfLifeDays 30` —, Redis's LFU dynamic aging, VS Code's
plain MRU, and the standard time-decayed counter). Conclusion: rank by **frecency with per-command
exponential decay**. Bucket × count can fossilize (a 1000-use command scores `1000 × 0.25 = 250`
forever) and zoxide's aging is a *global* renormalization that preserves that order; per-command
decay lets abandoned commands fade out of the list.

- `Frecency` stores a **decayed counter** instead of a lifetime count: every use does
  `score ← score·2^(−Δt / 30 d) + 1`, and `score()` applies the same decay at read time. A steady
  1-use/day command converges to ≈ 43 while a single fresh use is 1, so the reported case keeps the
  heavy command clearly ahead.
- A score below `0.1` reads as forgotten (it leaves Suggestions and the search tie-break) and is
  pruned lazily on the next `record` — Firefox deletes adaptive history below `0.975^90 ≈ 0.10`,
  and Redis ages on access the same way.
- The Suggestions section sorts by that score, highest first; ties break by most recent use, then
  id. The 5-row cap, the hidden-when-empty behavior, and the ⌃X / ⌃⇧X forget slots (ADR-0025) are
  unchanged. Search keeps breaking ties with the same score (ADR-0020 amendment) — one number,
  one meaning everywhere.
- Legacy files' lifetime `count` loads through a serde alias as the initial score; it decays away
  over ~10 half-lives. No migration step.
- Cost: a heavy command abandoned for months still ranks for a while (≈ 68 days to fall below the
  best case for a fresh 10-use command, ≈ 163 days below 1) — that fade is intended, and
  `HALF_LIFE_SECS` is the tuning knob if the palette should forget faster.
