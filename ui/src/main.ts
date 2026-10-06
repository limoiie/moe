import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import DOMPurify from "dompurify";
import { marked } from "marked";
import "./styles.css";
import { appendMention, humanBytes, validatePath } from "./attachment";
import { generatingEl } from "./generating";
import { iconEl } from "./icons";
import { kbdEl } from "./kbd";
import { GENERAL_KEY_LABELS, generalActionOf, type EntryAction } from "./keymap";
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
  /** 图标语义名（ADR-0012）；缺省时用来源 Command 的图标。 */
  icon?: string;
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
  /** 图标语义名（ADR-0012）。 */
  icon?: string;
  input: "none" | "query" | "selection";
  /** Live 列表：进入后输入变化即重跑（如历史搜索）。 */
  live: boolean;
}
// 外部 tagged 枚举：单位变体（silent）序列化为裸字符串；
// openSideView/writeBack 等带载荷变体为对象（侧栏开窗由后端执行）。
type ActionResult =
  | string
  | { writeBack: { text: string } }
  | { list: { items: Item[]; detailFull?: boolean } }
  | { openSideView: { payload: unknown } };

// ---- 视图状态 ----

/** actions 层的条目：Focused Item 的动作，或平台通用动作（Browse/New，ADR-0014）。 */
interface ActionRow {
  action: Action;
  platform?: EntryAction;
}

type Mode = "commands" | "items" | "actions";
interface View {
  mode: Mode;
  focus: number;
  commands: CommandMeta[];
  items: Item[];
  actions: ActionRow[];
  /** actions 模式下来自哪个 item；items 模式的来源 Command。 */
  sourceCommandId?: string;
  /** 来源 Command 的图标（结果项未自带图标时的回退）。 */
  sourceIcon?: string;
  /** 来源 Command 的标题（Input Bar 的 placeholder；经 ⌘P/⌘N 入口进来时也在命令表之外）。 */
  sourceTitle?: string;
  /** 来源 Command 是否 Live（输入变化即重跑列表）。 */
  sourceLive?: boolean;
  /** 结果声明的视图形态：true = 唯一一条即内容，详情占满面板（ADR-0013）。 */
  detailFull?: boolean;
  itemIndex?: number;
}

const q = document.querySelector<HTMLInputElement>("#query")!;
const listEl = document.querySelector<HTMLUListElement>("#list")!;
const detailEl = document.querySelector<HTMLDivElement>("#detail")!;
const actionBarEl = document.querySelector<HTMLDivElement>("#action-bar")!;
const toastEl = document.querySelector<HTMLDivElement>("#toast")!;
const toastIconEl = document.querySelector<HTMLSpanElement>("#toast-icon")!;
const toastTextEl = document.querySelector<HTMLSpanElement>("#toast-text")!;
const inputIconEl = document.querySelector<HTMLSpanElement>("#input-icon")!;
const bannerIconEl = document.querySelector<HTMLSpanElement>("#banner-icon")!;

const view = store<View>({
  mode: "commands",
  focus: 0,
  commands: [],
  items: [],
  actions: [],
});

/// 当前详情卡片对应的 item（流式事件据此重渲）
let detailItemId: string | null = null;
/** 详情卡片的形态：preview（焦点预览，列表仍在）/ message（全屏卡片，如错误）。 */
type DetailMode = "none" | "preview" | "message";
let detailMode: DetailMode = "none";
/** Esc 收起预览后，直到焦点变化才重新展开（Raycast 同款层级）。 */
let previewDismissed = false;

// ---- 轻量反馈（⌥⏎ 复制等无 UI 结果的动作）----

let toastTimer: ReturnType<typeof setTimeout> | undefined;
function toast(text: string) {
  toastTextEl.textContent = text;
  toastIconEl.replaceChildren(iconEl("check", { size: 14, className: "text-emerald-400" }));
  toastEl.classList.remove("hidden");
  toastEl.classList.add("flex");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    toastEl.classList.add("hidden");
    toastEl.classList.remove("flex");
  }, 1200);
}

