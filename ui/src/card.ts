// The floating "card": a filter input + rows, optionally grouped into Raycast-style sections
// (ADR-0026 amendment). The panel's actions card and the Side View's More Actions card are this one
// component (ADR-0028): identical rows, sections, filter, ↑↓/⏎ navigation, Esc/empty-⌫ dismissal
// and click-outside handling — only the anchor and the input's edge differ, and those are HTML
// structure, not this module. A section without a title renders headerless; a row marked
// `separator` draws a thin rule above itself (the actions card's rank groups, ADR-0037 amendment).
//
// The owner supplies the DOM elements and the sections per open; the card owns the rest.
// Headers never join navigation; while filtering, rows flatten and headers (and separators) hide
// (the Raycast convention).

import { iconEl } from "./icons";
import { kbdEl } from "./kbd";

export interface CardRow {
  /** Row label; also what the filter matches. */
  title: string;
  /** Lucide semantic name (icons.ts); omitted when `leading` carries the visual instead. */
  icon?: string;
  /** Custom leading element (a provider mark, MOE-0030): rendered before the icon slot. */
  leading?: HTMLElement;
  /** Custom trailing element (ability tiles, MOE-0030): rendered before the check mark. */
  trailing?: HTMLElement;
  /** A trailing check mark (the switcher's active model, MOE-0029). */
  checked?: boolean;
  /** Kbd display string (e.g. "⌘N"); null/undefined = the row has no key block. */
  keys?: string | null;
  /** Placeholder rows ("not built yet") render dimmed, are skipped by ↑↓ and can never run (ADR-0029). */
  disabled?: boolean;
  /** Draw a hairline above this row: the first row of a rank group inside an actions section
   *  (ADR-0037 amendment). Ignored while filtering (rows flatten, headers hide). */
  separator?: boolean;
  /**
   * Run without closing the card (ADR-0044). Row-level by intent: it is the row that knows — the
   * About card's theme rows stay open so a second theme can be tried immediately (ADR-0035
   * amendment), while "Open Config File" closes. The Filter Card does not need it; single-select
   * closes on choose.
   */
  keepOpen?: boolean;
  run: () => void;
}

export interface CardSection {
  /** Section header; absent renders the rows as one headerless list (the actions card's canonical order). */
  title?: string;
  /** Custom element before the header text (a provider mark, MOE-0030). */
  leading?: HTMLElement;
  rows: CardRow[];
}

export interface CardOptions {
  cardEl: HTMLElement;
  listEl: HTMLElement;
  inputEl: HTMLInputElement;
  /** Message when there is nothing to show and no filter is set. */
  emptyText?: string;
  /** Message when the filter excludes every row. Defaults to "No matching actions". */
  filteredEmptyText?: string;
  /**
   * Server-side filtering (the model switcher, MOE-0030): the owner replaces the sections per
   * query, so the card can offer rows it has not loaded (the backend ranks and caps them). Absent
   * = the card filters its own sections client-side by row title. Either way the input sits where
   * the owner's markup put it — the menu cards keep it at the bottom (ADR-0028 amendment).
   */
  onQuery?: (query: string) => CardSection[] | Promise<CardSection[]>;
  /**
   * Hover follows focus (the model switcher's companion detail card, MOE-0030): pointing at a row
   * focuses it, so the detail card follows the mouse the way it follows ↑↓. The guard keeps the
   * re-render from looping on the element the pointer already sits over.
   */
  focusOnHover?: boolean;
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
  /**
   * Re-render in place, optionally with fresh sections, keeping the filter text and the focused row
   * (ADR-0044): the caller's data changed without the card being reopened — the About card's theme
   * check follows the new preference. A no-op while closed.
   */
  refresh(sections?: CardSection[]): void;
}

