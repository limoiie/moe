// The settings window (MOE-0029): sidebar sections on the left, one pane on the right. The AI
// section manages the providers this install uses — per-provider keys, which models appear in
// the pickers (`hidden_models`), and the active provider/model pair — over the embedded model
// catalog. Everything else stays config-file driven (ADR-0009 amendment); the file is linked.

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./styles.css";
import { iconEl } from "./icons";
import { providerMark } from "./logos";
import { initTheme } from "./theme";

initTheme();

// ---- Backend shapes (camelCase mirrors of moe-extensions::ai_models) ----

interface ModelSettings {
  id: string;
  name: string;
  facts: string[];
  enabled: boolean;
  active: boolean;
}
interface ProviderSettings {
  id: string;
  name: string;
  official: boolean;
  protocol: string | null;
  apiUrl: string | null;
  doc: string | null;
  hasKey: boolean;
  hasBlock: boolean;
  keyPath: string;
  envNames: string[];
  active: boolean;
  models: ModelSettings[];
}
interface CatalogProvider {
  id: string;
  name: string;
  protocol: string | null;
  doc: string | null;
  envNames: string[];
  modelCount: number;
}

// ---- Shell ----

const navEl = document.querySelector<HTMLElement>("#nav")!;
const paneEl = document.querySelector<HTMLElement>("#pane")!;

// The settings window is a singleton behind the native title bar (MOE-0029): the close button
// hides it instead of destroying it, so reopening from the tray / palette / Side View gear is
// instant and the section stays where it was.
void getCurrentWindow().onCloseRequested((event) => {
  event.preventDefault();
  void getCurrentWindow().hide();
});

const toastEl = document.createElement("div");
toastEl.className =
  "moe-bubble pointer-events-none absolute bottom-4 left-1/2 hidden -translate-x-1/2 items-center gap-1.5 rounded-full bg-surface-float px-3 py-1 text-xs text-fg";
document.querySelector<HTMLElement>("#root")!.append(toastEl);
let toastTimer: ReturnType<typeof setTimeout> | undefined;
function toast(text: string, icon = "check") {
  toastEl.replaceChildren(iconEl(icon, { size: 12 }), document.createTextNode(text));
  toastEl.classList.remove("hidden");
  toastEl.classList.add("flex");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    toastEl.classList.add("hidden");
    toastEl.classList.remove("flex");
  }, 2600);
}

type SectionId = "general" | "ai" | "commands";
const SECTIONS: { id: SectionId; title: string; icon: string }[] = [
  { id: "general", title: "General", icon: "settings-2" },
  { id: "ai", title: "AI", icon: "sparkles" },
  { id: "commands", title: "AI Commands", icon: "wand-2" },
];
let section: SectionId = "ai"; // the window exists for the providers (MOE-0029)

function renderNav() {
  navEl.replaceChildren(
    ...SECTIONS.map((entry) => {
      const button = document.createElement("button");
      button.className = "moe-settings-row";
      button.dataset.active = String(entry.id === section);
      button.append(
        iconEl(entry.icon, {
          size: 15,
          className: entry.id === section ? "text-fg" : "text-fg-subtle",
        }),
      );
      const title = document.createElement("span");
      title.className = "min-w-0 flex-1 truncate";
      title.textContent = entry.title;
      button.append(title);
      button.addEventListener("click", () => {
        if (section === entry.id) return;
        section = entry.id;
        renderNav();
        void renderPane();
      });
      return button;
    }),
  );
}

// ---- Small builders ----

function heading(title: string, blurb: string): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "mb-5";
  const h1 = document.createElement("h1");
  h1.className = "text-lg font-semibold text-fg";
  h1.textContent = title;
  const p = document.createElement("p");
  p.className = "mt-1 max-w-prose text-xs leading-relaxed text-fg-muted";
  p.textContent = blurb;
  wrap.append(h1, p);
  return wrap;
}

function card(): HTMLElement {
  const el = document.createElement("section");
  el.className = "mb-4 overflow-hidden rounded-2xl border border-line bg-surface-overlay";
  return el;
}

