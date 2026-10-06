import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import DOMPurify from "dompurify";
import { marked } from "marked";
import "./styles.css";
import { appendMention, humanBytes, validatePath } from "./attachment";
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
  detail?: string | null;
  /** 仍在产出中（流式占位）：Esc 时优先请求停止生成。 */
  pending?: boolean;
}
interface CommandMeta {
  id: string;
  extensionId: string;
  title: string;
  subtitle?: string;
  input: "none" | "query" | "selection";
  /** Live 列表：进入后输入变化即重跑（如历史搜索）。 */
  live: boolean;
}
// 外部 tagged 枚举：单位变体（silent）序列化为裸字符串；
// openSideView/writeBack 等带载荷变体为对象（侧栏开窗由后端执行）。
type ActionResult =
  | string
  | { writeBack: { text: string } }
  | { list: { items: Item[] } }
  | { openSideView: { payload: unknown } };

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
  /** 来源 Command 是否 Live（输入变化即重跑列表）。 */
  sourceLive?: boolean;
  itemIndex?: number;
}

const q = document.querySelector<HTMLInputElement>("#query")!;
const listEl = document.querySelector<HTMLUListElement>("#list")!;
const detailEl = document.querySelector<HTMLDivElement>("#detail")!;
const hintsEl = document.querySelector<HTMLElement>("#hints")!;
const toastEl = document.querySelector<HTMLDivElement>("#toast")!;

const view = store<View>({
  mode: "commands",
  focus: 0,
  commands: [],
  items: [],
  actions: [],
});

/// 当前详情卡片对应的 item（流式事件据此重渲）
let detailItemId: string | null = null;
/** Hints Bar 里的动态「Esc 停止」提示（仅在流式期间显示）。 */
let stopHintEl: HTMLSpanElement | null = null;
/** 详情卡片的形态：preview（焦点预览，列表仍在）/ message（全屏卡片，如错误）。 */
type DetailMode = "none" | "preview" | "message";
let detailMode: DetailMode = "none";
/** Esc 收起预览后，直到焦点变化才重新展开（Raycast 同款层级）。 */
let previewDismissed = false;

// ---- 轻量反馈（⌥⏎ 复制等无 UI 结果的动作）----

let toastTimer: ReturnType<typeof setTimeout> | undefined;
function toast(text: string) {
  toastEl.textContent = text;
  toastEl.classList.remove("hidden");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => toastEl.classList.add("hidden"), 1200);
}

// ---- Hints Bar：键位语义来自 Rust 统一键位表，视图层不自行发明 ----

const HINT_LABELS: Record<string, string> = {
  navDown: "导航",
  navUp: "导航",
  apply: "应用",
  secondaryCopy: "复制",
  showAllActions: "动作",
  back: "回退",
  materialize: "侧栏",
  attach: "附件",
};

