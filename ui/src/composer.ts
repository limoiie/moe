// The shared composer row (MOE-0030, revised): the bottom bar's own language — the round glass
// add button, one capsule-like one-row input, and the round glass actions button, exactly the
// glass the root page's about chip and action pill wear. The panel's Quick Ask page and the Side
// View mount this same component, so the two surfaces' input affordances cannot drift (the
// message stream already shares messages.ts). Enter sends (and stops while generating); the add
// button swaps the capsule into attachment mode (a path input, Enter attaches, Esc cancels).

import { appendMention, validatePath } from "./attachment";
import { iconEl } from "./icons";

export interface Composer {
  /** The row (mount it into the surface's footer). */
  root: HTMLElement;
  /** The one-row draft input inside the capsule. */
  input: HTMLInputElement;
  /** Round glass add button (attachment path input). */
  addButton: HTMLButtonElement;
  /** Round glass actions button (the surface's actions card). */
  actionsButton: HTMLButtonElement;
  focus(): void;
  attachMode(): boolean;
  toggleAttach(): void;
  closeAttach(): void;
  /** Validate the path input; on success the mention joins the draft and the capsule returns. */
  submitAttach(): Promise<boolean>;
}

/** The round glass buttons flanking the capsule: the about chip's own style (ADR-0026). */
const ROUND_BUTTON = "moe-avatar-chip moe-focus-ring shrink-0";

export function createComposer(options: {
  placeholder?: string;
  onSend: () => void;
  onStop: () => void;
  onActions: () => void;
}): Composer {
  const root = document.createElement("div");
  root.className = "flex items-center gap-2";

  const addButton = document.createElement("button");
  addButton.className = ROUND_BUTTON;
  addButton.title = "Add attachment (path input)";
  addButton.append(iconEl("plus", { size: 15 }));

  const capsule = document.createElement("div");
  capsule.className = "moe-composer-capsule min-w-0 flex-1";

  const input = document.createElement("input");
  input.type = "text";
  input.placeholder = options.placeholder ?? "Ask anything";
  input.spellcheck = false;
  input.className =
    "min-w-0 flex-1 bg-transparent text-sm text-fg caret-accent placeholder-fg-subtle outline-none";

  const attachInput = document.createElement("input");
  attachInput.type = "text";
  attachInput.placeholder = "Paste a file path, Enter to attach (Esc cancels)";
  attachInput.spellcheck = false;
  attachInput.className =
    "hidden min-w-0 flex-1 bg-transparent text-sm text-fg caret-accent placeholder-fg-subtle outline-none";

  const attachMsg = document.createElement("span");
  attachMsg.className = "shrink-0 text-[11px] text-danger";

  capsule.append(input, attachInput, attachMsg);

  const actionsButton = document.createElement("button");
  actionsButton.className = ROUND_BUTTON;
  actionsButton.append(iconEl("command", { size: 15 }));

  root.append(addButton, capsule, actionsButton);

  function attachMode() {
    return !attachInput.classList.contains("hidden");
  }
  function openAttach() {
    input.classList.add("hidden");
    attachInput.classList.remove("hidden");
    attachMsg.textContent = "";
    attachInput.focus();
  }
  function closeAttach() {
    attachInput.classList.add("hidden");
    attachInput.value = "";
    attachMsg.textContent = "";
    input.classList.remove("hidden");
    input.focus();
  }
  function toggleAttach() {
    if (attachMode()) closeAttach();
    else openAttach();
  }

  async function submitAttach(): Promise<boolean> {
    const raw = attachInput.value.trim();
    if (!raw) {
      closeAttach();
      return true;
    }
    try {
      const info = await validatePath(raw);
      input.value = appendMention(input.value, info.path);
      closeAttach();
      return true;
    } catch (err) {
      attachMsg.textContent = String(err);
      return false;
    }
  }

  addButton.addEventListener("mousedown", (e) => e.preventDefault());
  actionsButton.addEventListener("mousedown", (e) => e.preventDefault());
  addButton.addEventListener("click", toggleAttach);
  actionsButton.addEventListener("click", options.onActions);
  attachInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.isComposing) {
      e.preventDefault();
      e.stopPropagation();
      void submitAttach();
    } else if (
      e.key === "Escape" ||
      (e.key === "Backspace" && attachInput.value === "" && !e.repeat)
    ) {
      e.preventDefault();
      e.stopPropagation();
      closeAttach();
    }
  });

  return {
    root,
    input,
    addButton,
    actionsButton,
    focus: () => input.focus(),
    attachMode,
    toggleAttach,
    closeAttach,
    submitAttach,
  };
}