function badge(text: string, tone: "accent" | "muted" | "warning" | "success"): HTMLElement {
  const el = document.createElement("span");
  const tones = {
    accent: "border-accent-line bg-accent-soft text-accent",
    muted: "border-line bg-surface-hover text-fg-subtle",
    warning: "border-warning-line bg-warning-soft text-warning",
    success: "border-line bg-surface-hover text-success",
  };
  el.className = `shrink-0 rounded-full border px-2 py-0.5 text-[10px] font-medium ${tones[tone]}`;
  el.textContent = text;
  return el;
}

/**
 * A one-row glass capsule input — the composer row's language (MOE-0030 revision): the settings
 * window's key entries and searches wear the same capsule as the chat surfaces' draft, with the
 * inline error message riding inside the capsule's right end.
 */
function capsuleInput(options: { placeholder: string; type?: string }): {
  capsule: HTMLElement;
  input: HTMLInputElement;
  msg: HTMLSpanElement;
} {
  const capsule = document.createElement("div");
  capsule.className = "moe-composer-capsule min-w-0 flex-1";
  const input = document.createElement("input");
  input.type = options.type ?? "text";
  input.placeholder = options.placeholder;
  input.spellcheck = false;
  input.className =
    "min-w-0 flex-1 bg-transparent text-sm text-fg caret-accent placeholder-fg-subtle outline-none";
  const msg = document.createElement("span");
  msg.className = "shrink-0 text-[11px] text-danger";
  capsule.append(input, msg);
  return { capsule, input, msg };
}

/** A round glass button — the about chip's style, like the composer row's add/actions buttons. */
function roundButton(icon: string, title: string): HTMLButtonElement {
  const button = document.createElement("button");
  button.className = "moe-avatar-chip moe-focus-ring shrink-0";
  button.title = title;
  button.append(iconEl(icon, { size: 15 }));
  return button;
}

/** A native control in the app's clothes: the settings window is a real window, so the platform's
 *  own select/input widgets are the right ones here (the checkboxes already are, MOE-0029). */
const CONTROL_CLASS =
  "min-w-0 rounded-lg border border-line bg-surface-float px-2 py-1 text-xs text-fg outline-none focus-visible:ring-1 focus-visible:ring-accent-line disabled:opacity-50";

function field(label: string, control: HTMLElement, hint?: string): HTMLElement {
  const wrap = document.createElement("label");
  wrap.className = "flex min-w-36 flex-1 flex-col gap-1";
  const text = document.createElement("span");
  text.className = "text-[11px] font-medium text-fg-subtle";
  text.textContent = label;
  wrap.append(text, control);
  if (hint) {
    const note = document.createElement("span");
    note.className = "text-[10px] leading-snug text-fg-subtle";
    note.textContent = hint;
    wrap.append(note);
  }
  return wrap;
}

// ---- AI Commands pane (MOE-0062) ----

/**
 * One AI Command's row in the settings window: what it runs on at a glance, and — behind the
 * disclosure — the three things this window owns: the model pair, the reasoning effort and the
 * temperature. Every control writes one key of `[ai.commands."<id>"]` and re-renders, so the row
 * always shows what the file says (a failed write leaves the file, and the row, unchanged).
 */
interface CommandSettings {
  id: string;
  title: string;
  subtitle: string | null;
  icon: string | null;
  providerId: string | null;
  providerName: string | null;
  modelId: string | null;
  modelName: string | null;
  overridden: boolean;
  reasoning: string | null;
  reasoningSupported: boolean;
  temperature: number | null;
}

interface ModelChoice {
  providerId: string;
  providerName: string;
  modelId: string;
  modelName: string;
}

/** The effort ladder (MOE-0062); `""` is the provider's own default, which is not a value. */
const REASONING_LEVELS = ["off", "low", "medium", "high"];