async function initHints() {
  const entries = await invoke<[string, string][]>("keymap");
  // 流式期间的动态键位：Esc = 停止生成（pending 时显示，IIE4AD-365）
  const stop = document.createElement("span");
  stop.className = "hidden";
  const stopKey = document.createElement("kbd");
  stopKey.className = "text-zinc-200";
  stopKey.textContent = "Esc";
  stop.append(stopKey, " 停止");
  stopHintEl = stop;
  hintsEl.replaceChildren(
    ...entries.map(([display, semantic]) => {
      const span = document.createElement("span");
      const kbd = document.createElement("kbd");
      kbd.className = "text-zinc-200";
      kbd.textContent = display;
      span.append(kbd, ` ${HINT_LABELS[semantic] ?? semantic}`);
      return span;
    }),
    stop,
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
  const rows = currentEntries().map((e, i) => {
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
  });
  listEl.replaceChildren(...rows);
  // 键盘导航：焦点行始终留在视口内（长列表）
  rows[v.focus]?.scrollIntoView({ block: "nearest" });
  renderDetail();
  // 流式占位项：显示「Esc 停止」
  const pending = v.mode === "items" && v.items[v.focus]?.pending === true;
  stopHintEl?.classList.toggle("hidden", !pending);
}

// ---- 详情卡片：焦点预览（列表共存）/ 全屏消息（错误等）----

const DETAIL_MESSAGE_CLASS =
  "md min-h-0 flex-1 overflow-y-auto px-4 pb-3 text-sm text-zinc-200";
const DETAIL_PREVIEW_CLASS =
  "md max-h-[55%] flex-none overflow-y-auto border-b border-zinc-800 px-4 pb-2 pt-3 text-sm text-zinc-200";

function paintDetail(markdown: string, itemId: string | null) {
  const nearBottom =
    detailEl.scrollHeight - detailEl.scrollTop - detailEl.clientHeight < 40;
  detailEl.innerHTML = DOMPurify.sanitize(
    marked.parse(markdown, { async: false }),
  );
  detailItemId = itemId;
  if (nearBottom) detailEl.scrollTop = detailEl.scrollHeight;
}

function clearDetail() {
  detailMode = "none";
  detailItemId = null;
  detailEl.className = "md hidden";
  detailEl.replaceChildren();
}

/** 全屏卡片（错误、回写失败等）：隐藏列表。 */
function showMessage(markdown: string) {
  detailMode = "message";
  detailEl.className = DETAIL_MESSAGE_CLASS;
  paintDetail(markdown, null);
  listEl.classList.add("hidden");
}

/** 焦点预览：items 模式下焦点项有 detail 就展示（列表保持可见）。 */
function renderDetail() {
  const v = view.get();
  const item = v.mode === "items" ? v.items[v.focus] : undefined;
  const preview = item?.detail;
  if (detailMode === "message" || previewDismissed || !preview) {
    if (detailMode !== "message") clearDetail();
    return;
  }
  detailMode = "preview";
  detailEl.className = DETAIL_PREVIEW_CLASS;
  paintDetail(preview, item.id);
  listEl.classList.remove("hidden");
}

/** Esc 的第一层：消费掉可见的详情（返回 true 表示已消费）。 */
function dismissDetail(): boolean {
  if (detailMode === "none") return false;
  if (detailMode === "preview") previewDismissed = true;
  clearDetail();
  listEl.classList.remove("hidden");
  return true;
}

view.subscribe(render);

// ---- 语义动作 ----

function primaryOf(item: Item) {
  return item.actions.find((a) => a.kind === "primary") ?? item.actions[0];
}
function secondaryOf(item: Item) {
  return item.actions.find((a) => a.kind === "secondary");
}

function applyResult(res: ActionResult, commandId: string, live = false) {
  if (typeof res === "string") {
    // silent：无需 UI 动作（openSideView 的开窗已由后端完成）
    return;
  }
  if ("writeBack" in res) {
    // 回写由后端完成（收面板 → AX / 剪贴板降级投递）；面板此刻已被收起
    return;
  }
  if ("openSideView" in res) {
    // 侧栏窗口由后端展示并收到载荷；面板已收起
    return;
  }
  if ("list" in res) {
    const items = res.list.items;
    clearDetail();
    previewDismissed = false;
    view.update((v) => ({
      ...v,
      mode: "items",
      items,
      focus: 0,
      sourceCommandId: commandId,
      sourceLive: live,
    }));
  }
}

async function applyFocused(alt: boolean) {
  const v = view.get();
  if (v.mode === "commands") {
    const cmd = v.commands[v.focus];
    if (!cmd) return;
    try {
      const res = await invoke<ActionResult>("invoke_command", {
        commandId: cmd.id,
        // Live 命令以自己的视图接管输入：进入时清空输入框，之后的输入即该命令的查询
        query: cmd.live ? null : q.value || null,
      });
      if (cmd.live) {
        q.value = "";
      }
      applyResult(res, cmd.id, cmd.live);
    } catch (err) {
      showMessage(`执行失败：${String(err)}`);
    }
    return;
  }
  if (v.mode === "items") {
    const item = v.items[v.focus];
    const action = alt
      ? (secondaryOf(item) ?? primaryOf(item))
      : primaryOf(item);
    if (!item || !action || !v.sourceCommandId) return;
    try {
      const res = await invoke<ActionResult>("run_item_action", {
        commandId: v.sourceCommandId,
        item,
        action,
      });
      if (action.id === "copy") toast("已复制");
      applyResult(res, v.sourceCommandId, v.sourceLive);
    } catch (err) {
      showMessage(`执行失败：${String(err)}`);
    }
    return;
  }
  // actions 模式：⌘K 展开后的选择
  const action = v.actions[v.focus];
  const item = v.items[v.itemIndex ?? 0];
  if (!action || !item || !v.sourceCommandId) return;
  try {
    const res = await invoke<ActionResult>("run_item_action", {
      commandId: v.sourceCommandId,
      item,
      action,
    });
    if (action.id === "copy") toast("已复制");
    applyResult(res, v.sourceCommandId, v.sourceLive);
  } catch (err) {
    showMessage(`执行失败：${String(err)}`);
  }
}

function move(delta: number) {
  previewDismissed = false;
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
    applyResult(res, v.sourceCommandId, v.sourceLive);
  }
}

