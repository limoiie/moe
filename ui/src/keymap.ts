// 平台通用动作的 UI 侧镜像（ADR-0014）。键位由 `moe-core::keymap` 定：
//   Browse ⌘P（记录列表）· Actions ⌘⇧P（动作清单）· New ⌘N（新建记录）
// 各表面（命令盘 / Side View / 将来的子应用）只做两件事：
//   1. 用 `generalActionOf(event)` 把键盘事件识别成语义；
//   2. 决定该语义在本表面的落点（面板：走 Extension 声明的入口；侧栏：本地视图）。

export type GeneralAction = "browse" | "actions" | "new";

/** 需要 Extension 声明入口的两个通用动作（与 Rust `EntryKind` 一一对应）。 */
export type EntryAction = Exclude<GeneralAction, "actions">;

/** 展示串（tooltip/提示用），与 Rust 键位表一致。 */
export const GENERAL_KEY_LABELS: Record<GeneralAction, string> = {
  browse: "⌘P",
  actions: "⌘⇧P",
  new: "⌘N",
};

/**
 * 识别通用动作；不是这三个键位则返回 null。
 * 只认 ⌘（不认 ⌃）：⌃P/⌃N 是 Navigation，不能与 Browse/New 抢键位。
 */
export function generalActionOf(e: KeyboardEvent): GeneralAction | null {
  if (!e.metaKey || e.ctrlKey) return null;
  const key = e.key.toLowerCase();
  if (key === "p") return e.shiftKey ? "actions" : "browse";
  if (key === "n" && !e.shiftKey) return "new";
  return null;
}