async function renderCommands() {
  paneEl.replaceChildren(
    heading(
      "AI Commands",
      "Each AI Command runs on the active model unless it is given its own. Reasoning effort and temperature are per-command too — they are written to config.toml, one block per command, and a protocol without a reasoning knob says so.",
    ),
  );
  let commands: CommandSettings[] = [];
  let models: ModelChoice[] = [];
  try {
    commands = await invoke<CommandSettings[]>("ai_command_settings");
    models = await invoke<ModelChoice[]>("ai_model_choices");
  } catch (err) {
    toast(`Failed to load the commands: ${String(err)}`, "alert");
  }
  const list = card();
  const head = document.createElement("header");
  head.className = "border-b border-line px-4 py-2.5 text-xs font-medium text-fg-muted";
  head.textContent = "Per-command model and run settings";
  list.append(head);
  const ul = document.createElement("ul");
  for (const command of commands) ul.append(commandRow(command, models));
  list.append(ul);
  paneEl.append(list);
}

function commandRow(command: CommandSettings, models: ModelChoice[]): HTMLElement {
  const li = document.createElement("li");
  li.className = "border-b border-line last:border-b-0";

  const row = document.createElement("div");
  row.className = "flex items-center gap-3 px-4 py-2";
  row.append(
    iconEl(command.icon ?? "sparkles", { size: 15, className: "shrink-0 text-fg-subtle" }),
  );
  const names = document.createElement("div");
  names.className = "min-w-0 flex-1";
  const title = document.createElement("div");
  title.className = "truncate text-sm text-fg";
  title.textContent = command.title;
  const sub = document.createElement("div");
  sub.className = "mt-0.5 truncate text-[11px] text-fg-subtle";
  sub.textContent = command.modelName
    ? `${command.providerName} · ${command.modelName}`
    : "No model yet — the command shows its setup card";
  names.append(title, sub);
  row.append(names);
  if (command.overridden) row.append(badge("own model", "accent"));
  if (command.reasoning) row.append(badge(`reasoning: ${command.reasoning}`, "muted"));
  if (command.temperature !== null) row.append(badge(`temp ${command.temperature}`, "muted"));
  const toggle = roundButton("chevron-down", `Configure ${command.title}`);
  row.append(toggle);
  li.append(row);

  const editor = document.createElement("div");
  editor.className = "hidden flex-wrap items-start gap-3 border-t border-line px-4 py-3";
  editor.append(
    modelField(command, models),
    reasoningField(command),
    temperatureField(command),
  );
  toggle.addEventListener("click", () => {
    const hidden = editor.classList.contains("hidden");
    editor.classList.toggle("hidden", !hidden);
    editor.classList.toggle("flex", hidden);
  });
  li.append(editor);
  return li;
}

/** The model pair: the active pair (default) or one of the enabled models, grouped by provider. */
function modelField(command: CommandSettings, models: ModelChoice[]): HTMLElement {
  const select = document.createElement("select");
  select.className = CONTROL_CLASS;
  const active = document.createElement("option");
  active.value = "";
  active.textContent = "Active model (default)";
  select.append(active);
  const groups = new Map<string, HTMLOptGroupElement>();
  for (const choice of models) {
    let group = groups.get(choice.providerId);
    if (!group) {
      group = document.createElement("optgroup");
      group.label = choice.providerName;
      groups.set(choice.providerId, group);
      select.append(group);
    }
    const option = document.createElement("option");
    option.value = `${choice.providerId}\u0000${choice.modelId}`;
    option.textContent = choice.modelName;
    group.append(option);
  }
  select.value =
    command.overridden && command.providerId && command.modelId
      ? `${command.providerId}\u0000${command.modelId}`
      : "";
  select.addEventListener("change", () => {
    const [provider, model] = select.value ? select.value.split("\u0000") : [null, null];
    void invoke("ai_set_command_model", { command: command.id, provider, model })
      .then(() => {
        toast(`${command.title}: model saved`);
        return renderPane();
      })
      .catch((err) => {
        toast(`Failed to save: ${String(err)}`, "alert");
        return renderPane();
      });
  });
  return field("Model", select);
}

