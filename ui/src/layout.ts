// 结果页面的**三种形态**（ADR-0018）：扩展只提供内容，形态、分栏比例与窗口尺寸
// 全部由这里统一决定——任何 Extension/子应用只要照常返回 Item（可选带 detail），
// 就自动获得与其他页面完全一致的版式与交互，不需要在视图层做任何定制。
//
//   列表（list）  ：单列结果，无详情栏                       → 默认高度
//   两栏（split） ：左侧列表 + 右侧详情（焦点项的 detail）   → 更高（两栏都读得下）
//   详情（detail）：唯一一条结果本身即内容（AI 回答、通知）  → 详情占满，默认高度
//
// 这就是 Raycast 的 List / List+Detail / Detail：形态由数据（有没有 detail、
// 是不是整屏）决定，不由各命令自己排版。

import type { Item } from "./types";

export type PageShape = "list" | "split" | "detail";

/** 面板宽度（逻辑像素）：**所有形态一致**（Raycast 同款：宽度恒定，高度随内容）。
 * 根页面与两栏页面同宽，切换形态时宽度不跳。 */
export const PANEL_WIDTH = 900;

/** 面板高度（逻辑像素）：只有两栏页面更高——列表与详情都读得下。 */
export const PANEL_HEIGHT: Record<PageShape, number> = {
  list: 420,
  detail: 420,
  split: 540,
};

// 三种形态的排版类：只在 render 里套用，别处不得自行拼宽度。

/** 单列列表（list）：吃满宽度。 */
export const LIST_FULL_CLASS =
  "min-h-0 flex-1 overflow-y-auto px-2 pb-12";

/** 两栏页面的左侧列表：**窄栏**（会话标题 + 时间够用），详情吃掉剩余宽度。 */
export const LIST_NARROW_CLASS =
  "min-h-0 w-[280px] shrink-0 overflow-y-auto border-r border-zinc-800 px-2 pb-12";

/** 两栏页面的右侧详情（也是详情整屏用的容器）：吃满剩余宽度。 */
export const DETAIL_PANE_CLASS =
  "md min-h-0 flex-1 overflow-y-auto px-4 pb-12 pt-3 text-sm text-zinc-200";

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
