import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./styles.css";
import { store } from "./store";

// ---- 类型：镜像 moe-core 的 serde camelCase 契约（ADR-0006）----

type ActionKind = "primary" | "secondary";
interface Action {
  id: string;
  title: string;
  kind: ActionKind;
  keybinding?: string | null;
}
interface Item {
  id: string;
  title: string;
  subtitle?: string;
  actions: Action[];
  payload: unknown;
}
interface CommandMeta {
  id: string;
  extensionId: string;
  title: string;
  subtitle?: string;
  input: "none" | "query" | "selection";
}
// 外部 tagged 枚举：单位变体（silent/openSideView）序列化为裸字符串。
type ActionResult =
  | string
  | { writeBack: { text: string } }
  | { list: { items: Item[] } };

// ---- 视图状态 ----

type Mode = "commands" | "items" | "actions";
interface View {
  mode: Mode;
  focus: number;
  commands: CommandMeta[];
  items: Item[];
  actions: Action[];
  /** actions 模式下来自哪个 item；items 模式的来源 Command。 */
  sourceCommandId?: string;
  itemIndex?: number;
}

const q = document.querySelector<HTMLInputElement>("#query")!;
const listEl = document.querySelector<HTMLUListElement>("#list")!;
const detailEl = document.querySelector<HTMLDivElement>("#detail")!;
const hintsEl = document.querySelector<HTMLElement>("#hints")!;

const view = store<View>({
  mode: "commands",
  focus: 0,
  commands: [],
  items: [],
  actions: [],
});

// ---- Hints Bar：键位语义来自 Rust 统一键位表，视图层不自行发明 ----

const HINT_LABELS: Record<string, string> = {
  navDown: "导航",
  navUp: "导航",
  apply: "应用",
  secondaryCopy: "复制",
  showAllActions: "动作",
  back: "回退",
  materialize: "侧栏",
};

async function initHints() {
  const entries = await invoke<[string, string][]>("keymap");
  hintsEl.replaceChildren(
    ...entries.map(([display, semantic]) => {
      const span = document.createElement("span");
      const kbd = document.createElement("kbd");
      kbd.className = "text-zinc-200";
      kbd.textContent = display;
      span.append(kbd, ` ${HINT_LABELS[semantic] ?? semantic}`);
      return span;
    }),
  );
}

// ---- 渲染 ----

function currentEntries(): { title: string; subtitle?: string; key?: string }[] {
  const v = view.get();
  if (v.mode === "commands") {
    return v.commands.map((c) => ({ title: c.title, subtitle: c.subtitle }));
  }
  if (v.mode === "items") {
    return v.items.map((i) => ({
      title: i.title,
      subtitle: i.subtitle,
      key: secondaryOf(i)?.keybinding ?? undefined,
    }));
  }
  return v.actions.map((a) => ({ title: a.title, key: a.keybinding ?? undefined }));
}

function render() {
  const v = view.get();
  listEl.replaceChildren(
    ...currentEntries().map((e, i) => {
      const li = document.createElement("li");
      li.className =
        "flex items-baseline justify-between gap-4 rounded-lg px-3 py-1.5 text-sm " +
        (i === v.focus ? "bg-zinc-700/70 text-zinc-50" : "text-zinc-300");
      const left = document.createElement("div");
      left.className = "min-w-0 truncate";
      left.textContent = e.title;
      if (e.subtitle) {
        const sub = document.createElement("span");
        sub.className = "ml-2 text-xs text-zinc-500";
        sub.textContent = e.subtitle;
        left.append(sub);
      }
      li.append(left);
      if (e.key) {
        const k = document.createElement("kbd");
        k.className = "shrink-0 text-xs text-zinc-400";
        k.textContent = e.key;
        li.append(k);
      }
      li.addEventListener("mousedown", () => {
        view.update((s) => ({ ...s, focus: i }));
      });
      return li;
    }),
  );
}

view.subscribe(render);

// ---- 语义动作 ----

function primaryOf(item: Item) {
  return item.actions.find((a) => a.kind === "primary") ?? item.actions[0];
}
function secondaryOf(item: Item) {
  return item.actions.find((a) => a.kind === "secondary");
}

function showDetail(text: string) {
  detailEl.textContent = text;
  detailEl.classList.remove("hidden");
}
function hideDetail() {
  detailEl.classList.add("hidden");
}

function applyResult(res: ActionResult, commandId: string) {
  if (typeof res === "string") {
    if (res === "openSideView") showDetail("Side View：M3 接入（ADR-0004）");
    return;
  }
  if ("writeBack" in res) {
    // M2: 交给 moe-platform::TextTarget 真实回写（ADR-0002/0006）。
    showDetail(`WriteBack（M2 真实回写前预览）\n\n${res.writeBack.text}`);
    return;
  }
  if ("list" in res) {
    view.update((v) => ({
      ...v,
      mode: "items",
      items: res.list.items,
      focus: 0,
      sourceCommandId: commandId,
    }));
  }
}