export function createCard(options: CardOptions): Card {
  const {
    cardEl,
    listEl,
    inputEl,
    emptyText = "No matching actions",
    filteredEmptyText = "No matching actions",
    onFocusChange,
    onClose,
  } = options;
  let sections: CardSection[] = [];
  /** Focus over the flat row order (sections are rendering only; headers are never focused). */
  let focus = 0;
  let open = false;
  /** Bumped per query so out-of-order server responses drop (see the input listener). */
  let queryToken = 0;

  /** Filter rows by title (case-insensitive); a section whose rows all filtered out disappears.
      Server-filtered cards (`onQuery`) arrive pre-filtered — a client title filter on top would
      hide exactly the capability matches the backend found. */
  function filtered(): CardSection[] {
    const needle = inputEl.value.trim().toLowerCase();
    if (options.onQuery || !needle) return sections;
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

  function headerEl(title: string, leading?: HTMLElement): HTMLLIElement {
    const li = document.createElement("li");
    li.className =
      "select-none flex items-center gap-1.5 px-2.5 pt-2 pb-0.5 text-[11px] font-medium uppercase tracking-wider text-fg-subtle";
    if (leading) li.append(leading);
    const text = document.createElement("span");
    text.className = "min-w-0 truncate";
    text.textContent = title;
    li.append(text);
    return li;
  }

  /** A hairline between rank groups inside a section (ADR-0037 amendment); never focusable. */
  function separatorEl(): HTMLLIElement {
    const li = document.createElement("li");
    li.className = "mx-2.5 my-1 border-t border-line";
    li.setAttribute("role", "presentation");
    return li;
  }

  function rowEl(row: CardRow, focused: boolean, index: number): HTMLLIElement {
    const li = document.createElement("li");
    const disabled = row.disabled === true;
    li.className =
      "moe-row flex cursor-default items-center gap-2.5 rounded-xl px-2.5 py-1.5 text-sm transition-colors duration-100 " +
      (disabled
        ? "text-fg-subtle opacity-45"
        : focused
          ? "bg-surface-selected text-fg"
          : "text-fg");
    if (focused) li.dataset.focused = "true";
    if (row.leading) li.append(row.leading);
    if (row.icon) {
      li.append(
        iconEl(row.icon, {
          size: 15,
          className: disabled ? "text-fg-faint" : focused ? "text-fg" : "text-fg-subtle",
        }),
      );
    }
    const title = document.createElement("span");
    title.className = "min-w-0 flex-1 truncate";
    title.textContent = row.title;
    li.append(title);
    if (row.trailing) li.append(row.trailing);
    if (row.checked) {
      li.append(iconEl("check", { size: 14, className: "shrink-0 text-accent" }));
    }
    if (row.keys) {
      const kbd = kbdEl(row.keys, { firstOnly: true });
      kbd.classList.add("shrink-0");
      li.append(kbd);
    }
    if (!disabled) li.addEventListener("click", () => runRow(row));
    if (options.focusOnHover && !disabled) {
      li.addEventListener("mouseenter", () => {
        if (index !== focus) {
          focus = index;
          render();
        }
      });
    }
    return li;
  }

  function render() {
    const rows = flatRows();
    focus = rows.length === 0 ? 0 : Math.min(focus, rows.length - 1);
    if (rows.length === 0 || rows.every((row) => row.disabled)) {
      const empty = document.createElement("li");
      empty.className = "px-2 py-3 text-xs text-fg-subtle";
      empty.textContent = inputEl.value.trim() ? filteredEmptyText : emptyText;
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
      if (showHeaders && section.title) lis.push(headerEl(section.title, section.leading));
      let firstRow = true;
      for (const row of section.rows) {
        if (showHeaders && row.separator && !firstRow) lis.push(separatorEl());
        const li = rowEl(row, index === focus, index);
        if (index === focus) focusedLi = li;
        lis.push(li);
        index += 1;
        firstRow = false;
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
    if (!row.keepOpen) close();
    row.run();
  }

  /**
   * Re-render without reopening (ADR-0044): a `keepOpen` row changed the data the card is showing,
   * so the rows are rebuilt while the filter text and the focused row stay where the user left them.
   */
  function refreshCard(next?: CardSection[]) {
    if (next) sections = next;
    if (open) render();
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
    if (options.onQuery) {
      // A late response must not overwrite a newer query's sections.
      const token = ++queryToken;
      const value = inputEl.value;
      void Promise.resolve(options.onQuery(value)).then((next) => {
        if (token !== queryToken || !open) return;
        sections = next;
        render();
      });
      return;
    }
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
    refresh: refreshCard,
  };
}