/** The reasoning effort: the provider's default, off, or one of the three levels. */
function reasoningField(command: CommandSettings): HTMLElement {
  const select = document.createElement("select");
  select.className = CONTROL_CLASS;
  const fallback = document.createElement("option");
  fallback.value = "";
  fallback.textContent = "Provider default";
  select.append(fallback);
  for (const level of REASONING_LEVELS) {
    const option = document.createElement("option");
    option.value = level;
    option.textContent = level.charAt(0).toUpperCase() + level.slice(1);
    select.append(option);
  }
  select.value = command.reasoning ?? "";
  select.disabled = !command.reasoningSupported;
  select.addEventListener("change", () => {
    void invoke("ai_set_command_reasoning", {
      command: command.id,
      reasoning: select.value === "" ? null : select.value,
    })
      .then(() => {
        toast(`${command.title}: reasoning saved`);
        return renderPane();
      })
      .catch((err) => {
        toast(`Failed to save: ${String(err)}`, "alert");
        return renderPane();
      });
  });
  return field(
    "Reasoning effort",
    select,
    command.reasoningSupported
      ? undefined
      : "This protocol has no reasoning knob: reasoning is a property of the model.",
  );
}

/** The temperature: empty = the key is not sent (the only safe default for reasoning models). */
function temperatureField(command: CommandSettings): HTMLElement {
  const input = document.createElement("input");
  input.type = "number";
  input.min = "0";
  input.max = "2";
  input.step = "0.1";
  input.placeholder = "Provider default";
  input.value = command.temperature === null ? "" : String(command.temperature);
  input.className = `${CONTROL_CLASS} w-32`;
  input.addEventListener("change", () => {
    const raw = input.value.trim();
    void invoke("ai_set_command_temperature", {
      command: command.id,
      temperature: raw === "" ? null : Number(raw),
    })
      .then(() => {
        toast(`${command.title}: temperature saved`);
        return renderPane();
      })
      .catch((err) => {
        toast(`Failed to save: ${String(err)}`, "alert");
        return renderPane();
      });
  });
  return field("Temperature", input, "Empty = the key is not sent at all.");
}

// ---- General pane ----

async function renderGeneral() {
  paneEl.replaceChildren(
    heading("General", "Appearance and the config file. The AI section manages providers and models."),
  );
  const theme = await invoke<string>("ui_theme").catch(() => "system");
  const appearance = card();
  const head = document.createElement("header");
  head.className = "border-b border-line px-4 py-2.5 text-xs font-medium text-fg-muted";
  head.textContent = "Appearance";
  appearance.append(head);
  const rows: { id: string; title: string; icon: string }[] = [
    { id: "system", title: "System", icon: "monitor" },
    { id: "light", title: "Light", icon: "sun" },
    { id: "dark", title: "Dark", icon: "moon" },
  ];
  for (const row of rows) {
    const button = document.createElement("button");
    button.className = "moe-settings-row";
    button.append(iconEl(row.icon, { size: 15, className: "text-fg-subtle" }));
    const title = document.createElement("span");
    title.className = "min-w-0 flex-1 truncate";
    title.textContent = row.title;
    button.append(title);
    if (row.id === theme) button.append(iconEl("check", { size: 14, className: "text-accent" }));
    button.addEventListener("click", () => {
      void invoke("set_theme", { theme: row.id })
        .then(() => renderPane())
        .catch((err) => toast(`Failed to set theme: ${String(err)}`, "alert"));
    });
    appearance.append(button);
  }
  const config = card();
  const configRow = document.createElement("button");
  configRow.className = "moe-settings-row";
  configRow.append(iconEl("file-text", { size: 15, className: "text-fg-subtle" }));
  const configTitle = document.createElement("span");
  configTitle.className = "min-w-0 flex-1 truncate";
  configTitle.textContent = "Open Config File";
  const configSub = document.createElement("span");
  configSub.className = "shrink-0 text-[11px] text-fg-subtle";
  configSub.textContent = "summon key, [ai] overrides, [ui]";
  configRow.append(configTitle, configSub);
  configRow.addEventListener("click", () => {
    void invoke("invoke_command", {
      commandId: "moe.open-config",
      query: null,
      record: false,
    }).catch((err) => toast(`Failed to open config: ${String(err)}`, "alert"));
  });
  config.append(configRow);
  paneEl.append(appearance, config);
}

// ---- AI pane: Models & Providers ----

