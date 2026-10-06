// Lucide 图标（ADR-0012）：契约里只存语义名，具体图形由视图层映射。
// 未知名字回退到 Circle，保证任何 Extension 都不会因为图标名而破坏布局。

import {
  ArrowRight,
  Briefcase,
  Check,
  Circle,
  Command,
  Copy,
  CornerDownLeft,
  Expand,
  History,
  Info,
  KeyRound,
  Languages,
  Lightbulb,
  List,
  ListChecks,
  Megaphone,
  MessageSquare,
  MessagesSquare,
  PanelLeft,
  PanelRight,
  Paperclip,
  PenLine,
  Plus,
  Quote,
  Search,
  Send,
  Settings2,
  Shrink,
  Smile,
  Sparkles,
  SpellCheck,
  Square,
  Terminal,
  TriangleAlert,
  Wand2,
  X,
  createElement,
  type IconNode,
} from "lucide";

/** 语义名 → Lucide 图形（扩展只用这里注册过的名字）。 */
const ICONS: Record<string, IconNode> = {
  sparkles: Sparkles,
  history: History,
  "message-square": MessageSquare,
  "messages-square": MessagesSquare,
  info: Info,
  "settings-2": Settings2,
  "key-round": KeyRound,
  terminal: Terminal,
  list: List,
  megaphone: Megaphone,
  quote: Quote,
  // AI Commands（ADR-0024）
  "wand-2": Wand2,
  "spell-check": SpellCheck,
  shrink: Shrink,
  expand: Expand,
  "pen-line": PenLine,
  "list-checks": ListChecks,
  languages: Languages,
  briefcase: Briefcase,
  smile: Smile,
  lightbulb: Lightbulb,
  "arrow-right": ArrowRight,
  // 固定用途（界面自己用，不由扩展指定）
  search: Search,
  command: Command,
  copy: Copy,
  "corner-down-left": CornerDownLeft,
  "panel-right": PanelRight,
  "panel-left": PanelLeft,
  plus: Plus,
  paperclip: Paperclip,
  send: Send,
  stop: Square,
  alert: TriangleAlert,
  check: Check,
  close: X,
};

export interface IconOptions {
  size?: number;
  className?: string;
}

/** 图标名是否已知（UI 需要区分「扩展给了图标」与「回退图形」时用）。 */
export function hasIcon(name: string | null | undefined): boolean {
  return !!name && name in ICONS;
}

/** 语义名 → 内联 SVG（16px 描边图标，颜色随 currentColor）。 */
export function iconEl(name: string | null | undefined, options: IconOptions = {}): SVGElement {
  const node = (name && ICONS[name]) || Circle;
  const size = options.size ?? 16;
  const svg = createElement(node, { width: size, height: size, "stroke-width": 1.75 });
  svg.setAttribute("aria-hidden", "true");
  svg.classList.add("shrink-0");
  for (const cls of (options.className ?? "").split(/\s+/).filter(Boolean)) {
    svg.classList.add(cls);
  }
  return svg;
}