// ---- 悬浮动作条（Raycast 同款）：主操作 + 动作，键位仍取自 Rust 统一键位表 ----

/** 语义 → 展示串（如 apply → "⏎"、showAllActions → "⌘K / ⌘⇧P"）。 */
const keyDisplay = new Map<string, string>();

async function initActionBar() {
  const entries = await invoke<[string, string][]>("keymap");
  for (const [display, semantic] of entries) {
    if (!keyDisplay.has(semantic)) keyDisplay.set(semantic, display);
  }
  renderActionBar();
}

/** 焦点项的主操作（与 Apply 同一语义）：返回标题与点击时的落点。 */
function primaryActionOf(): { title: string; keys: string; run: () => void; disabled: boolean } {
  const v = view.get();
  const applyKeys = keyDisplay.get("apply") ?? "⏎";
  if (v.mode === "items") {
    const item = v.items[v.focus];
    // 生成中：主操作让位给停止（IIE4AD-365，Esc 的第一优先级）
    if (item?.pending) {
      return {
        title: "停止生成",
        keys: keyDisplay.get("back") ?? "Esc",
        run: () => void stopGeneration(),
        disabled: false,
      };
    }
    const action = item ? primaryOf(item) : undefined;
    return {
      title: action?.title ?? "应用",
      keys: action?.keybinding ?? applyKeys,
      run: () => void applyFocused(false),
      disabled: !action,
    };
  }
  if (v.mode === "actions") {
    const row = v.actions[v.focus];
    return {
      title: row?.action.title ?? "应用",
      keys: row?.action.keybinding ?? applyKeys,
      run: () => void applyFocused(false),
      disabled: !row,
    };
  }
  return { title: "应用", keys: applyKeys, run: () => void applyFocused(false), disabled: false };
}

function renderActionBar() {
  const primary = primaryActionOf();
  const primaryButton = document.createElement("button");
  primaryButton.id = "primary-action";
  primaryButton.disabled = primary.disabled;
  primaryButton.title = `${primary.title}（${primary.keys}）`;
  primaryButton.append(
    document.createTextNode(primary.title),
    kbdEl(primary.keys, { firstOnly: true }),
  );
  primaryButton.addEventListener("click", () => {
    primary.run();
    q.focus(); // 动作条不抢输入栏焦点（keyboard-first）
  });

  const actionsKeys = keyDisplay.get("showAllActions") ?? "⌘K";
  const actionsButton = document.createElement("button");
  actionsButton.id = "actions-action";
  actionsButton.title = `全部动作（${actionsKeys}）`;
  actionsButton.append(document.createTextNode("动作"), kbdEl(actionsKeys, { firstOnly: true }));
  actionsButton.addEventListener("click", () => {
    if (view.get().mode === "actions") closeActions();
    else void openActions();
    q.focus();
  });

  // 按钮不能抢走输入栏焦点（否则后续 Enter 会重复点击按钮）
  for (const button of [primaryButton, actionsButton]) {
    button.addEventListener("mousedown", (e) => e.preventDefault());
  }

  actionBarEl.replaceChildren(primaryButton, actionsButton);
}

// ---- 渲染 ----

interface Row {
  title: string;
  subtitle?: string;
  key?: string;
  /** 语义名；undefined = 用来源 Command 的图标，仍无则回退。 */
  icon?: string;
  /** 回退图形（未指定图标）時淡化显示。 */
  iconMuted?: boolean;
}

function currentEntries(): Row[] {
  const v = view.get();
  if (v.mode === "commands") {
    return v.commands.map((c) => ({
      title: c.title,
      subtitle: c.subtitle,
      icon: c.icon,
      iconMuted: !c.icon,
    }));
  }
  if (v.mode === "items") {
    return v.items.map((i) => {
      const icon = i.icon ?? v.sourceIcon;
      return {
        title: i.title,
        subtitle: i.subtitle,
        key: secondaryOf(i)?.keybinding ?? undefined,
        icon,
        iconMuted: !icon,
      };
    });
  }
  return v.actions.map(({ action, platform }) => ({
    title: action.title,
    key: action.keybinding ?? (action.kind === "primary" ? "⏎" : undefined),
    icon: platform
      ? platform === "browse"
        ? "history"
        : "plus"
      : action.kind === "primary"
        ? "corner-down-left"
        : "copy",
  }));
}

