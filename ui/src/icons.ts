// Lucide icons (ADR-0012): the contract stores only semantic names; concrete shapes are mapped by the view layer.
// Unknown names fall back to Circle, so no Extension can break the layout with an icon name.

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

/** Semantic name → Lucide shape (extensions may only use names registered here). */
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
  // AI Commands (ADR-0024)
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
  // Fixed use (the UI's own icons, not set by extensions)
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

/** Whether the icon name is known (used when the UI must tell "extension-provided icon" from "fallback shape"). */
export function hasIcon(name: string | null | undefined): boolean {
  return !!name && name in ICONS;
}

/** Semantic name → inline SVG (16px stroke icon, color follows currentColor). */
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
