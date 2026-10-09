# Moe

A keyboard-first desktop command palette: double-tap ⌘ in any app to summon a centered panel,
search commands, and write results back to the selection or cursor. Product language lives in
`CONTEXT.md`; every design decision lives in `docs/adr/0001–0030`.

## Install (macOS)

Requires **macOS 15 or newer** (the glass material and its accessibility fallbacks target Safari 18+).

1. Download `Moe_<version>_<arch>.dmg` from the GitHub Actions **Release** workflow
   (Actions → Release → the run → Artifacts; triggered by pushing a `v*` tag or manually).
2. Open the dmg and drag **Moe.app** into Applications.
3. **Unsigned / unnotarized**: on first launch right-click Moe.app → Open → Open again
   (a one-time Gatekeeper confirmation for unnotarized apps; normal double-click works after that).
4. First run: the Moe tray icon appears; the panel guides you through granting
   **Input Monitoring** (the gate for double-tap ⌘) and **Accessibility** (the gate for
   reading the selection / writing back). Both take effect immediately after granting.

## Usage

Double-tap ⌘ to summon the panel (it appears on the screen under the mouse cursor, horizontally
centered near the top, ADR-0032); typing searches commands. One set of keybinding semantics
spans every extension:

On an empty query, root results are **grouped by source** (ADR-0020): one section per extension,
headed by the extension name (headers are not focusable); the section order is **declared**
(`set_source_order` in `install()`: AI Commands → AI → Moe → Echo, ADR-0020 amendment — not
usage-derived), and items keep their frecency order within a group.
**Typing a query merges everything into one "Results" section in match-score order**
(ADR-0033 — the row still names its extension, ADR-0030). Each row reads (ADR-0030): **command
name** (never prefixed with the extension
name) · **extension name** · the command's declared shortcut as a Kbd (e.g. Open Config File's
`⌘,`, revealed while the row is focused or hovered, ADR-0031) · a trailing **kind badge**
("Command" / "AI Command"). On an empty query two pinned
sections sit on top: **Favorites** (ADR-0029 — the
commands you starred via the root actions card, in the order you added them) and then
**Suggestions** (ADR-0023 — the most recently used commands, at most 5, most recent first);
nothing pinned is repeated in the sections below, and both are hidden while empty.
**⌃X forgets the focused suggestion, ⌃⇧X clears all recent usage** (ADR-0025) — the commands
themselves stay listed in their own sections.