function render() {
  const v = view.get();
  const rows = currentEntries().map((e, i) => {
    const focused = i === v.focus;
    const li = document.createElement("li");
    li.className =
      "flex items-center gap-2.5 rounded-lg px-3 py-1.5 text-sm " +
      (focused ? "bg-zinc-700/70 text-zinc-50" : "text-zinc-300");
    li.append(
      iconEl(e.icon, {
        className: e.iconMuted
          ? "text-zinc-600"
          : focused
            ? "text-zinc-200"
            : "text-zinc-400",
      }),
    );
    const left = document.createElement("div");
    left.className = "min-w-0 flex-1 truncate";
    left.textContent = e.title;
    if (e.subtitle) {
      const sub = document.createElement("span");
      sub.className = "ml-2 text-xs text-zinc-500";
      sub.textContent = e.subtitle;
      left.append(sub);
    }
    li.append(left);
    if (e.key) {
      // 键位块（shadcn Kbd 同款）：与动作条、⌘K 面板保持同一种嵌键样式
      const keys = kbdEl(e.key, { firstOnly: true });
      keys.classList.add("shrink-0");
      li.append(keys);
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
  renderActionBar();
  updatePlaceholder();
}

/** Input Bar 的 placeholder 跟随当前层（Raycast 同款：进了哪个命令就显示哪个）。 */
function updatePlaceholder() {
  if (attaching) return; // 附件模式自己管
  const v = view.get();
  if (v.mode === "commands") {
    q.placeholder = QUERY_PLACEHOLDER;
  } else if (v.mode === "actions") {
    q.placeholder = "选择动作…";
  } else {
    const command = v.commands.find((c) => c.id === v.sourceCommandId);
    q.placeholder = v.sourceTitle ?? command?.title ?? "结果…";
  }
}

// ---- 详情卡片：焦点预览（列表共存）/ 全屏消息（错误等）----

// 结果层多条：列表在左、详情在右（Raycast 同款左右分栏，IIE4AD 反馈 #1）
// 底部留出悬浮动作条的高度（pb-14），最后一屏内容不被按钮遮住
const DETAIL_MESSAGE_CLASS =
  "md min-h-0 flex-1 overflow-y-auto px-4 pb-14 pt-3 text-sm text-zinc-200";
const DETAIL_PREVIEW_CLASS =
  "md w-[58%] shrink-0 overflow-y-auto border-l border-zinc-800 px-4 pb-14 pt-3 text-sm text-zinc-200";

function paintDetail(markdown: string, itemId: string | null, pending: boolean) {
  const nearBottom =
    detailEl.scrollHeight - detailEl.scrollTop - detailEl.clientHeight < 40;
  const parts = ensureDetailParts();
  parts.body.innerHTML = DOMPurify.sanitize(
    marked.parse(markdown, { async: false }),
  );
  // 生成中的行内指示（像 ChatGPT 的加载点，而不是把状态写成正文）：
  // 正文与指示分开，流式事件只换正文，三点动画不被重建打断
  parts.indicator.classList.toggle("hidden", !pending);
  detailItemId = itemId;
  if (nearBottom) detailEl.scrollTop = detailEl.scrollHeight;
}

interface DetailParts {
  body: HTMLElement;
  indicator: HTMLElement;
}
let detailParts: DetailParts | null = null;

/** 详情正文容器 + 固定的行内生成指示（clearDetail 后重建）。 */
function ensureDetailParts(): DetailParts {
  if (!detailParts || !detailEl.contains(detailParts.body)) {
    const body = document.createElement("div");
    const indicator = generatingEl();
    detailEl.replaceChildren(body, indicator);
    detailParts = { body, indicator };
  }
  return detailParts;
}

function clearDetail() {
  detailMode = "none";
  detailItemId = null;
  detailParts = null;
  detailEl.className = "md hidden";
  detailEl.replaceChildren();
  listEl.classList.remove("hidden");
}

/** 全屏卡片（错误、回写失败等）：隐藏列表。 */
function showMessage(markdown: string) {
  detailMode = "message";
  detailEl.className = DETAIL_MESSAGE_CLASS;
  paintDetail(markdown, null, false);
  listEl.classList.add("hidden");
}

/**
 * items 模式的详情：
 * - 结果声明 detailFull（如 AI 回答、通知）→ 整屏就是内容，生成状态用行内指示；
 * - 否则（如历史搜索）→ 左列表、右预览。
 */
function renderDetail() {
  const v = view.get();
  const item = v.mode === "items" ? v.items[v.focus] : undefined;
  const hasContent = !!item && (item.detail != null || item.pending === true);
  if (detailMode === "message" || previewDismissed || !hasContent) {
    if (detailMode !== "message") clearDetail();
    return;
  }
  const full = v.detailFull === true && v.items.length === 1;
  detailMode = "preview";
  detailEl.className = full ? DETAIL_MESSAGE_CLASS : DETAIL_PREVIEW_CLASS;
  paintDetail(item.detail ?? "", item.id, item.pending === true);
  if (full) listEl.classList.add("hidden");
  else listEl.classList.remove("hidden");
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

function applyResult(
  res: ActionResult,
  commandId: string,
  live = false,
  icon?: string,
  title?: string,
) {
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
      sourceIcon: icon,
      sourceTitle: title,
      detailFull: res.list.detailFull === true,
    }));
  }
}

