// The floating "card": a filter input + rows grouped into Raycast-style sections (ADR-0026 amendment).
// The panel's actions card and the Side View's More Actions card are this one component (ADR-0028):
// identical rows, sections, filter, ↑↓/⏎ navigation, Esc/empty-⌫ dismissal and click-outside handling —
// only the anchor and the input's edge differ, and those are HTML structure, not this module.
//
// The owner supplies the DOM elements and the sections per open; the card owns the rest.
// Headers never join navigation; while filtering, rows flatten and headers hide (the Raycast convention).

import { iconEl } from "./icons";
import { kbdEl } from "./kbd";

export interface CardRow {
  /** Row label; also what the filter matches. */
  title: string;
  /** Lucide semantic name (icons.ts). */
  icon: string;
  /** Kbd display string (e.g. "⌘N"); null/undefined = the row has no key block. */
  keys?: string | null;
  /** Placeholder rows ("not built yet") render dimmed, are skipped by ↑↓ and can never run (ADR-0029). */
  disabled?: boolean;
  run: () => void;
}

export interface CardSection {
  title: string;
  rows: CardRow[];
}

export interface CardOptions {
  cardEl: HTMLElement;
  listEl: HTMLElement;
  inputEl: HTMLInputElement;
  /** Message when there is nothing to show and no filter is set (a filtered-empty card always reads "No matching actions"). */
  emptyText?: string;
  /** The focused row changed (open, ↑↓, filter): the panel's action pill follows it. */
  onFocusChange?: (row: CardRow | undefined) => void;
  /** The card was hidden (e.g. return focus to the window's input bar / composer). */
  onClose?: () => void;
}

export interface Card {
  open(sections: CardSection[]): void;
  close(): void;
  isOpen(): boolean;
  /** The focused row (undefined when the card is closed or has no rows). */
  focused(): CardRow | undefined;
  /** Run the focused row (closes the card first): the action pill's click path. */
  runFocused(): void;
}

