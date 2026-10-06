// 键位块（shadcn Kbd 同款）：每个键一个方块，字符居中、行内基线对齐。
// 展示串来自 `moe-core::keymap`（如 "⌘K / ⌘⇧P"、"↓ / ⌃N"、"⌥⏎"），
// 这里只负责切块与排版，不发明键位。

/** 具名键（一个名字一个块，不按字符切开）。 */
const NAMED_KEYS = [
  "Enter",
  "Return",
  "Esc",
  "Space",
  "Tab",
  "Delete",
  "Backspace",
  "↑",
  "↓",
  "←",
  "→",
  "⏎",
];

/** "⌘⇧A" → ["⌘","⇧","A"]；"Esc" → ["Esc"]。 */
function tokenize(part: string): string[] {
  const tokens: string[] = [];
  let rest = part;
  outer: while (rest.length > 0) {
    const lower = rest.toLowerCase();
    for (const named of NAMED_KEYS) {
      if (lower.startsWith(named.toLowerCase())) {
        tokens.push(named);
        rest = rest.slice(named.length);
        continue outer;
      }
    }
    tokens.push(rest[0]);
    rest = rest.slice(1);
  }
  return tokens;
}

export interface KbdOptions {
  /** 只画第一种键位（如 "⌘K / ⌘⇧P" → ⌘K），按钮里用。 */
  firstOnly?: boolean;
}

/** 键位块序列；可选项之间用 " / " 分隔。 */
export function kbdEl(display: string, options: KbdOptions = {}): HTMLElement {
  const wrap = document.createElement("span");
  wrap.className = "moe-keys";
  const parts = display
    .split("/")
    .map((part) => part.trim())
    .filter(Boolean);
  const shown = options.firstOnly ? parts.slice(0, 1) : parts;
  shown.forEach((part, index) => {
    if (index > 0) {
      const sep = document.createElement("span");
      sep.className = "moe-keys-sep";
      sep.textContent = "/";
      wrap.append(sep);
    }
    for (const token of tokenize(part)) {
      const kbd = document.createElement("kbd");
      kbd.className = "moe-kbd";
      kbd.textContent = token;
      wrap.append(kbd);
    }
  });
  return wrap;
}

/** 纯文本形式（placeholder、aria 等不能放 DOM 的地方）。 */
export function kbdText(display: string): string {
  return display
    .split("/")
    .map((part) => part.trim())
    .filter(Boolean)
    .join(" / ");
}
