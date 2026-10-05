# 视图层用原生 TS + Tailwind，拒绝 React/shadcn/TanStack

React 生态（shadcn/ui、TanStack）是桌面 Webview 应用的默认选项，我们刻意不用：状态主权在 Rust（ADR-0001），前端只渲染固定的视图词汇（Input Bar / Result List / Detail / Chat），由 IPC 事件驱动的微型 signal store（约 50 行）足够；Radix 式组件库的焦点管理与 keyboard-first 的全局键位系统（ADR-0006 的 Apply/副操作语义）冲突，需要的是直接的 window keydown 控制。代价是放弃组件库现成件、自绘列表与 chat 视图。若将来 UI 词汇表膨胀（如第三方扩展自带 UI），重新评估——渲染层按视图词汇隔离正是为这次重评留的门。