export function createCard(options: CardOptions): Card {
  const { cardEl, listEl, inputEl, emptyText = "No matching actions", onFocusChange, onClose } = options;
  let sections: CardSection[] = [];
  /** Focus over the flat row order (sections are rendering only; headers are never focused). */
  let focus = 0;
  let open = false;

  /** Filter rows by title (case-insensitive); a section whose rows all filtered out disappears. */
  function filtered(): CardSection[] {
    const needle = inputEl.value.trim().toLowerCase();
    if (!needle) return sections;
    return sections
      .map((section) => ({
        title: section.title,
        rows: section.rows.filter((row) => row.title.toLowerCase().includes(needle)),
      }))
      .filter((section) => section.rows.length > 0);
  }

  function flatRows(): CardRow[] {
    return filtered().flatMap((section) => section.rows);
  }

  function headerEl(title: string): HTMLLIElement {
    const li = document.createElement("li");
    li.className =
      "select-none px-2.5 pt-2 pb-0.5 text-[11px] font-medium uppercase tracking-wider text-fg-subtle";
    li.textContent = title;
    return li;
  }

  function rowEl(row: CardRow, focused: boolean): HTMLLIElement {
    const li = document.createElement("li");
    const disabled = row.disabled === true;
    li.className =
      "flex cursor-default items-center gap-2.5 rounded-xl px-2.5 py-1.5 text-sm transition-colors duration-100 " +
      (disabled
        ? "text-fg-subtle opacity-45"
        : focused
          ? "bg-surface-selected text-fg"
          : "text-fg hover:bg-surface-hover");
    li.append(
      iconEl(row.icon, {
        size: 15,
        className: disabled ? "text-fg-faint" : focused ? "text-fg" : "text-fg-subtle",
      }),
    );
    const title = document.createElement("span");
    title.className = "min-w-0 flex-1 truncate";
    title.textContent = row.title;
    li.append(title);
    if (row.keys) {
      const kbd = kbdEl(row.keys, { firstOnly: true });
      kbd.classList.add("shrink-0");
      li.append(kbd);
    }
    if (!disabled) li.addEventListener("click", () => runRow(row));
    return li;
  }

  function render() {
    const rows = flatRows();
    focus = rows.length === 0 ? 0 : Math.min(focus, rows.length - 1);
    if (rows.length === 0 || rows.every((row) => row.disabled)) {
      const empty = document.createElement("li");
      empty.className = "px-2 py-3 text-xs text-fg-subtle";
      empty.textContent = inputEl.value.trim() ? "No matching actions" : emptyText;
      listEl.replaceChildren(empty);
      onFocusChange?.(undefined);
      return;
    }
    // Focus never rests on a disabled placeholder row (ADR-0029)
    if (rows[focus]?.disabled) {
      const next = rows.findIndex((row) => !row.disabled);
      focus = next === -1 ? 0 : next;
    }
    // Raycast-style: while filtering, matches render as one flat list without headers; headers return with the cleared input.
    const showHeaders = inputEl.value.trim() === "";
    const lis: HTMLLIElement[] = [];
    let index = 0;
    let focusedLi: HTMLLIElement | null = null;
    for (const section of filtered()) {
      if (showHeaders) lis.push(headerEl(section.title));
      for (const row of section.rows) {
        const li = rowEl(row, index === focus);
        if (index === focus) focusedLi = li;
        lis.push(li);
        index += 1;
      }
    }
    listEl.replaceChildren(...lis);
    focusedLi?.scrollIntoView({ block: "nearest" });
    onFocusChange?.(rows[focus]);
  }

  function openCard(next: CardSection[]) {
    sections = next;
    focus = 0;
    inputEl.value = "";
    open = true;
    cardEl.classList.remove("hidden");
    render();
    inputEl.focus();
  }

  function close() {
    if (!open) return;
    open = false;
    cardEl.classList.add("hidden");
    onClose?.();
  }

  function runRow(row: CardRow) {
    if (row.disabled) return;
    close();
    row.run();
  }

  function focusedRow(): CardRow | undefined {
    if (!open) return undefined;
    return flatRows()[focus];
  }

  function move(delta: number) {
    const rows = flatRows();
    if (rows.length === 0) return;
    // Skip disabled placeholder rows (ADR-0029)
    let next = focus;
    for (let step = 0; step < rows.length; step++) {
      next = (next + delta + rows.length) % rows.length;
      if (!rows[next].disabled) break;
    }
    focus = next;
    render();
  }

  inputEl.addEventListener("input", () => {
    focus = 0;
    render();
  });

  // The card owns its keys while its input is focused (same UX as the panel card and the About card)
  inputEl.addEventListener("keydown", (e) => {
    const up =
      e.key === "ArrowUp" ||
      (e.ctrlKey && !e.metaKey && !e.shiftKey && e.key.toLowerCase() === "p");
    const down =
      e.key === "ArrowDown" ||
      (e.ctrlKey && !e.metaKey && !e.shiftKey && e.key.toLowerCase() === "n");
    if (up || down) {
      e.preventDefault();
      e.stopPropagation();
      move(down ? 1 : -1);
    } else if (e.key === "Enter" && !e.isComposing) {
      e.preventDefault();
      e.stopPropagation();
      runFocused();
    } else if (e.key === "Escape" || e.key === "Backspace") {
      // The front-most card owns these keys: the window-level "empty Backspace = Back" must
      // never see them (otherwise deleting a filter char would close the card, IIE4AD-406)
      e.stopPropagation();
      if (e.key === "Escape" || (inputEl.value === "" && !e.repeat)) {
        e.preventDefault();
        close();
      }
    }
  });

  function runFocused() {
    const row = focusedRow();
    if (row) runRow(row);
  }

  return {
    open: openCard,
    close,
    isOpen: () => open,
    focused: focusedRow,
    runFocused,
  };
}