async function applyFocused(alt: boolean) {
  const v = view.get();
  if (v.mode === "commands") {
    const cmd = v.commands[v.focus];
    if (!cmd) return;
    const res = await invoke<ActionResult>("invoke_command", {
      commandId: cmd.id,
      query: q.value || null,
    });
    applyResult(res, cmd.id);
    return;
  }
  if (v.mode === "items") {
    const item = v.items[v.focus];
    const action = alt
      ? (secondaryOf(item) ?? primaryOf(item))
      : primaryOf(item);
    if (!item || !action || !v.sourceCommandId) return;
    const res = await invoke<ActionResult>("run_item_action", {
      commandId: v.sourceCommandId,
      item,
      action,
    });
    applyResult(res, v.sourceCommandId);
    return;
  }
  // actions 模式：⌘K 展开后的选择
  const action = v.actions[v.focus];
  const item = v.items[v.itemIndex ?? 0];
  if (!action || !item || !v.sourceCommandId) return;
  const res = await invoke<ActionResult>("run_item_action", {
    commandId: v.sourceCommandId,
    item,
    action,
  });
  applyResult(res, v.sourceCommandId);
}

function move(delta: number) {
  view.update((v) => {
    const len =
      v.mode === "commands" ? v.commands.length : v.mode === "items" ? v.items.length : v.actions.length;
    return { ...v, focus: len === 0 ? 0 : (v.focus + delta + len) % len };
  });
}

function openActions() {
  const v = view.get();
  if (v.mode !== "items") return;
  view.update((s) => ({
    ...s,
    mode: "actions",
    actions: s.items[s.focus]?.actions ?? [],
    itemIndex: s.focus,
    focus: 0,
  }));
}

async function materialize() {
  const v = view.get();
  const item = v.mode === "items" ? v.items[v.focus] : undefined;
  const action = item?.actions.find((a) => a.id === "materialize" || a.keybinding === "⌘M");
  if (item && action && v.sourceCommandId) {
    const res = await invoke<ActionResult>("run_item_action", {
      commandId: v.sourceCommandId,
      item,
      action,
    });
    applyResult(res, v.sourceCommandId);
  }
}

// Esc 分层回退：detail → 结果层 → 输入 → 关面板（ADR-0006 Keymap::Back）
async function back() {
  const v = view.get();
  if (!detailEl.classList.contains("hidden")) {
    hideDetail();
    return;
  }
  if (v.mode !== "commands") {
    view.update((s) => ({ ...s, mode: "commands", items: [], actions: [], focus: 0 }));
    return;
  }
  if (q.value) {
    q.value = "";
    await refresh("");
    return;
  }
  await invoke("hide_panel");
}

async function refresh(query: string) {
  hideDetail();
  const commands = await invoke<CommandMeta[]>("search_commands", { query });
  view.update((v) => ({ ...v, mode: "commands", commands, focus: 0 }));
}

// ---- 全局键盘事件（keyboard-first：所有能力都可达，鼠标仅冗余）----

window.addEventListener("keydown", (e) => {
  if (e.key === "ArrowDown" || (e.ctrlKey && e.key === "n")) {
    e.preventDefault();
    move(1);
  } else if (e.key === "ArrowUp" || (e.ctrlKey && e.key === "p")) {
    e.preventDefault();
    move(-1);
  } else if (e.key === "Enter") {
    e.preventDefault();
    void applyFocused(e.altKey);
  } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
    e.preventDefault();
    openActions();
  } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "m") {
    e.preventDefault();
    void materialize();
  } else if (e.key === "Escape") {
    e.preventDefault();
    void back();
  }
});

let debounce: ReturnType<typeof setTimeout> | undefined;
q.addEventListener("input", () => {
  clearTimeout(debounce);
  debounce = setTimeout(() => void refresh(q.value), 60);
});

// ---- 呼出授权引导（ADR-0008）：未授权时常显，授权后自动消失 ----

interface SummonStatus {
  status: "ready" | "needsPermission" | "unsupported";
  key: string;
  doubleTapMs: number;
}

const KEY_LABELS: Record<string, string> = {
  "double-cmd": "双击 ⌘",
  "double-option": "双击 ⌥",
  "double-ctrl": "双击 ⌃",
  "double-shift": "双击 ⇧",
};

const bannerEl = document.querySelector<HTMLDivElement>("#banner")!;
const bannerTextEl = document.querySelector<HTMLSpanElement>("#banner-text")!;
const bannerActionEl = document.querySelector<HTMLButtonElement>("#banner-action")!;

function hideBanner() {
  bannerEl.classList.add("hidden");
  bannerEl.classList.remove("flex");
}

async function refreshBanner() {
  const status = await invoke<SummonStatus>("summon_status");
  if (status.status === "needsPermission") {
    const key = KEY_LABELS[status.key] ?? status.key;
    bannerTextEl.textContent = `${key} 呼出需要「输入监控」授权；授权后自动生效，个别系统版本需重启 Moe 一次。`;
    bannerEl.classList.remove("hidden");
    bannerEl.classList.add("flex");
  } else {
    hideBanner();
  }
}

bannerActionEl.addEventListener("click", () => void invoke("open_permission_settings"));
void listen("summon-authorized", () => hideBanner());

// 呼出时清空回到命令层
window.addEventListener("focus", () => {
  q.value = "";
  void refresh("");
  void refreshBanner();
});

await initHints();
await refresh("");
await refreshBanner();
render();
