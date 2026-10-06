// 结果页面的**三种形态**（ADR-0018）：扩展只提供内容，形态、分栏比例与窗口尺寸
// 全部由这里统一决定——任何 Extension/子应用只要照常返回 Item（可选带 detail），
// 就自动获得与其他页面完全一致的版式与交互，不需要在视图层做任何定制。
//
//   列表（list）  ：单列结果，无详情栏                       → 面板默认尺寸
//   两栏（split） ：左侧列表 + 右侧详情（焦点项的 detail）   → 面板加宽加高
//   详情（detail）：唯一一条结果本身即内容（AI 回答、通知）  → 详情占满，面板默认尺寸
//
// 这就是 Raycast 的 List / List+Detail / Detail：形态由数据（有没有 detail、
// 是不是整屏）决定，不由各命令自己排版。

import type { Item } from "./types";

export type PageShape = "list" | "split" | "detail";

/** 面板尺寸（逻辑像素）：两栏页面需要更宽更高，列表/详情用默认尺寸。 */
export const PANEL_SIZE: Record<PageShape, { width: number; height: number }> = {
  list: { width: 750, height: 420 },
  detail: { width: 750, height: 420 },
  split: { width: 900, height: 540 },
};

/** 两栏页面里右侧详情栏的宽度（Raycast 同款：列表略宽于详情）。 */
export const SPLIT_PANE_CLASS = "w-[46%]";

/**
 * 当前应该用哪种形态渲染。
 * @param item      焦点项（list 形态下为 undefined）
 * @param detailFull 结果声明的「详情整屏」（ADR-0013，Extension 决定）
 * @param itemCount  结果条数（整屏只对唯一一条成立）
 */
export function pageShapeOf(
  item: Item | undefined,
  detailFull: boolean,
  itemCount: number,
): PageShape {
  if (!item) return "list";
  const hasDetail = item.detail != null || item.pending === true;
  if (!hasDetail) return "list";
  return detailFull && itemCount === 1 ? "detail" : "split";
}
