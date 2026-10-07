# Moe

A keyboard-first desktop command palette: double-tap ⌘ in any app to summon a centered panel,
search commands, and write results back to the selection or cursor. Product language lives in
`CONTEXT.md`; every design decision lives in `docs/adr/0001–0026`.

## Install (macOS)

1. Download `Moe_<version>_<arch>.dmg` from the GitHub Actions **Release** workflow
   (Actions → Release → the run → Artifacts; triggered by pushing a `v*` tag or manually).
2. Open the dmg and drag **Moe.app** into Applications.
3. **Unsigned / unnotarized**: on first launch right-click Moe.app → Open → Open again
   (a one-time Gatekeeper confirmation for unnotarized apps; normal double-click works after that).
4. First run: the Moe tray icon appears; the panel guides you through granting
   **Input Monitoring** (the gate for double-tap ⌘) and **Accessibility** (the gate for
   reading the selection / writing back). Both take effect immediately after granting.

## Usage

Double-tap ⌘ to summon the panel; typing searches commands. One set of keybinding semantics
spans every extension:

Root results are **grouped by source** (ADR-0020): one section per extension, headed by the
extension name (headers are not focusable); section order follows each group's best item
(match score when searching, frecency on an empty query), and items keep their score order
within a group. On an empty query there is also a **Suggestions** section on top (ADR-0023):
the most recently used commands (at most 5, most recent first) that are not repeated in the
sections below; it is hidden when nothing has been used yet. **⌃X forgets the focused
suggestion, ⌃⇧X clears all recent usage** (ADR-0025) — the commands themselves stay listed
in their own sections.

| Keys | Semantics | Notes |
|---|---|---|
| `↓` / `⌃N`, `↑` / `⌃P` | Navigate | Move the Focused Item |
| `⏎` | Apply | Run the primary action on the Focused Item |
| `⌥⏎` | Secondary | Default semantics: copy (e.g. copy the full AI answer to the clipboard) |
| `⌘K` / `⌘⇧P` | Show All Actions | The current item's primary/secondary action list |
| `⌘P` | Browse | The current extension's record list (AI = chat history; a hint if the extension declares none) |
| `⌘N` | New | Create a new record (AI = new chat; a hint if the extension declares none) |
| `⌃X` | Delete | Delete the current record (AI = current conversation; results layer and Suggestions only, ADR-0022/0025) |
| `⌃⇧X` | DeleteAll | Delete all records (AI = all conversations; also clears all suggestions, ADR-0022/0025) |
| `⌘M` | Materialize | Move the current conversation into the extension's Side View |
| `⌘⇧A` | Attach | Type/paste a file path, inserted as `@"path"` (ADR-0010) |
| `Esc` | Layered back | While generating → stop; otherwise actions card → (full-screen) detail → root → clear input → close the panel; on split pages the detail never collapses, Back goes straight to root |
| `⌫` | Layered back | Non-empty input = normal delete; **empty input steps back one layer** (like `Esc`, but the root layer never closes the panel, ADR-0017) |

A **bottom bar** floats over the list, exactly one row tall (ADR-0026) — the left chip and the
right pill line up with the last visible row band, and the chip's avatar is centered on the item
icons:

- **Avatar chip (bottom-left)**: Moe's own avatar on the root page; once you are inside a command
  it shows that extension's icon and name. It doubles as the **toast host**: toasts expand the
  chip into a pill (Raycast-style) instead of popping up anywhere else.
- **Action pill (bottom-right)**: **primary action** (the focused item's main action; becomes
  "Stop Generation" while generating) and **actions** (`⌘K` / `⌘⇧P`). `⌘K` pops up the **actions
  card**: a floating overlay whose top input filters actions and whose list below shows the item's
  actions (plus the `⌘P`/`⌘N` entries the extension declared), without replacing the main body;
  `↑↓` select, `⏎` run, `Esc` or empty `⌫` dismiss (ADR-0015).
- **About card**: clicking the avatar chip opens the app's About menu — a card with the same UX
  as the actions card but its search input at the bottom: **Open Config File** / **Save AI Key** /
  **Send Feedback**. `↑↓` select, `⏎` run, `Esc` or empty `⌫` dismiss, clicking outside closes it.

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
- **AI Chat**: type a question and press Enter (the fallback "AI: Ask \"…\"" appears when nothing
  matches); answers stream as Markdown cards; `⌥⏎` copies the full text, `⌘M` continues in the
  Side View. **Text selected before summoning automatically becomes question context** (not
  repeated when the question already contains it, truncated when overlong; history stores only
  the question itself). **Files selected in Finder before summoning automatically become
  attachments** (deduplicated with `@path` by path; the first use asks for Automation permission,
  ADR-0021).
- **AI Commands (text transforms, ADR-0024)**: 12 one-shot transform commands — Improve Writing /
  Fix Spelling & Grammar / Make Shorter / Make Longer / Simplify Language / Summarize /
  Translate to English / Translate to Chinese / Tone: Professional / Tone: Friendly /
  Extract Key Ideas / Continue Writing. **Select text, summon, type the command name (e.g.
  "improve") and press Enter**: the result streams into a result card and **automatically
  replaces the selection and dismisses the panel when done**; with no selection the input-bar
  text is used; Esc keeps what was generated so far and does not write back (⏎ writes back
  manually, ⌥⏎ copies).
- **Side View (AI chat)**: a window on the right with no persistent history bar. Three icon
  buttons sit at the end of the header: **More Actions (⌘ icon: New Chat / Open Config File /
  Hide)** , **History**, and **New Chat**. Clicking History (or `⌘P`) pops up a **floating history
  card** centered at the top: its top input filters titles, the list below highlights with `↑` `↓`,
  `⏎` opens, `Esc` dismisses, and clicking outside also dismisses it. `⌘P` (Browse) / `⌘⇧P`
  (Actions) / `⌘N` (New) / `⌃X` (Delete) / `⌃⇧X` (DeleteAll) are the general actions spanning
  every command/sub-app (ADR-0014/0022); in the Side View they land on the history card / actions
  menu / new chat / delete current conversation / delete all conversations (while the history card
  is open they act on the focused row; deleting the current conversation returns to the empty
  state). In the panel they land on the extension's declared record list / actions layer / new
  record / delete hooks (an error toast when the extension implements none). `⌃[` / `⌃]` step
  through the current conversation list; opening/switching a conversation scrolls to the bottom
  of the last line; `⏎` sends / `⇧⏎` newline / `Esc` dismisses overlays first, stops while
  generating, otherwise hides; 📎 adds attachments; window position and size are remembered after
  dragging. Past conversations are also reachable from the command "AI: Search Chat History"
  (narrow list on the left + detail on the right, showing the last answer).
- **Config**: run "Moe: Open Config File" or edit
  `~/Library/Application Support/moe/config.toml` directly:

  ```toml
  [summon]
  key = "double-cmd"   # double-cmd | double-option | double-ctrl | a combo such as cmd+shift+space
  double_tap_ms = 400  # 100..=1000

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
- **UI**: one keybinding semantics set (`↓`/`⌃N`, `↑`/`⌃P`, `⏎`, `⌥⏎`, `⌘K`, `⌘M`, `Esc`)
  spans every command; the results layer has two shapes declared by the command (full-screen
  detail / list + detail, ADR-0013); icons are Lucide (ADR-0012).

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
