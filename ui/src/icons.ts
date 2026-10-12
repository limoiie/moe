// Lucide icons (ADR-0012): the contract stores only semantic names; concrete shapes are mapped by the view layer.
// Unknown names fall back to Circle, so no Extension can break the layout with an icon name.

import {
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  Briefcase,
  Check,
  ChevronDown,
  ChevronRight,
  Circle,
  Command,
  Copy,
  CornerDownLeft,
  Expand,
  ExternalLink,
  Filter,
  FileText,
  Globe,
  History,
  Image,
  Info,
  KeyRound,
  Languages,
  Lightbulb,
  List,
  ListChecks,
  Megaphone,
  MessageCircleWarning,
  MessageSquare,
  MessagesSquare,
  Minus,
  Monitor,
  Moon,
  PanelLeft,
  PanelRight,
  Paperclip,
  PenLine,
  Plus,
  Quote,
  RotateCw,
  Search,
  Send,
  Settings2,
  Shrink,
  Smile,
  Sparkles,
  SpellCheck,
  Square,
  Star,
  Sun,
  Tag,
  Terminal,
  TextCursorInput,
  Trash2,
  TriangleAlert,
  Type,
  Wand2,
  Wrench,
  X,
  Zap,
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
  "message-circle-warning": MessageCircleWarning,
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
  "arrow-up": ArrowUp,
  // Favorites (ADR-0029)
  star: Star,
  // Appearance (the About card's theme rows, ADR-0035 amendment)
  monitor: Monitor,
  sun: Sun,
  moon: Moon,
  // Tool Call rows (MOE-0026)
  "file-text": FileText,
  globe: Globe,
  "rotate-cw": RotateCw,
  // The Filter Control and its dimension (ADR-0043)
  tag: Tag,
  filter: Filter,
  // The Thinking row's lightning (MOE-0036)
  zap: Zap,
  // Model detail (MOE-0030 revision)
  wrench: Wrench,
  image: Image,
  "external-link": ExternalLink,
  // The text modality tile reads the "T" glyph (MOE-0030 revision 8)
  type: Type,
  // Fixed use (the UI's own icons, not set by extensions)
  search: Search,
  "text-cursor-input": TextCursorInput,
  "arrow-left": ArrowLeft,
  "chevron-right": ChevronRight,
  "chevron-down": ChevronDown,
  "trash-2": Trash2,
  command: Command,
  copy: Copy,
  "corner-down-left": CornerDownLeft,
  "panel-right": PanelRight,
  "panel-left": PanelLeft,
  plus: Plus,
  minus: Minus,
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
  /** Stroke width in the 24-unit viewBox (default 1.75); scales with `size`. */
  strokeWidth?: number;
  /** Semantic color name (ADR-0041). Mapped to an ink class; unknown names add none (currentColor). */
  color?: string;
}

/**
 * Semantic color name → ink class (ADR-0041's bounded palette). This is the mapping `iconEl` needed
 * to tint an accessory glyph, and the same names the result-row tags use. An unknown name adds no
 * class, so the glyph keeps `currentColor` — the same fallback discipline as an unknown icon name.
 */
const INK_CLASSES: Record<string, string> = {
  blue: "moe-ink-blue",
  green: "moe-ink-green",
  magenta: "moe-ink-magenta",
  orange: "moe-ink-orange",
  purple: "moe-ink-purple",
  red: "moe-ink-red",
  yellow: "moe-ink-yellow",
  primary: "moe-ink-primary",
  secondary: "moe-ink-secondary",
};

/** The ink class for a semantic color name, or undefined when the name is unset/unknown. */
export function inkClass(name: string | null | undefined): string | undefined {
  return name ? INK_CLASSES[name] : undefined;
}

/** Whether the icon name is known (used when the UI must tell "extension-provided icon" from "fallback shape"). */
export function hasIcon(name: string | null | undefined): boolean {
  return !!name && name in ICONS;
}

/** Semantic name → inline SVG (16px stroke icon, color follows currentColor). */
export function iconEl(name: string | null | undefined, options: IconOptions = {}): SVGElement {
  const node = (name && ICONS[name]) || Circle;
  const size = options.size ?? 16;
  const svg = createElement(node, {
    width: size,
    height: size,
    "stroke-width": options.strokeWidth ?? 1.75,
  });
  svg.setAttribute("aria-hidden", "true");
  svg.classList.add("shrink-0");
  for (const cls of (options.className ?? "").split(/\s+/).filter(Boolean)) {
    svg.classList.add(cls);
  }
  const ink = inkClass(options.color);
  if (ink) svg.classList.add(ink);
  return svg;
}