| Keys | Semantics | Notes |
|---|---|---|
| `↓` / `⌃N`, `↑` / `⌃P` | Navigate | Move the Focused Item |
| `⏎` | Apply | Run the primary action on the Focused Item; **while an answer streams the primary yields to Stop Generation, so `⏎` stops it** (every surface — result cards and the Quick Ask page alike); on the Quick Ask page it sends the draft when no stream is running (ADR-0036 amendment) |
| `⌥⏎` | Secondary | Default semantics: copy (e.g. copy the full AI answer to the clipboard) |
| `⌘K` | Show All Actions | The current item's primary/secondary action list, on every surface (`⌃K` stays the macOS kill-line, ADR-0031) |
| `⌘P` | Browse | The current extension's record list (AI = chat history; a hint if the extension declares none) |
| `⌘N` | New | Create a new record (AI = new chat; a hint if the extension declares none) |
| `⌃X` | Delete | Delete the current record (AI = current conversation; results layer, Quick Ask page and Suggestions only, ADR-0022/0025/0036) |
| `⌃⇧X` | DeleteAll | Delete all records (AI = all conversations; also clears all suggestions, ADR-0022/0025) |
| `⌘J` | Materialize | Move the current conversation into the extension's Side View |
| `⌃[` / `⌃]` | Step conversations | Quick Ask page and Side View: previous / next conversation in the chat history (ADR-0036) |
| `⌘⇧A` | Attach | Type/paste a file path, inserted as `@"path"` (ADR-0010) |
| `⌘⇧K` | About | Open the About card (Open Config File / Save AI Key / Send Feedback; the avatar chip's menu, ADR-0027) |
| `⌘⇧F` | Favorite | Add/remove the current command from Favorites (root: the focused command; inside a command: the source command, ADR-0029) |
| `⌘,` | OpenConfig | Open the config file (the macOS Preferences convention; also the About card's first row, ADR-0027) |
| `Esc` | Layered back | While generating → a confirmation dialog first (every streaming surface, ADR-0036/0038); otherwise actions card → root (a result page backs out in one step) → clear input → close the panel; on split pages the detail never collapses, Back goes straight to root |
| `⌫` | Layered back | Non-empty input = normal delete; **empty input steps back one layer** (like `Esc` — including the confirmation dialog while a result streams — but the root layer never closes the panel, ADR-0017) |

A **bottom bar** floats over the list, exactly one row tall (ADR-0026) — the left chip and the
right pill line up with the last visible row band, and the chip's avatar is centered on the item
icons:

- **Avatar chip (bottom-left)**: the About button's own icon
  (`message-circle-warning`) on the root page; once you are inside a command it shows that
  extension's icon and name. It doubles as the **toast host**: toasts expand the
  chip into a pill (Raycast-style) instead of popping up anywhere else.
- **Action pill (bottom-right)**: **primary action** (the focused item's main action; becomes
  "Stop Generation" while generating) and **actions** (`⌘K`). `⌘K` pops up the **actions
  card**: a floating overlay that never replaces the main body, its sections loaded automatically
  per page from registered sources (ADR-0037) — **Actions** (the focused item's own actions on
  every page shape, or the shown conversation's: Open in Side View / Copy Last Answer / Write
  Back), **Command** (the page command's own menu: **Open Command** first on the root — the same
  Apply as Enter, with the `⏎` Kbd — then **Add to / Remove from Favorites** (`⌘⇧F`, ADR-0029),
  **Remove from Suggestions** on a suggestion (`⌃X`, ADR-0025), and the dimmed **Configure
  Extension** placeholder), and **General** — the platform slots live on that page: **Browse
  Records / Chats** (`⌘P`) and **New Record / Chat** (`⌘N`) when the extension declares them,
  plus **Delete Record** (`⌃X`) / **Delete All Records** (`⌃⇧X`) — or **Remove Chat / Remove All
  Chats** on the conversation page — wherever the page carries records (ADR-0014/0022).
  Sections flatten while the search input at the card's bottom filters,
  `↑↓` / `⌃N` `⌃P` select, `⏎` run, `Esc` or empty `⌫` dismiss (ADR-0015, ADR-0026 amendment).
  While a card is open it owns the keyboard: navigation and `⌫` act on the card, never on the
  list below (ADR-0031). The Side
  View's **More Actions** is this same card (ADR-0028) with this surface's landing spots
  (**Chat**: Chat History / New Chat / Remove Conversation / Remove All Conversations; **App**:
  Open Config File; **Window**: Hide Side View).
- **About card**: clicking the avatar chip (or `⌘⇧K`, ADR-0027) opens the app's About menu — a
  card with the same UX as the actions card (search input at the bottom): **App** holds
  **Open Config File** (`⌘,`) / **Save AI Key**, **Theme** holds **System** / **Light** / **Dark**
  (applied immediately, persisted to config.toml, the active row checked, ADR-0035), **Support**
  holds **Send Feedback**; rows show their Kbd when a binding exists. `↑↓` / `⌃N` `⌃P` select,
  `⏎` run, `Esc` or empty `⌫` dismiss, clicking outside closes it.

Every shortcut is rendered as Kbd blocks, one key per block.

- **Page shapes**: exactly three — **list** / **split** (narrow list on the left + detail on the
  right) / **full-screen detail** — laid out and window-sized by the platform (ADR-0018). An
  extension gets the split page for free by providing `item.detail` (left list fixed at 280px,
  detail fills the rest with the item's icon/title/time on top); the panel shows exactly ten rows
  plus one section header (ADR-0026) and is measured to fit them (768×510 for the default input bar).
- **Panel**: auto-dismisses on losing focus; input and results are kept across hide/show. The
  tray icon offers Show Panel / AI Chat / Launch at Login / Open Config File / Quit. The results
  layer has two shapes declared by the command (ADR-0013): full-screen detail (AI answers,
  notices) fills the panel and shows an inline three-dot indicator while generating; everything
  else (like AI history search) is a list on the left with a detail preview on the right.
- **AI Chat (Quick Ask)**: typing a question with nothing matching offers the fallback **Ask "…"**
  (`⏎` sends it); applying **Quick Ask** itself (e.g. typing `quick`) opens the **conversation page**
  in the panel instead — the search text is never sent as a question (ADR-0036). On the page the
  Input Bar is the composer: **`⏎` sends** (input cleared) and answers stream into the conversation,
  the page following the answer down as it streams;
  **`⏎` while generating stops** (Back pops a confirmation first); **`⌘N`** starts a new blank chat,
  **`⌃X`** removes the current one (the chat history's own deletion), **`⌃[` / `⌃]`** step through
  history, **`⌘P`** opens the history list, **`⌘J`** continues in the Side View, `⌥⏎` copies the last
  answer and `⌘K` groups the conversation's actions (Open in Side View / Copy Last Answer / Write
  Back) with the page's slots (Browse Chats / New Chat / Remove Chat / Remove All Chats). **Text
  selected before summoning automatically becomes
  context for the conversation's first message** (not repeated when the question already contains
  it, truncated when overlong; history stores only the question itself). **Files selected in Finder
  before summoning automatically become attachments** (deduplicated with `@path` by path; the first
  use asks for Automation permission, ADR-0021).
- **AI Commands (text transforms, ADR-0024)**: 12 one-shot transform commands — Improve Writing /
  Fix Spelling & Grammar / Make Shorter / Make Longer / Simplify Language / Summarize /
  Translate to English / Translate to Chinese / Tone: Professional / Tone: Friendly /
  Extract Key Ideas / Continue Writing. **Select text, summon, type the command name (e.g.
  "improve") and press Enter**: the result streams into a result card and **automatically
  replaces the selection and dismisses the panel when done**; with no selection the command guides
  you to select the text first — the search text is never transformed (ADR-0036) and is cleared from
  the Input Bar when the command opens. **While the result streams, `⏎` stops the generation** (the
  primary action yields to Stop, ADR-0036 amendment) and **Back (`Esc` / empty `⌫`) asks first** — an
  accidental Back must not kill the generation (ADR-0038); with the search text gone, Back leaves the
  result page in one step. After the stream has stopped or finished, `⏎` writes back manually and
  `⌥⏎` copies — a stopped generation keeps what was generated and does not write back by itself.
- **Side View (AI chat)**: a window on the right with no persistent history bar. Three icon
  buttons sit at the end of the header: **More Actions (⌘ icon / `⌘K`)**, **History**, and
  **New Chat**. **More Actions** is the panel's actions card in this window (ADR-0028/0037): the same
  registered sections, filter input, `↑↓`/`⌃N` `⌃P`/`⏎` selection and `Esc` dismissal — with the
  **Chat** section (Chat History / New Chat / Remove Conversation / Remove All Conversations),
  **App** (Open Config File) and **Window** (Hide Side View) —
  floating centered nearly at the top, its filter input on top. Clicking
  History (or `⌘P`) pops up a **floating history card** centered at the top: its top input
  filters titles, the list below highlights with `↑` `↓` (or `⌃N` `⌃P`),
  `⏎` opens, `Esc` dismisses, and clicking outside also dismisses it. `⌘P` (Browse) / `⌘K`
  (Actions) / `⌘N` (New) / `⌃X` (Delete) / `⌃⇧X` (DeleteAll) are the general actions spanning
  every command/sub-app (ADR-0014/0022); in the Side View they land on the history card / actions
  card / new chat / delete current conversation / delete all conversations (while the history card
  is open they act on the focused row; deleting the current conversation returns to the empty
  state). In the panel they land on the extension's declared record list / actions layer / new
  record / delete hooks (an error toast when the extension implements none; on the Quick Ask page
  they land on history / chat actions / new chat / remove chat, ADR-0036). `⌃[` / `⌃]` step
  through the current conversation list (Quick Ask page and Side View); opening/switching a conversation scrolls to the bottom
  of the last line; `⏎` sends / `⇧⏎` newline / `Esc` dismisses overlays first, stops while
  generating, otherwise hides; 📎 adds attachments; window position and size are remembered after
  dragging. Past conversations are also reachable from the command "Search Chat History" (AI)
  (narrow list on the left + detail on the right, showing the last answer).
- **Config**: run "Open Config File" or edit
  `~/Library/Application Support/moe/config.toml` directly:

  ```toml
  [summon]
  key = "double-cmd"   # double-cmd | double-option | double-ctrl | a combo such as cmd+shift+space
  double_tap_ms = 400  # 100..=1000

  [ui]
  theme = "system"     # system | light | dark (system follows the OS; also switchable from the About card, ADR-0035)

  [ai]
  base_url = "https://api.deepseek.com/v1"  # any OpenAI-compatible endpoint
  model = "deepseek-chat"
  ```

  The API key lives in `~/Library/Application Support/moe/api-key` (0600, never echoed;
  an unsigned app triggers repeated keychain prompts, see ADR-0019). Type `key <your-key>` in
  the panel, or set the `MOE_AI_API_KEY` environment variable; keys stored by older versions
  in the keychain migrate to the file automatically.

## Structure

| Path | Responsibility |
|---|---|
| `crates/moe-core` | Extension / Command / Item contract (ADR-0006), unified keymap, Registry and search |
| `crates/moe-platform` | Platform boundary (ADR-0002/0008): TextTarget, clipboard, summon listener (macOS CGEventTap / Linux X11 XRecord) |
| `crates/moe-extensions` | Built-in extensions (Moe settings commands, Echo contract demo, AI chat, AI Commands) |
| `crates/moe-app` | Tauri 2 shell: panel/Side View windows, tray, IPC |
| `ui/` | Thin vanilla TS + Tailwind view layer (ADR-0007) |

## Development

```sh
pnpm -C ui install
cargo test                                     # contract and platform tests
cd crates/moe-app && cargo tauri dev           # run the panel in development (recommended: starts Vite + hot reload)
```

⚠️ A plain `cargo run -p moe-app` is a **dev build**: the WebView connects to `devUrl`
(http://localhost:1420), so without the Vite dev server you get a fully transparent empty window
(it looks like nothing started). Working options:

1. `cd crates/moe-app && cargo tauri dev` (recommended)
2. Two terminals: `pnpm -C ui dev` + `cargo run -p moe-app`
3. Production build: `cd crates/moe-app && cargo tauri build` (embeds `ui/dist`, produces .app/.dmg)

Icon sources: `python3 crates/moe-app/icons/gen-app-icon.py` (app icon, then run
`cargo tauri icon`) and `gen-tray-icon.py` (menu bar template).

## Current feature set

- **Summon**: double-tap ⌘ (macOS CGEventTap, needs Input Monitoring; Linux/X11 uses XRecord,
  no permission needed).
- **Text**: the selection is captured before summoning; `WriteBack` replaces the selection or
  inserts at the cursor via AX, falling back to "clipboard snapshot → synthesize ⌘V → restore"
  when denied (demo command `Echo: Shout`).
- **Search**: nucleo fuzzy matching (exact prefix > match position) with frecency breaking ties;
  the empty query lists everything by frecency with a recent-commands Suggestions section on top.
- **AI**: streaming chat against any OpenAI-compatible endpoint; conversations and messages live
  in local SQLite (`data_dir/moe/moe.db`, `ai` namespace); attachments (text inlined / images
  multimodal); Side View continuation uses the whole history as context; AI Commands transform
  text and write the result back automatically.
- **Resident**: menu bar tray (with Launch at Login via LaunchAgent); no Dock icon on macOS,
  not in ⌘-Tab.
- **UI**: one keybinding semantics set (`↓`/`⌃N`, `↑`/`⌃P`, `⏎`, `⌥⏎`, `⌘K`, `⌘J`, `Esc`)
  spans every command; the results layer has two shapes declared by the command (full-screen
  detail / list + detail, ADR-0013); icons are Lucide (ADR-0012); two themes (light / dark) behind
  one semantic token layer, following the OS appearance, overridable with `[ui] theme` (ADR-0035).

## FAQ

- **Double-tap ⌘ does nothing**: check the guidance banner at the top of the panel — usually
  Input Monitoring hasn't been granted (System Settings → Privacy & Security). On some macOS
  versions a restart of Moe is needed after granting.
- **Write-back does nothing / asks for permission**: Accessibility needs to be granted; some apps
  (e.g. certain Electron apps) expose read-only AX, in which case the clipboard fallback kicks in.
- **"Developer cannot be verified" on first open**: unsigned app — right-click → Open (see Install).
- **Wayland**: global keyboard interception is impossible (XRecord only sees XWayland clients).
  Two alternative paths: 1) use a combo key in `config.toml` (`[summon] key = "cmd+shift+space"`);
  2) bind `moe --toggle` in the window manager (single-instance forwarding: toggles the panel when
  already running, otherwise starts and shows it):

  ```conf
  # Hyprland (~/.config/hypr/hyprland.conf)
  bind = SUPER, M, exec, moe --toggle
  ```

  ```conf
  # i3 (~/.config/i3/config) / sway (~/.config/sway/config)
  bindsym $mod+m exec moe --toggle
  ```

- **AI errors**: the answer card shows what the endpoint returned; check the `[ai]`
  base_url/model, that the key is saved, and that the endpoint supports the chosen model
  (images need vision).

Milestone acceptance criteria live in `docs/adr/0009-milestone-scope.md`.