/**
 * 通用动作（ADR-0014）：Browse（⌘P）打开当前 Extension 的记录列表，
 * New（⌘N）新建一条记录。平台只定键位与路由，入口由 Extension 声明；
 * 没声明就给一次内联提示，不静默。
 */
async function openEntry(kind: "browse" | "new") {
  const v = view.get();
  // 当前 Extension：进了结果/动作层用来源 Command，还在命令层用焦点 Command
  const from = v.sourceCommandId ?? (v.mode === "commands" ? v.commands[v.focus]?.id : undefined);
  if (!from) return;
  let command: CommandMeta | null;
  try {
    command = await invoke<CommandMeta | null>("entry_command", {
      commandId: from,
      kind,
    });
  } catch (err) {
    showMessage(`执行失败：${String(err)}`);
    return;
  }
  if (!command) {
    toast(kind === "browse" ? "该扩展没有记录列表" : "该扩展没有新建入口");
    return;
  }
  try {
    const res = await invoke<ActionResult>("invoke_command", {
      commandId: command.id,
      // Live 命令以自己的视图接管输入：进入时清空输入框（同 applyFocused）
      query: null,
    });
    if (command.live) q.value = "";
    applyResult(res, command.id, command.live, command.icon, command.title);
  } catch (err) {
    showMessage(`执行失败：${String(err)}`);
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
      applyResult(res, cmd.id, cmd.live, cmd.icon, cmd.title);
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
      applyResult(res, v.sourceCommandId, v.sourceLive, v.sourceIcon, v.sourceTitle);
    } catch (err) {
      showMessage(`执行失败：${String(err)}`);
    }
    return;
  }
  // actions 模式：⌘K 展开后的选择（item 动作，或平台通用动作 Browse/New）
  const row = v.actions[v.focus];
  if (!row) return;
  if (row.platform) {
    // 平台通用动作（ADR-0014）：与 ⌘P/⌘N 同一条路径，没声明就给提示
    closeActions();
    await openEntry(row.platform);
    return;
  }
  const action = row.action;
  const item = v.items[v.itemIndex ?? 0];
  if (!item || !v.sourceCommandId) return;
  try {
    const res = await invoke<ActionResult>("run_item_action", {
      commandId: v.sourceCommandId,
      item,
      action,
    });
    if (action.id === "copy") toast("已复制");
    applyResult(res, v.sourceCommandId, v.sourceLive, v.sourceIcon, v.sourceTitle);
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

/**
 * 打开动作层：Focused Item 的主/副操作，末尾附上该 Extension 真的
 * 声明了的平台通用动作（Browse ⌘P / New ⌘N，ADR-0014）——所有可做的事都在这一层。
 */
async function openActions() {
  const v = view.get();
  if (v.mode !== "items") return;
  const item = v.items[v.focus];
  const rows: ActionRow[] = (item?.actions ?? []).map((action) => ({ action }));
  if (v.sourceCommandId) {
    const kinds: EntryAction[] = ["browse", "new"];
    const found = await Promise.all(
      kinds.map((kind) =>
        invoke<CommandMeta | null>("entry_command", {
          commandId: v.sourceCommandId,
          kind,
        }).catch(() => null),
      ),
    );
    kinds.forEach((kind, index) => {
      const entry = found[index];
      if (!entry) return;
      rows.push({
        platform: kind,
        action: {
          id: `moe.platform.${kind}`,
          title: kind === "browse" ? "浏览记录" : "新建记录",
          kind: "secondary",
          keybinding: GENERAL_KEY_LABELS[kind],
        },
      });
    });
  }
  view.update((s) => ({
    ...s,
    mode: "actions",
    actions: rows,
    itemIndex: s.focus,
    focus: 0,
  }));
}

/** 动作层返回上一层（结果层）；Esc 与动作按钮都走这里。 */
function closeActions() {
  const v = view.get();
  if (v.mode !== "actions") return;
  view.update((s) => ({ ...s, mode: "items", focus: s.itemIndex ?? 0 }));
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
    applyResult(res, v.sourceCommandId, v.sourceLive, v.sourceIcon, v.sourceTitle);
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
    applyResult(res, v.sourceCommandId, true, v.sourceIcon, v.sourceTitle);
  } catch (err) {
    showMessage(`执行失败：${String(err)}`);
  }
}

