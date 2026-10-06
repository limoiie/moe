// 平台通用动作的 UI 侧镜像（ADR-0014/0022）。键位由 `moe-core::keymap` 定：
//   Browse ⌘P（记录列表）· Actions ⌘⇧P（动作清单）· New ⌘N（新建记录）
//   Delete ⌃X（删除当前记录）· DeleteAll ⌃⇧X（删除全部记录）
// 各表面（命令盘 / Side View / 将来的子应用）只做两件事：
//   1. 用 `generalActionOf(event)` 把键盘事件识别成语义；
//   2. 决定该语义在本表面的落点（面板：走 Extension 声明的入口/删除钩子；侧栏：本地视图）。

export type GeneralAction = "browse" | "actions" | "new" | "delete" | "deleteAll";

/** 需要 Extension 声明入口的三个通用动作（与 Rust `EntryKind` 一一对应）。 */
export type EntryAction = Exclude<GeneralAction, "actions" | "delete" | "deleteAll">;

/** 展示串（tooltip/提示用），与 Rust 键位表一致。 */
export const GENERAL_KEY_LABELS: Record<GeneralAction, string> = {
  browse: "⌘P",
  actions: "⌘⇧P",
  new: "⌘N",
  delete: "⌃X",
  deleteAll: "⌃⇧X",
};

/**
 * 识别通用动作；不是这几个键位则返回 null。
 * 删除槽只认 ⌃（不认 ⌘）；Browse/New 只认 ⌘（⌃P/⌃N 是 Navigation）。
 */
export function generalActionOf(e: KeyboardEvent): GeneralAction | null {
  const key = e.key.toLowerCase();
  // Delete / DeleteAll（ADR-0022）：⌃X / ⌃⇧X，不与 ⌘ 混
  if (e.ctrlKey && !e.metaKey && !e.altKey && key === "x") {
    return e.shiftKey ? "deleteAll" : "delete";
  }
  if (!e.metaKey || e.ctrlKey) return null;
  if (key === "p") return e.shiftKey ? "actions" : "browse";
  if (key === "n" && !e.shiftKey) return "new";
  return null;
}