/** Live 命令：输入变化即用新查询重跑列表（如「AI: 搜索历史会话」）。 */
async function rerunLive(query: string) {
  const v = view.get();
  if (!v.sourceCommandId) return;
  try {
    const res = await invoke<ActionResult>("invoke_command", {
      commandId: v.sourceCommandId,
      query: query || null,
      record: false, // 重跑不算一次启动（frecency 语义）
    });
    applyResult(res, v.sourceCommandId, true);
  } catch (err) {
    showMessage(`执行失败：${String(err)}`);
  }
}

// Esc 分层回退：停止生成 → 预览/详情 → 结果层 → 输入 → 关面板（ADR-0006 Keymap::Back）
async function back() {
  const v = view.get();
  // 流式生成中：Esc 的第一优先级是停止（IIE4AD-365）
  if (v.mode === "items" && v.items[v.focus]?.pending) {
    await stopGeneration();
    return;
  }
  if (dismissDetail()) {
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
  clearDetail();
  previewDismissed = false;
  const commands = await invoke<CommandMeta[]>("search_commands", { query });
  view.update((v) => ({ ...v, mode: "commands", commands, focus: 0 }));
}

/** 停止进行中的生成（平台级：不区分扩展/会话，IIE4AD-365）。 */
async function stopGeneration() {
  try {
    const stopped = await invoke<number>("stop_generation");
    toast(stopped > 0 ? "已停止生成" : "没有进行中的生成");
  } catch (err) {
    showMessage(`停止失败：${String(err)}`);
  }
}

// ---- 附件输入（⌘⇧A）：复用同一 Input Bar 输入路径，Enter 插入 `@"path"` mention（ADR-0010）----

const attachBarEl = document.querySelector<HTMLDivElement>("#attach")!;
const attachTextEl = document.querySelector<HTMLSpanElement>("#attach-text")!;
const QUERY_PLACEHOLDER = "搜索 Command…";
const ATTACH_BAR_BASE =
  "items-center justify-between gap-3 border-b px-4 py-2 text-xs";

let attaching = false;
let savedQuery = "";
let attachBarToken = 0;

function setAttachBar(kind: "hint" | "error", text: string) {
  attachTextEl.textContent = text;
  const palette =
    kind === "error"
      ? "border-red-500/30 bg-red-500/10 text-red-200"
      : "border-sky-500/30 bg-sky-500/10 text-sky-200";
  attachBarEl.className = `${ATTACH_BAR_BASE} flex ${palette}`;
}

function hideAttachBar() {
  attachBarEl.className = `${ATTACH_BAR_BASE} hidden`;
}

function startAttach() {
  if (attaching) return;
  attaching = true;
  savedQuery = q.value;
  q.value = "";
  q.placeholder = "粘贴文件路径，Enter 添加附件";
  setAttachBar(
    "hint",
    "附件模式：粘贴文件路径，Enter 添加（Esc 取消；支持 ~ 与含空格路径）",
  );
  q.focus();
}

function cancelAttach() {
  if (!attaching) return;
  attaching = false;
  q.value = savedQuery;
  savedQuery = "";
  q.placeholder = QUERY_PLACEHOLDER;
  hideAttachBar();
}

async function submitAttach() {
  const raw = q.value.trim();
  if (!raw) {
    cancelAttach();
    return;
  }
  try {
    const info = await validatePath(raw);
    attaching = false;
    q.value = appendMention(savedQuery, info.path);
    savedQuery = "";
    q.placeholder = QUERY_PLACEHOLDER;
    const kind = info.kind === "image" ? "图片" : "文本";
    setAttachBar(
      "hint",
      `已添加附件：${info.name}（${kind}，${humanBytes(info.bytes)}）`,
    );
    const token = ++attachBarToken;
    window.setTimeout(() => {
      if (!attaching && token === attachBarToken) hideAttachBar();
    }, 3000);
    // 列表跟随新查询（纯附件时会出现「AI: 带附件的提问」入口，避免误跑第一个命令）
    void refresh(q.value);
    q.focus();
  } catch (err) {
    setAttachBar("error", `附件添加失败：${String(err)}`);
  }
}

// ---- 全局键盘事件（keyboard-first：所有能力都可达，鼠标仅冗余）----

window.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "a") {
    e.preventDefault();
    if (attaching) cancelAttach();
    else startAttach();
    return;
  }
  if (attaching) {
    // 附件模式下输入栏只服务路径：不触发任何面板语义
    if (e.key === "Enter") {
      e.preventDefault();
      void submitAttach();
    } else if (e.key === "Escape") {
      e.preventDefault();
      cancelAttach();
    }
    return;
  }
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
  debounce = setTimeout(() => {
    if (attaching) return;
    const v = view.get();
    if (v.mode === "items" && v.sourceLive && v.sourceCommandId) {
      void rerunLive(q.value);
    } else {
      void refresh(q.value);
    }
  }, 60);
});

