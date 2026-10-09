// Key blocks (shadcn Kbd style): one square per key, centered characters, inline baseline alignment.
// Display strings come from `moe-core::keymap` (e.g. "⌘K / ⌘⇧P", "↓ / ⌃N", "⌥⏎");
// this module only splits and lays them out, never invents bindings.

/** Named keys (one block per name, not split by character). */
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

/** "⌘⇧A" → ["⌘","⇧","A"]; "Esc" → ["Esc"]. */
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
  /** Render only the first binding (e.g. "⌘K / ⌘⇧P" → ⌘K), for buttons. */
  firstOnly?: boolean;
}

/** Sequence of key blocks; alternatives separated by " / ". */
export function kbdEl(display: string, options: KbdOptions = {}): HTMLElement {
  const wrap = document.createElement("span");
  wrap.className = "moe-keys";
  // Split on the " / " separator only (spaces included): a bare "/" is a key of its own,
  // so "⌘/" must tokenize to ⌘ + / instead of losing the slash.
  const parts = display
    .split(" / ")
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
      // Named keys (Esc/Tab…) use the same square, shrunk font to fit (see styles.css)
      if (token.length > 1) kbd.dataset.wide = "true";
      kbd.textContent = token;
      wrap.append(kbd);
    }
  });
  return wrap;
}

/** Plain-text form (for placeholders, aria, and other non-DOM spots). */
export function kbdText(display: string): string {
  return display
    .split(" / ")
    .map((part) => part.trim())
    .filter(Boolean)
    .join(" / ");
}