async function renderAi() {
  paneEl.replaceChildren(
    heading(
      "Models & Providers",
      "The providers this Moe uses, from the model catalog and your config. Check the models that should appear in the model pickers, click a model to make it the active pair, and keep each provider's key in its own file.",
    ),
  );
  let providers: ProviderSettings[] = [];
  try {
    providers = await invoke<ProviderSettings[]>("ai_settings_providers");
  } catch (err) {
    const error = document.createElement("p");
    error.className = "text-xs text-danger";
    error.textContent = `Failed to load providers: ${String(err)}`;
    paneEl.append(error);
    return;
  }
  if (providers.length === 0) {
    const empty = document.createElement("p");
    empty.className = "mb-4 max-w-prose text-xs leading-relaxed text-fg-muted";
    empty.textContent =
      "No provider yet: connect one below (a key is all an official provider needs), or write a [ai.providers.<id>] block in config.toml.";
    paneEl.append(empty);
  }
  for (const provider of providers) paneEl.append(providerCard(provider));
  paneEl.append(connectCard());
  paneEl.append(await webSearchCard());
}

/** One provider group: header (name, protocol, key state) + the model rows. */
function providerCard(provider: ProviderSettings): HTMLElement {
  const el = card();
  const header = document.createElement("header");
  header.className = "flex flex-wrap items-center gap-2 border-b border-line px-4 py-3";
  header.append(providerMark(provider.id, provider.name, 20));
  const names = document.createElement("div");
  names.className = "min-w-0 flex-1";
  const title = document.createElement("div");
  title.className = "flex items-center gap-2";
  const name = document.createElement("span");
  name.className = "truncate text-sm font-medium text-fg";
  name.textContent = provider.name;
  title.append(name);
  if (provider.active) title.append(badge("Active", "accent"));
  if (!provider.official) title.append(badge("custom", "muted"));
  const sub = document.createElement("div");
  sub.className = "mt-0.5 truncate text-[11px] text-fg-subtle";
  const subParts = [provider.id, provider.protocol, provider.apiUrl].filter(
    (part): part is string => Boolean(part),
  );
  sub.textContent = subParts.join(" · ");
  names.append(title, sub);
  header.append(names);

  const keyButton = document.createElement("button");
  keyButton.className =
    "moe-focus-ring shrink-0 rounded-full border border-line px-2.5 py-1 text-[11px] text-fg-muted transition-colors hover:bg-surface-hover hover:text-fg";
  keyButton.textContent = provider.hasKey ? "Replace Key…" : "Add Key…";

  // Remove provider (MOE-0032): key file + config block + the active pair when it pointed here.
  // Two-step inline confirmation — the header's right side swaps to the question.
  const removeButton = roundButton("trash-2", "Remove provider");
  const isLegacySynthetic =
    provider.id === "default" && !provider.official && !provider.hasBlock;
  const confirmRemove = () => {
    const question = document.createElement("span");
    question.className = "shrink-0 text-[11px] text-fg-muted";
    question.textContent = `Remove ${provider.name}?`;
    const yes = document.createElement("button");
    yes.className =
      "moe-focus-ring shrink-0 rounded-full bg-warning-fill px-2.5 py-1 text-[11px] text-on-warning transition-colors hover:bg-warning-fill-hover";
    yes.textContent = "Remove";
    const no = document.createElement("button");
    no.className =
      "moe-focus-ring shrink-0 rounded-full border border-line px-2.5 py-1 text-[11px] text-fg-muted transition-colors hover:bg-surface-hover hover:text-fg";
    no.textContent = "Cancel";
    const row = document.createElement("div");
    row.className = "flex shrink-0 items-center gap-2";
    row.append(question, yes, no);
    header.replaceChild(row, controls);
    no.addEventListener("click", () => void renderPane());
    yes.addEventListener("click", () => {
      void invoke("ai_remove_provider", { provider: provider.id })
        .then(() => {
          toast(`Removed ${provider.name}`, "trash-2");
          return renderPane();
        })
        .catch((err) => {
          toast(`Failed to remove: ${String(err)}`, "alert");
          return renderPane();
        });
    });
  };
  removeButton.addEventListener("click", confirmRemove);

  // The key-missing toast from a model click also opens this row (MOE-0032).
  const gateActivation = () => {
    toast(`Save a key for ${provider.name} first`, "alert");
    toggleKeyRow(true);
  };
  // `controls` is what the inline confirmation swaps out; declared before confirmRemove runs.
  const controls = document.createElement("div");
  controls.className = "flex shrink-0 items-center gap-2";
  controls.append(
    badge(provider.hasKey ? "key saved" : "key missing", provider.hasKey ? "success" : "warning"),
    keyButton,
  );
  if (!isLegacySynthetic) controls.append(removeButton);
  header.append(controls);
  el.append(header);

  // The key entry row: a capsule password field + a round save button, writing the provider's
  // own 0600 file (ADR-0019) — the composer row's language (MOE-0030 revision).
  const keyRow = document.createElement("div");
  keyRow.className = "hidden items-center gap-2 border-b border-line px-4 py-2.5";
  const keyField = capsuleInput({
    type: "password",
    placeholder: provider.envNames.length
      ? `Paste the key (or set ${provider.envNames[0]})`
      : "Paste the key",
  });
  const keySave = roundButton("check", "Save key");
  keyRow.append(keyField.capsule, keySave);
  const keyInput = keyField.input;
  const keyMsg = keyField.msg;
  const toggleKeyRow = (show: boolean) => {
    keyRow.classList.toggle("hidden", !show);
    keyRow.classList.toggle("flex", show);
    if (show) keyInput.focus();
  };
  keyButton.addEventListener("click", () => toggleKeyRow(keyRow.classList.contains("hidden")));
  const saveKey = async () => {
    const value = keyInput.value.trim();
    if (!value) {
      keyMsg.textContent = "empty key";
      return;
    }
    try {
      await invoke("ai_save_provider_key", { provider: provider.id, key: value });
      toast(`Key saved for ${provider.name}`);
      keyInput.value = "";
      await renderPane();
    } catch (err) {
      keyMsg.textContent = String(err);
    }
  };
  keySave.addEventListener("click", () => void saveKey());
  keyInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      void saveKey();
    } else if (e.key === "Escape") {
      e.preventDefault();
      toggleKeyRow(false);
    }
  });
  el.append(keyRow);

  if (provider.models.length === 0) {
    const note = document.createElement("p");
    note.className = "px-4 py-3 text-xs leading-relaxed text-fg-subtle";
    note.textContent = provider.official
      ? "The catalog lists no models for this provider."
      : "This custom provider lists no models: add them under models = […] in its config block.";
    el.append(note);
  }
  const list = document.createElement("ul");
  for (const model of provider.models) list.append(modelRow(provider, model, gateActivation));
  el.append(list);

  if (provider.doc) {
    const footer = document.createElement("div");
    footer.className = "border-t border-line px-4 py-2";
    const link = document.createElement("a");
    link.href = provider.doc;
    link.target = "_blank";
    link.rel = "noreferrer";
    link.className = "text-[11px] text-accent hover:underline";
    link.textContent = "Provider docs";
    footer.append(link);
    el.append(footer);
  }
  return el;
}