// ---- 呼出授权引导（ADR-0008）：未授权时常显，授权后自动消失 ----

interface SummonStatus {
  status: "ready" | "needsPermission" | "unsupported";
  key: string;
  doubleTapMs: number;
  accessibility: boolean;
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
    bannerActionEl.textContent = "打开输入监控设置";
    bannerActionEl.dataset.action = "input-monitoring";
    showBanner();
  } else if (status.status === "unsupported") {
    // Linux/Wayland 等无法全局拦截键盘的会话（IIE4AD-350）：给替代路径
    const key = KEY_LABELS[status.key] ?? status.key;
    bannerTextEl.textContent = `${key} 呼出在当前会话不可用（Wayland 等环境无法全局拦截键盘）：改 config.toml 的 [summon] key 用组合键，或用 WM 绑定 \`moe --toggle\`。`;
    bannerActionEl.textContent = "打开配置文件";
    bannerActionEl.dataset.action = "config";
    showBanner();
  } else if (!status.accessibility) {
    bannerTextEl.textContent =
      "读取选区与回写需要「辅助功能」授权（写回时也会自动弹系统引导）；授权后无需重启。";
    bannerActionEl.textContent = "打开辅助功能设置";
    bannerActionEl.dataset.action = "accessibility";
    showBanner();
  } else {
    hideBanner();
  }
}

function showBanner() {
  bannerEl.classList.remove("hidden");
  bannerEl.classList.add("flex");
}

bannerActionEl.addEventListener("click", () => {
  const action = bannerActionEl.dataset.action;
  if (action === "config") {
    void invoke("invoke_command", {
      commandId: "moe.open-config",
      query: null,
      record: false,
    });
    return;
  }
  void invoke(
    action === "accessibility"
      ? "open_accessibility_settings"
      : "open_input_monitoring_settings",
  );
});

// 流式命令事件：按 item id 就地更新（如 AI 回答逐字到达）
interface CommandEventPayload {
  itemUpdated?: { commandId: string; item: Item };
}

void listen<CommandEventPayload>("command-event", (event) => {
  const payload = event.payload?.itemUpdated;
  if (!payload) return;
  const v = view.get();
  if (v.mode !== "items" || v.sourceCommandId !== payload.commandId) return;
  const idx = v.items.findIndex((i) => i.id === payload.item.id);
  if (idx === -1) return;
  const items = v.items.slice();
  items[idx] = payload.item;
  // view 更新会触发 render() → 预览就地在流式事件上重渲（保持贴底）
  view.update((s) => ({ ...s, items }));
});
void listen("summon-authorized", () => hideBanner());

// 呼出时保留上次的输入与结果（用户可能在隐藏后补充输入），
// 只刷新权限引导状态（可能刚去系统设置授过权）；未完成的附件输入不跨呼出保留。
window.addEventListener("focus", () => {
  cancelAttach();
  void refreshBanner();
});

await initHints();
await refresh("");
await refreshBanner();
render();
