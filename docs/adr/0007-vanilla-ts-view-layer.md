# 视图层用原生 TS + Tailwind，拒绝 React/shadcn/TanStack

React 生态（shadcn/ui、TanStack）是桌面 Webview 应用的默认选项，我们刻意不用：状态主权在 Rust（ADR-0001），前端只渲染固定的视图词汇（Input Bar / Result List / Detail / Chat），由 IPC 事件驱动的微型 signal store（约 50 行）足够；Radix 式组件库的焦点管理与 keyboard-first 的全局键位系统（ADR-0006 的 Apply/副操作语义）冲突，需要的是直接的 window keydown 控制。代价是放弃组件库现成件、自绘列表与 chat 视图。若将来 UI 词汇表膨胀（如第三方扩展自带 UI），重新评估——渲染层按视图词汇隔离正是为这次重评留的门。

## Amendment: a renderer lives in a module, with its side effects injected (MOE-0052)

The cost above is 自绘列表与 chat 视图 — hand-drawn lists and chat views — and part of that bill came
due as **verification**: `main.ts` queries the DOM at module load, so no test can import it, and six
tickets in a row ended with "nothing has rendered on screen" because a renderer could only be checked by
hand.

The seam that fixes it, now a rule rather than a trick: **a renderer worth testing lives in its own
module and takes its side effects as arguments.** `metadata.ts` renders the detail pane's metadata
block against `MetadataDeps` — the page's dimension values, `onFilter`, `onAction`, `onOpenLink` — so a
jsdom test drives it directly, while `main.ts` passes the panel's real state in and owns nothing of the
block's internals. The pure-rule modules (`filter.ts`, `layers.ts`, `accessories.ts`, `dates.ts`) are
the other half of the same idea and stay dependency-free.

What it does not buy: jsdom lays nothing out, so row heights, column widths and the measured panel path
stay hand-checked. The rule is about element kind, class, order and dispatch — the parts that were
silently wrong before, not the parts that were merely unmeasured.