/** One model row: click the name to activate the pair (a keyed provider only); the checkbox is
 *  the picker visibility. */
function modelRow(
  provider: ProviderSettings,
  model: ModelSettings,
  onNeedsKey: () => void,
): HTMLElement {
  const li = document.createElement("li");
  li.className = "flex items-center gap-3 px-4 py-1.5";

  const main = document.createElement("button");
  main.className = "moe-focus-ring flex min-w-0 flex-1 items-center gap-2 rounded-lg px-1.5 py-1 text-left transition-colors hover:bg-surface-hover";
  if (model.active) {
    main.append(iconEl("check", { size: 13, className: "shrink-0 text-accent" }));
  } else {
    const dot = document.createElement("span");
    dot.className = "mx-[3px] my-[5px] shrink-0 rounded-full border border-line-strong";
    dot.style.width = "7px";
    dot.style.height = "7px";
    main.append(dot);
  }
  const name = document.createElement("span");
  name.className = "truncate text-sm text-fg";
  name.textContent = model.name;
  main.append(name);
  for (const fact of model.facts.slice(1)) {
    // The context window rides the column; the chips carry the capabilities
    main.append(badge(fact, "muted"));
  }
  main.title = model.active
    ? `${model.name} is the active model`
    : `Make ${model.name} the active model`;
  main.addEventListener("click", () => {
    if (model.active) return;
    // A model cannot run without its provider's key: the row opens the key entry instead of
    // silently activating a pair that would fail on its first request (MOE-0032).
    if (!provider.hasKey) {
      onNeedsKey();
      return;
    }
    void invoke("ai_set_pair", { provider: provider.id, model: model.id })
      .then(() => {
        toast(`${provider.name} · ${model.name} is now the active model`);
        return renderPane();
      })
      .catch((err) => toast(`Failed to switch: ${String(err)}`, "alert"));
  });

  const context = document.createElement("span");
  context.className = "w-12 shrink-0 text-right text-xs text-fg-subtle";
  context.textContent = model.facts[0] ?? "";

  const check = document.createElement("input");
  check.type = "checkbox";
  check.className = "moe-check shrink-0";
  check.checked = model.enabled;
  check.title = model.enabled
    ? "Shows in the model pickers"
    : "Hidden from the model pickers";
  check.addEventListener("change", () => {
    const enabled = check.checked;
    void invoke("ai_set_model_visibility", {
      provider: provider.id,
      model: model.id,
      enabled,
    }).catch((err) => {
      check.checked = !enabled; // roll the checkbox back; the file did not change
      toast(`Failed to update visibility: ${String(err)}`, "alert");
    });
  });

  li.append(main, context, check);
  return li;
}