// Esc 分层回退：停止生成 → 动作层 → 预览/详情 → 结果层 → 输入 → 关面板（ADR-0006 Keymap::Back）
async function back() {
  const v = view.get();
  // 流式生成中：Esc 的第一优先级是停止（IIE4AD-365）
  if (v.mode === "items" && v.items[v.focus]?.pending) {
    await stopGeneration();
    return;
  }
  // 动作层是自己打开的一层：Esc 先收回结果层，再往下退（Raycast 同款）
  if (v.mode === "actions") {
    closeActions();
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

/** Input Bar 前置图标：平时搜索，附件模式换纸夹（ADR-0012）。 */
function renderInputIcon() {
  // 与列表行同一列：尺寸/颜色对齐（ADR：Input Bar 与 Result List 同一网格）
  inputIconEl.replaceChildren(
    iconEl(attaching ? "paperclip" : "search", { size: 16, className: "text-zinc-400" }),
  );
}

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
  renderInputIcon();
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
  renderInputIcon();
  q.value = savedQuery;
  savedQuery = "";
  updatePlaceholder();
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
    renderInputIcon();
    q.value = appendMention(savedQuery, info.path);
    savedQuery = "";
    updatePlaceholder();
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
  // 通用动作（ADR-0014）：Browse ⌘P / Actions ⌘⇧P / New ⌘N。
  // 语义由共享键位模块识别，落点由本表面决定（面板 = 动作层 / Extension 声明的入口）。
  const general = generalActionOf(e);
  if (general) {
    e.preventDefault();
    if (general === "actions") void openActions();
    else void openEntry(general);
    return;
  }
  if (e.key === "ArrowDown" || (e.ctrlKey && !e.metaKey && e.key === "n")) {
    e.preventDefault();
    move(1);
  } else if (e.key === "ArrowUp" || (e.ctrlKey && !e.metaKey && e.key === "p")) {
    e.preventDefault();
    move(-1);
  } else if (e.key === "Enter") {
    e.preventDefault();
    void applyFocused(e.altKey);
  } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
    e.preventDefault();
    void openActions();
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
  bannerIconEl.replaceChildren(iconEl("alert", { size: 14 }));
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

await initActionBar();
renderInputIcon();
await refresh("");
await refreshBanner();
render();