/** "Connect a provider": search the catalog providers this install does not use yet; saving a
 *  key connects one (official providers need no config block). */
function connectCard(): HTMLElement {
  const el = card();
  const header = document.createElement("header");
  header.className = "border-b border-line px-4 py-2.5 text-xs font-medium text-fg-muted";
  header.textContent = "Connect a provider";
  const body = document.createElement("div");
  body.className = "p-2";
  const search = capsuleInput({ placeholder: "Search catalog providers…" });
  search.capsule.classList.add("mb-1");
  const list = document.createElement("ul");
  list.className = "max-h-56 overflow-y-auto";
  body.append(search.capsule, list);
  el.append(header, body);
  const input = search.input;

  let results: CatalogProvider[] = [];
  const render = () => {
    list.replaceChildren(
      ...results.map((provider) => {
        const li = document.createElement("li");
        const row = document.createElement("div");
        row.className = "moe-settings-row";
        row.append(iconEl("sparkles", { size: 14, className: "text-fg-subtle" }));
        const name = document.createElement("span");
        name.className = "min-w-0 flex-1 truncate";
        name.textContent = provider.name;
        row.append(name);
        if (provider.protocol) row.append(badge(provider.protocol, "muted"));
        const count = document.createElement("span");
        count.className = "shrink-0 text-[11px] text-fg-subtle";
        count.textContent = `${provider.modelCount} models`;
        row.append(count);
        li.append(row);

        // Saving a key connects the provider: its group appears on the next load.
        const keyRow = document.createElement("div");
        keyRow.className = "hidden items-center gap-2 px-2 py-1.5";
        const keyField = capsuleInput({
          type: "password",
          placeholder: `Paste the ${provider.name} key`,
        });
        const keySave = roundButton("check", "Save key");
        keyRow.append(keyField.capsule, keySave);
        li.append(keyRow);
        const keyInput = keyField.input;
        const keyMsg = keyField.msg;

        const showKey = (show: boolean) => {
          keyRow.classList.toggle("hidden", !show);
          keyRow.classList.toggle("flex", show);
          if (show) keyInput.focus();
        };
        row.addEventListener("click", () => showKey(keyRow.classList.contains("hidden")));
        const save = async () => {
          const value = keyInput.value.trim();
          if (!value) {
            keyMsg.textContent = "empty key";
            return;
          }
          try {
            await invoke("ai_save_provider_key", { provider: provider.id, key: value });
            toast(`Key saved for ${provider.name}`);
            await renderPane();
          } catch (err) {
            keyMsg.textContent = String(err);
          }
        };
        keySave.addEventListener("click", () => void save());
        keyInput.addEventListener("keydown", (e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            void save();
          } else if (e.key === "Escape") {
            e.preventDefault();
            showKey(false);
          }
        });
        return li;
      }),
    );
    if (results.length === 0) {
      const empty = document.createElement("li");
      empty.className = "px-2.5 py-2 text-[11px] text-fg-subtle";
      empty.textContent = input.value.trim()
        ? "No catalog provider matches."
        : "Every catalog provider with a key is already listed above.";
      list.append(empty);
    }
  };

  const load = async (query: string) => {
    try {
      results = await invoke<CatalogProvider[]>("ai_catalog_providers", { query });
    } catch {
      results = [];
    }
    render();
  };
  input.addEventListener("input", () => void load(input.value));
  void load("");
  return el;
}

// ---- AI pane: Web Search (the agent's SearXNG tool, MOE-0034) ----

/**
 * The SearXNG instance row: a URL capsule + Test + Save. Saving an empty value removes the key
 * again — unset means the model is never offered `web_search` at all (tools.rs `specs`).
 */
async function webSearchCard(): Promise<HTMLElement> {
  const el = card();
  const header = document.createElement("header");
  header.className = "flex items-center gap-2 border-b border-line px-4 py-2.5";
  header.append(iconEl("globe", { size: 15, className: "text-fg-subtle" }));
  const title = document.createElement("span");
  title.className = "min-w-0 flex-1 text-xs font-medium text-fg-muted";
  title.textContent = "Web Search";
  header.append(title);

  let current: string | null = null;
  try {
    current = await invoke<string | null>("ai_searxng_url");
  } catch {
    current = null;
  }
  header.append(badge(current ? "configured" : "off", current ? "success" : "muted"));

  const row = document.createElement("div");
  row.className = "flex items-center gap-2 px-4 py-3";
  const field = capsuleInput({ placeholder: "http://localhost:8080" });
  field.input.value = current ?? "";
  field.msg.className += " max-w-56 truncate";
  const testButton = roundButton("search", "Test the instance");
  const saveButton = roundButton("check", "Save");
  row.append(field.capsule, testButton, saveButton);

  const sub = document.createElement("p");
  sub.className = "border-t border-line px-4 py-2 text-[11px] leading-relaxed text-fg-subtle";
  sub.textContent =
    "A self-hosted SearXNG instance with the JSON format enabled — when set, conversations may call web_search (chat and Anthropic protocol providers; the tool needs no API key). Save an empty value to turn web search off.";

  el.append(header, row, sub);

  const input = field.input;
  const msg = field.msg;
  const say = (text: string) => {
    msg.textContent = text;
    msg.title = text;
  };
  /** The trimmed field value, or null when it is neither empty nor an http(s) URL. */
  const checkedUrl = (): string | null => {
    const value = input.value.trim();
    if (value && !/^https?:\/\//i.test(value)) {
      say("http(s) URL");
      return null;
    }
    say("");
    return value;
  };
  const save = async () => {
    const value = checkedUrl();
    if (value === null) return;
    try {
      await invoke("ai_set_searxng_url", { url: value || null });
      toast(value ? "Web search saved" : "Web search turned off");
      await renderPane();
    } catch (err) {
      say(String(err));
    }
  };
  const test = async () => {
    const value = checkedUrl();
    if (value === null) return;
    if (!value) {
      say("nothing to test");
      return;
    }
    for (const button of [testButton, saveButton]) {
      button.disabled = true;
      button.classList.add("opacity-50");
    }
    try {
      toast(await invoke<string>("ai_test_searxng", { url: value }), "search");
    } catch (err) {
      say(String(err));
    } finally {
      for (const button of [testButton, saveButton]) {
        button.disabled = false;
        button.classList.remove("opacity-50");
      }
    }
  };
  saveButton.addEventListener("click", () => void save());
  testButton.addEventListener("click", () => void test());
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      void save();
    }
  });
  return el;
}

// ---- Pane routing ----

async function renderPane() {
  if (section === "general") await renderGeneral();
  else if (section === "commands") await renderCommands();
  else await renderAi();
}

window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    e.preventDefault();
    void getCurrentWindow().hide();
  }
});

renderNav();
void renderPane();
