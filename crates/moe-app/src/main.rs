#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri 2 shell (ADR-0001): windows and IPC only; all logic lives in moe-core / moe-extensions /
//! moe-platform. The summon key is assembled from config.toml: double-tap modifiers go through
//! a CGEventTap listener, combos go through the global-shortcut plugin (ADR-0008).

use std::sync::Mutex;
use std::time::Duration;

use moe_core::contract::Emitter as CommandEmitter;
use moe_core::contract::{
    Action, ActionResult, CommandEvent, CommandMeta, CommandSection, EntryKind, ExtensionMeta,
    Item, Selection,
};
use moe_core::frecency::Frecency;
use moe_core::keymap::SystemKey;
use moe_core::registry::Registry;
use moe_platform::config::{MoeConfig, SummonKey};
use moe_platform::summon::{Modifier, SummonEvent, SummonListener, SummonStatus};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Builder as ShortcutBuilder, ShortcutState};

#[cfg(target_os = "macos")]
use tauri_nspanel::{CollectionBehavior, ManagerExt, PanelLevel, WebviewWindowExt};

// The panel as an NSPanel (the floating semantics of ADR-0008): non-activating, joins the Spaces
// of full-screen apps, and can become the key window to receive keyboard input.
// `can_become_key_window` is the prerequisite for typing.
#[cfg(target_os = "macos")]
tauri_nspanel::tauri_panel! {
    panel!(MoePanel {
        config: {
            can_become_key_window: true,
            is_floating_panel: true
        }
    })
}

// The side view (chat) gets the same floating semantics: non-activating, joins the Spaces of
// full-screen apps, and can become the key window to receive input.
// The macro injects a batch of `use` imports and can only be invoked once per module, so it lives
// in a submodule that re-exports the type.
#[cfg(target_os = "macos")]
mod chat_panel {
    tauri_nspanel::tauri_panel! {
        panel!(MoeChatPanel {
            config: {
                can_become_key_window: true,
                is_floating_panel: true
            }
        })
    }
}

#[cfg(target_os = "macos")]
use chat_panel::MoeChatPanel;

/// Center the panel on the monitor under the mouse (including the virtual screen a full-screen app occupies).
fn centered_on(
    monitor_pos: (i32, i32),
    monitor_size: (u32, u32),
    window_size: (u32, u32),
) -> (i32, i32) {
    let x = monitor_pos.0 + (monitor_size.0.saturating_sub(window_size.0) / 2) as i32;
    let y = monitor_pos.1 + (monitor_size.1.saturating_sub(window_size.1) / 2) as i32;
    (x, y)
}

/// Right-docked: flush to the monitor's right edge, top-aligned (used by the side view window).
fn right_docked_on(
    monitor_pos: (i32, i32),
    monitor_size: (u32, u32),
    window_size: (u32, u32),
) -> (i32, i32) {
    let x = monitor_pos.0 + monitor_size.0.saturating_sub(window_size.0) as i32;
    (x, monitor_pos.1)
}

/// Combo notation → the global-shortcut plugin's accelerator syntax.
fn accelerator(key: &SummonKey) -> Option<String> {
    let SummonKey::Combo { modifiers, key } = key else {
        return None;
    };
    let mut parts: Vec<String> = modifiers
        .iter()
        .map(|m| {
            match m {
                Modifier::Meta => "Command",
                Modifier::Alt => "Alt",
                Modifier::Control => "Control",
                Modifier::Shift => "Shift",
            }
            .to_string()
        })
        .collect();
    parts.push(match key.as_str() {
        "space" => "Space".to_string(),
        other => {
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                // Single-character keys: the plugin's accelerator syntax uses uppercase letters/digits
                (Some(c), None) => c.to_ascii_uppercase().to_string(),
                _ => other.to_string(),
            }
        }
    });
    Some(parts.join("+"))
}

fn key_label(key: &SummonKey) -> String {
    match key {
        SummonKey::DoubleTap(Modifier::Meta) => "double-cmd".into(),
        SummonKey::DoubleTap(Modifier::Alt) => "double-option".into(),
        SummonKey::DoubleTap(Modifier::Control) => "double-ctrl".into(),
        SummonKey::DoubleTap(Modifier::Shift) => "double-shift".into(),
        SummonKey::Combo { .. } => accelerator(key).unwrap_or_default(),
    }
}

/// Forward incremental Command events to the panel (streaming emits from a worker thread).
struct TauriEventEmitter(AppHandle);

impl CommandEmitter for TauriEventEmitter {
    fn emit(&self, event: CommandEvent) {
        // Auto write-back after an AI command finishes streaming (ADR-0024): write into the host app and dismiss the panel, without forwarding to the UI.
        if let CommandEvent::WriteBack { text } = &event {
            if let Err(err) = deliver_writeback(&self.0, text.clone()) {
                eprintln!("moe: auto write-back failed: {err}");
            }
            return;
        }
        let _ = self.0.emit("command-event", event);
    }
}

struct AppState {
    registry: Mutex<Registry>,
    config: MoeConfig,
    listener: Mutex<Box<dyn SummonListener>>,
    /// The selection context captured before the panel is summoned (AX focus leaves the target
    /// app once the panel becomes key): text selection + Finder-selected files (ADR-0021).
    selection: Mutex<Option<Selection>>,
    /// The platform implementation of selection read and write-back (ADR-0002).
    text_target: Box<dyn moe_platform::TextTarget>,
    /// Command usage records (platform-level, IIE4AD-346); lock order: frecency → favorites → registry.
    frecency: Mutex<Frecency>,
    /// User-curated favorites (platform-level, ADR-0027): pinned above Suggestions on the root page.
    favorites: Mutex<moe_core::favorites::Favorites>,
    /// When the panel was last shown (blur-to-dismiss must ignore the showing transient).
    last_shown: Mutex<Option<std::time::Instant>>,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SummonStatusPayload {
    /// "ready" | "needsPermission" | "unsupported"
    status: &'static str,
    key: String,
    double_tap_ms: u64,
    /// Accessibility permission (for selection/write-back; always true off macOS).
    accessibility: bool,
}

/// Move the panel to the monitor under the mouse (including full-screen virtual screens) and center it.
fn place_on_active_screen(window: &tauri::WebviewWindow) {
    let Ok(size) = window.outer_size() else {
        return;
    };
    center_at_cursor(window, size.width as f64, size.height as f64);
}

/// Center the window on the monitor under the mouse (physical-pixel size, provided by the caller).
fn center_at_cursor(window: &tauri::WebviewWindow, width: f64, height: f64) {
    let Ok(cursor) = window.cursor_position() else {
        return;
    };
    let monitor = window
        .monitor_from_point(cursor.x, cursor.y)
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let pos = monitor.position();
    let msize = monitor.size();
    let (x, y) = centered_on(
        (pos.x, pos.y),
        (msize.width, msize.height),
        (width as u32, height as u32),
    );
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

/// Resize the panel by page shape (logical pixels, ADR-0018): split pages are wider and taller.
/// The UI computes the size from the shape table in `ui/src/layout.ts`; this only resizes and re-centers the window.
#[tauri::command]
fn resize_panel(window: tauri::WebviewWindow, width: f64, height: f64) -> Result<(), String> {
    window
        .set_size(tauri::LogicalSize::new(width, height))
        .map_err(|err| err.to_string())?;
    let scale = window.scale_factor().unwrap_or(1.0);
    center_at_cursor(&window, width * scale, height * scale);
    eprintln!("moe: panel size → {width}×{height} (logical pixels)");
    Ok(())
}

/// The height the menu bar occupies at the top of the screen (physical pixels).
/// NSScreen requires the main thread; call sites are already inside main-thread closures.
#[cfg(target_os = "macos")]
fn top_inset_physical(scale: f64) -> u32 {
    use tauri_nspanel::objc2::MainThreadMarker;
    use tauri_nspanel::objc2_app_kit::NSScreen;
    let Some(mtm) = MainThreadMarker::new() else {
        return 0;
    };
    let Some(screen) = NSScreen::mainScreen(mtm) else {
        return 0;
    };
    let frame = screen.frame();
    let visible = screen.visibleFrame();
    let inset_points =
        (frame.origin.y + frame.size.height) - (visible.origin.y + visible.size.height);
    (inset_points.max(0.0) * scale).round() as u32
}

/// Default side view placement: the right side of the monitor under the mouse (including full-screen
/// virtual screens), full height. Used only when there is no remembered frame or the remembered
/// frame is no longer on any monitor (IIE4AD-369).
fn place_side_view(window: &tauri::WebviewWindow) {
    let Ok(cursor) = window.cursor_position() else {
        return;
    };
    let monitor = window
        .monitor_from_point(cursor.x, cursor.y)
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    let (Some(monitor), Ok(size)) = (monitor, window.outer_size()) else {
        return;
    };
    let pos = monitor.position();
    let msize = monitor.size();
    // Height minus the menu bar: macOS constrains the window's top edge to the visible area;
    // otherwise the extra height pushes the bottom edge (the input bar) off-screen.
    #[cfg(target_os = "macos")]
    let inset = top_inset_physical(monitor.scale_factor());
    #[cfg(not(target_os = "macos"))]
    let inset = 0;
    let height = msize.height.saturating_sub(inset);
    let (x, y) = right_docked_on(
        (pos.x, pos.y + inset as i32),
        (msize.width, height),
        (size.width, size.height),
    );
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
    let _ = window.set_size(tauri::PhysicalSize::new(size.width, height));
}

/// Restore the previous position and size (remembered after the user drags/resizes); give up if the frame is no longer on any monitor.
fn restore_side_view(window: &tauri::WebviewWindow) -> bool {
    let Some(frame) = moe_platform::store::load_window_frame("chat") else {
        return false;
    };
    let on_screen = window
        .monitor_from_point(frame.x as f64, frame.y as f64)
        .ok()
        .flatten()
        .is_some();
    if !on_screen {
        return false;
    }
    let _ = window.set_position(tauri::PhysicalPosition::new(frame.x, frame.y));
    let _ = window.set_size(tauri::PhysicalSize::new(frame.width, frame.height));
    true
}

/// Remember the side view's current frame (debounced write after drag/resize).
fn schedule_chat_frame_save(window: &tauri::WebviewWindow) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static GENERATION: AtomicU64 = AtomicU64::new(0);
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let handle = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(400));
        if GENERATION.load(Ordering::SeqCst) != generation {
            return; // a newer move/resize event arrived; hand it to the newcomer
        }
        let (Ok(pos), Ok(size)) = (handle.outer_position(), handle.outer_size()) else {
            return;
        };
        moe_platform::store::save_window_frame(
            "chat",
            moe_platform::store::WindowFrame {
                x: pos.x,
                y: pos.y,
                width: size.width,
                height: size.height,
            },
        );
    });
}

/// A transparent window's shadow is generated from content alpha and cached: recompute once before showing (ADR-0016).
#[cfg(target_os = "macos")]
fn refresh_window_shadow(window: &tauri::WebviewWindow) {
    if let Ok(ns_window) = window.ns_window() {
        // SAFETY: the pointer comes from Tauri's window handle and its lifetime follows the window.
        unsafe { moe_platform::mac::refresh_window_shadow(ns_window) };
    }
}

/// Must be called on the main thread: capture the selection first, then position and show the panel.
fn show_panel_blocking(window: &tauri::WebviewWindow) {
    if let Some(state) = window.app_handle().try_state::<AppState>() {
        // Capture the selection context before showing the panel (system focus leaves the target
        // app once the panel becomes key): text selection + Finder-selected files (ADR-0021).
        let text = state.text_target.read_selection().unwrap_or(None);
        let files = moe_platform::files::finder_selection();
        eprintln!(
            "moe: selection {} chars, {} Finder files",
            text.as_deref().map(|s| s.chars().count()).unwrap_or(0),
            files.len()
        );
        *state.selection.lock().expect("selection poisoned") = Some(Selection { text, files });
        *state.last_shown.lock().expect("last_shown poisoned") = Some(std::time::Instant::now());
    }
    place_on_active_screen(window);
    #[cfg(target_os = "macos")]
    refresh_window_shadow(window);
    #[cfg(target_os = "macos")]
    if let Ok(panel) = window.app_handle().get_webview_panel(window.label()) {
        // NSPanel: no app activation, no Space switch; directly becomes the key window to receive input
        panel.show_and_make_key();
        eprintln!("moe: panel shown (NSPanel)");
        return;
    }
    let _ = window.show();
    let _ = window.set_focus();
    eprintln!("moe: panel shown");
}

/// Must be called on the main thread.
fn hide_panel_blocking(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    if let Ok(panel) = app.get_webview_panel("panel") {
        panel.hide();
        eprintln!("moe: panel hidden (NSPanel)");
        return;
    }
    if let Some(window) = app.get_webview_window("panel") {
        let _ = window.hide();
        eprintln!("moe: panel hidden");
    }
}

fn toggle_panel_blocking(app: &AppHandle) {
    let Some(window) = app.get_webview_window("panel") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        hide_panel_blocking(app);
    } else {
        show_panel_blocking(&window);
    }
}

fn toggle_panel(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || toggle_panel_blocking(&handle));
}

/// Materialize: dismiss the panel → show the chat window right-docked → deliver the payload (conversation id, etc.).
fn open_side_view(app: &AppHandle, payload: serde_json::Value) {
    let handle = app.clone();
    // Window operations (including AppKit's orderOut/orderFront) must run on the main thread.
    let _ = app.run_on_main_thread(move || {
        hide_panel_blocking(&handle);
        let Some(window) = handle.get_webview_window("chat") else {
            eprintln!("moe: chat window does not exist");
            return;
        };
        // Use the remembered frame if the user has dragged it, otherwise default right-docked
        if !restore_side_view(&window) {
            place_side_view(&window);
        }
        #[cfg(target_os = "macos")]
        refresh_window_shadow(&window);

        #[cfg(target_os = "macos")]
        let panel_shown = if let Ok(panel) = handle.get_webview_panel("chat") {
            panel.show_and_make_key();
            true
        } else {
            false
        };
        #[cfg(not(target_os = "macos"))]
        let panel_shown = false;

        if !panel_shown {
            let _ = window.show();
            let _ = window.set_focus();
        }
        let _ = handle.emit_to("chat", "side-open", payload);
        eprintln!("moe: side view shown");
    });
}

/// WriteBack delivery: permission pre-check → dismiss panel → wait for focus to return → write via AX/clipboard.
fn deliver_writeback(app: &AppHandle, text: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    if !moe_platform::mac::is_accessibility_trusted() {
        moe_platform::mac::prompt_accessibility_permission();
        return Err("Accessibility permission is required to write back (System Settings → Privacy & Security → Accessibility; the system prompt was opened, no restart needed once granted)".into());
    }
    hide_panel_blocking(app);
    let handle = app.clone();
    std::thread::spawn(move || {
        // Wait for the non-activating panel to close and focus to return to the target app
        std::thread::sleep(Duration::from_millis(120));
        let state = handle.state::<AppState>();
        match state.text_target.write_text(&text) {
            Ok(()) => eprintln!("moe: wrote back ({} chars)", text.chars().count()),
            Err(err) => eprintln!("moe: write back failed: {err}"),
        }
    });
    Ok(())
}

#[tauri::command]
fn keymap() -> Vec<(&'static str, SystemKey)> {
    moe_core::keymap::default_keymap()
}

#[tauri::command]
fn search_commands(state: State<'_, AppState>, query: String) -> Vec<CommandSection> {
    // The selection only matters for the "no match → fallback" path (e.g. an AI prompt uses the selected text as context); grab it and release.
    let selection = state.selection.lock().expect("selection poisoned").clone();
    // Lock order: frecency, favorites, then registry (the invoke path never holds nested locks)
    let frecency = state.frecency.lock().expect("frecency poisoned");
    let favorites = state.favorites.lock().expect("favorites poisoned");
    state.registry.lock().expect("registry poisoned").search(
        &query,
        selection.as_ref(),
        &*frecency,
        &favorites,
    )
}

/// Entry lookup for the generic actions Browse (⌘P) / New (⌘N) (ADR-0014):
/// resolve the entry command declared by the current command's Extension; None = the extension has no such records.
#[tauri::command]
fn entry_command(
    state: State<'_, AppState>,
    command_id: String,
    kind: EntryKind,
) -> Option<CommandMeta> {
    state
        .registry
        .lock()
        .expect("registry poisoned")
        .entry_command(&command_id, kind)
}

#[tauri::command]
fn invoke_command(
    app: AppHandle,
    state: State<'_, AppState>,
    command_id: String,
    query: Option<String>,
    record: Option<bool>,
) -> Result<ActionResult, String> {
    let selection = state.selection.lock().expect("selection poisoned").clone();
    let emitter: std::sync::Arc<dyn CommandEmitter> =
        std::sync::Arc::new(TauriEventEmitter(app.clone()));
    let result = state
        .registry
        .lock()
        .expect("registry poisoned")
        .invoke_streaming(&command_id, query.as_deref(), selection.as_ref(), emitter)
        .map_err(|e| e.to_string())?;

    // Re-running a Live list as input changes is not a "launch" (frecency semantics, IIE4AD-360).
    if record.unwrap_or(true) {
        let mut frecency = state.frecency.lock().expect("frecency poisoned");
        frecency.record(&command_id, std::time::SystemTime::now());
        moe_platform::store::save_frecency(&frecency);
    }

    if let ActionResult::WriteBack { text } = &result {
        deliver_writeback(&app, text.clone())?;
    }
    if let ActionResult::OpenSideView { payload } = &result {
        open_side_view(&app, payload.clone());
    }
    Ok(result)
}

#[tauri::command]
fn run_item_action(
    app: AppHandle,
    state: State<'_, AppState>,
    command_id: String,
    item: Item,
    action: Action,
) -> Result<ActionResult, String> {
    let result = state
        .registry
        .lock()
        .expect("registry poisoned")
        .run_item_action(&command_id, &item, &action)
        .map_err(|e| e.to_string())?;
    if let ActionResult::WriteBack { text } = &result {
        deliver_writeback(&app, text.clone())?;
    }
    if let ActionResult::OpenSideView { payload } = &result {
        open_side_view(&app, payload.clone());
    }
    Ok(result)
}

/// Delete the current record (generic action Delete, ⌃X, ADR-0022): returns the actual number deleted.
#[tauri::command]
fn delete_item(
    state: State<'_, AppState>,
    command_id: String,
    item: Item,
) -> Result<usize, String> {
    state
        .registry
        .lock()
        .expect("registry poisoned")
        .delete_item(&command_id, &item)
        .map_err(|e| e.to_string())
}

/// Delete all records (generic action DeleteAll, ⌃⇧X, ADR-0022): returns the actual number deleted.
#[tauri::command]
fn delete_all(state: State<'_, AppState>, command_id: String) -> Result<usize, String> {
    state
        .registry
        .lock()
        .expect("registry poisoned")
        .delete_all(&command_id)
        .map_err(|e| e.to_string())
}

/// Extension identity for the UI's avatar chip (ADR-0026): id + title + a representative icon.
#[tauri::command]
fn extension_meta(state: State<'_, AppState>, command_id: String) -> Option<ExtensionMeta> {
    state
        .registry
        .lock()
        .expect("registry poisoned")
        .extension_meta(&command_id)
}

/// Toggle a command's favorite state (ADR-0027); returns the new state.
#[tauri::command]
fn toggle_favorite(state: State<'_, AppState>, command_id: String) -> bool {
    let mut favorites = state.favorites.lock().expect("favorites poisoned");
    let now = favorites.toggle(&command_id);
    moe_platform::store::save_favorites(&favorites);
    now
}

/// Whether a command is currently favorited (the actions card's label, ADR-0027).
#[tauri::command]
fn is_favorite(state: State<'_, AppState>, command_id: String) -> bool {
    state
        .favorites
        .lock()
        .expect("favorites poisoned")
        .contains(&command_id)
}

/// Open an external URL in the default browser (About card → Send Feedback, ADR-0026).
#[tauri::command]
fn open_external(url: String) -> Result<(), String> {
    moe_platform::open::open_url(&url)
}

/// Forget one recently used command (⌃X on a suggestion, ADR-0025).
/// Returns whether the command had recorded usage.
#[tauri::command]
fn delete_suggestion(state: State<'_, AppState>, command_id: String) -> Result<bool, String> {
    let mut frecency = state.frecency.lock().expect("frecency poisoned");
    let removed = frecency.remove(&command_id);
    moe_platform::store::save_frecency(&frecency);
    Ok(removed)
}

/// Clear all recently used commands (⌃⇧X on suggestions, ADR-0025).
/// Returns how many commands were forgotten.
#[tauri::command]
fn clear_suggestions(state: State<'_, AppState>) -> Result<usize, String> {
    let mut frecency = state.frecency.lock().expect("frecency poisoned");
    let count = frecency.len();
    frecency.clear();
    moe_platform::store::save_frecency(&frecency);
    Ok(count)
}

#[tauri::command]
fn hide_panel(app: AppHandle) {
    hide_panel_blocking(&app);
}

/// Stop in-progress generation (IIE4AD-365): called by the panel/side view on Esc; returns the number of generations aborted.
#[tauri::command]
fn stop_generation(state: State<'_, AppState>) -> usize {
    state
        .registry
        .lock()
        .expect("registry poisoned")
        .stop_generation()
}

/// Attachment path validation (IIE4AD-358): probes only type and size limits, never reads content;
/// errors go straight back to the panel/side view for inline hints.
#[tauri::command]
fn resolve_attachment(path: String) -> Result<moe_extensions::attachment::AttachmentInfo, String> {
    moe_extensions::attachment::inspect(std::path::Path::new(&path))
}

/// Side view conversation history list (left column of the split view, IIE4AD-369): filtered by title, most recent first.
#[tauri::command]
fn side_conversations(
    query: Option<String>,
) -> Result<Vec<moe_core::conversation::Conversation>, String> {
    moe_platform::db::Db::open_default()?.conversations("ai", query.as_deref(), 50)
}

/// Side view history: all messages of a conversation (chronological order).
#[tauri::command]
fn side_messages(conversation_id: String) -> Result<Vec<moe_core::conversation::Message>, String> {
    moe_platform::db::Db::open_default()?.messages(&conversation_id)
}

/// Side view continue-chat: an empty conversation id creates a new conversation in the `ai` namespace; returns the (possibly new) conversation id.
#[tauri::command]
fn side_send(
    app: AppHandle,
    state: State<'_, AppState>,
    conversation_id: String,
    message: String,
) -> Result<String, String> {
    let emitter: std::sync::Arc<dyn CommandEmitter> =
        std::sync::Arc::new(TauriEventEmitter(app.clone()));
    state
        .registry
        .lock()
        .expect("registry poisoned")
        .side_continue("ai", &conversation_id, &message, emitter)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn summon_status(state: State<'_, AppState>) -> SummonStatusPayload {
    let status = match state.listener.lock().expect("listener poisoned").status() {
        SummonStatus::Ready => "ready",
        SummonStatus::NeedsPermission => "needsPermission",
        SummonStatus::Unsupported => "unsupported",
    };
    #[cfg(target_os = "macos")]
    let accessibility = moe_platform::mac::is_accessibility_trusted();
    #[cfg(not(target_os = "macos"))]
    let accessibility = true;
    SummonStatusPayload {
        status,
        key: key_label(&state.config.summon.key),
        double_tap_ms: state.config.summon.double_tap_ms,
        accessibility,
    }
}

#[tauri::command]
fn open_input_monitoring_settings() {
    #[cfg(target_os = "macos")]
    {
        // The Input Monitoring pane: the gate for the listen-only keyboard tap
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
            .spawn();
    }
}

#[tauri::command]
fn open_accessibility_settings() {
    #[cfg(target_os = "macos")]
    {
        // The Accessibility pane: the gate for AX selection read/write-back
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
    }
}

/// Menu bar resident: tray icon + menu (Show Panel / AI Chat / Launch at Login / Open Config File / Quit).
fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::TrayIconBuilder;
    use tauri_plugin_autostart::ManagerExt as _;

    let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);

    let show = MenuItem::with_id(app, "tray.show", "Show Panel", true, None::<&str>)?;
    let chat = MenuItem::with_id(app, "tray.chat", "AI Chat", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(
        app,
        "tray.autostart",
        "Launch at Login",
        true,
        autostart_enabled,
        None::<&str>,
    )?;
    let config = MenuItem::with_id(app, "tray.config", "Open Config File", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray.quit", "Quit Moe", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &chat, &autostart, &separator, &config, &quit])?;

    let autostart_item = autostart.clone();
    let mut builder = TrayIconBuilder::with_id("moe-tray")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "tray.show" => toggle_panel(app),
            // Side view with an empty conversation: start one if none is running; history is recoverable via "AI: Search Chat History"
            "tray.chat" => open_side_view(app, serde_json::json!({})),
            "tray.autostart" => {
                let currently = app.autolaunch().is_enabled().unwrap_or(false);
                let result = if currently {
                    app.autolaunch().disable()
                } else {
                    app.autolaunch().enable()
                };
                match result {
                    Ok(()) => {
                        let now = !currently;
                        if let Err(err) = autostart_item.set_checked(now) {
                            eprintln!("moe: failed to update autostart check: {err}");
                        }
                        eprintln!(
                            "moe: launch at login {}",
                            if now { "enabled" } else { "disabled" }
                        );
                    }
                    Err(err) => eprintln!("moe: failed to toggle launch at login: {err}"),
                }
            }
            "tray.config" => {
                if let Err(err) = moe_platform::config::open_in_editor() {
                    eprintln!("moe: failed to open config file: {err}");
                }
            }
            "tray.quit" => app.exit(0),
            _ => {}
        });

    match tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png")) {
        Ok(icon) => builder = builder.icon(icon),
        Err(err) => eprintln!("moe: failed to decode tray icon: {err}"),
    }
    #[cfg(target_os = "macos")]
    {
        // Template image: the menu bar renders it automatically for light/dark
        builder = builder.icon_as_template(true);
    }
    builder.build(app)?;
    eprintln!("moe: tray created");
    Ok(())
}

/// Whether the panel should be proactively shown for guidance 600ms after startup.
fn needs_attention(status: SummonStatus, key: &SummonKey) -> bool {
    match status {
        // Missing Input Monitoring permission (macOS): show permission guidance in the panel
        SummonStatus::NeedsPermission => true,
        // Double-tap listening is unavailable in this session (Wayland / no RECORD / unimplemented platform):
        // guide the user to a combo or a WM binding for `moe --toggle`
        SummonStatus::Unsupported => matches!(key, SummonKey::DoubleTap(_)),
        SummonStatus::Ready => false,
    }
}

fn main() {
    let config = MoeConfig::load();

    #[cfg(target_os = "macos")]
    let listener: Box<dyn SummonListener> = match &config.summon.key {
        SummonKey::DoubleTap(modifier) => Box::new(moe_platform::mac::MacSummonListener::new(
            *modifier,
            Duration::from_millis(config.summon.double_tap_ms),
        )),
        // Combos are handled by the global-shortcut plugin; the listener stays empty.
        SummonKey::Combo { .. } => Box::new(moe_platform::UnsupportedSummon),
    };
    #[cfg(not(target_os = "macos"))]
    let listener: Box<dyn SummonListener> = match &config.summon.key {
        // XRecord permission-free listening (IIE4AD-350); on Wayland / no RECORD it self-checks and reports Unsupported
        #[cfg(target_os = "linux")]
        SummonKey::DoubleTap(modifier) => Box::new(moe_platform::x11::X11SummonListener::new(
            *modifier,
            Duration::from_millis(config.summon.double_tap_ms),
        )),
        // Double-tap is not implemented on other platforms: the panel guides the user to a combo after startup
        #[cfg(not(target_os = "linux"))]
        SummonKey::DoubleTap(_) => Box::new(moe_platform::UnsupportedSummon),
        SummonKey::Combo { .. } => Box::new(moe_platform::UnsupportedSummon),
    };

    #[cfg(target_os = "macos")]
    let text_target: Box<dyn moe_platform::TextTarget> =
        Box::new(moe_platform::mac_text::mac_text_target());
    #[cfg(not(target_os = "macos"))]
    let text_target: Box<dyn moe_platform::TextTarget> = Box::new(moe_platform::Unsupported);

    let mut registry = Registry::new();
    moe_extensions::install(&mut registry);

    let accelerator = accelerator(&config.summon.key);
    let is_double_tap = matches!(config.summon.key, SummonKey::DoubleTap(_));
    let summon_label = key_label(&config.summon.key);
    // `moe --toggle`: shows the panel directly on a cold start; an already-running instance forwards it as a toggle via the single-instance callback.
    let toggle_on_start = std::env::args().any(|arg| arg == "--toggle");
    eprintln!("moe: started, summon key = {summon_label}");

    let builder = tauri::Builder::default()
        // Single instance must be registered first: `moe --toggle` is forwarded from the second
        // process to the running instance (the WM binding path when Wayland has no global key
        // interception, IIE4AD-350)
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            eprintln!("moe: an instance is already running, forwarding request: {argv:?}");
            toggle_panel(app);
        }))
        // Launch at login (LaunchAgent; the tray checkbox, IIE4AD-362)
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            ShortcutBuilder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_panel(app);
                    }
                })
                .build(),
        )
        .manage(AppState {
            registry: Mutex::new(registry),
            config,
            listener: Mutex::new(listener),
            selection: Mutex::new(None),
            text_target,
            frecency: Mutex::new(moe_platform::store::load_frecency()),
            favorites: Mutex::new(moe_platform::store::load_favorites()),
            last_shown: Mutex::new(None),
        });

    // NSPanel support (macOS): must be registered before to_panel
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    builder
        .setup(move |app| {
            let handle = app.handle().clone();

            if let Some(accel) = accelerator {
                use tauri_plugin_global_shortcut::GlobalShortcutExt;
                if let Err(err) = app.global_shortcut().register(accel.as_str()) {
                    eprintln!("moe: failed to register summon combo {accel}: {err}");
                }
            }

            if is_double_tap {
                let state = handle.state::<AppState>();
                let mut listener = state.listener.lock().expect("listener poisoned");
                let summon_handle = handle.clone();
                listener.start(Box::new(move |event| match event {
                    SummonEvent::Summon => toggle_panel(&summon_handle),
                    // Permission granted: tell the panel to dismiss the guidance banner
                    SummonEvent::Authorized => {
                        let _ = summon_handle.emit("summon-authorized", ());
                    }
                }));
            }

            // macOS: convert the window in place into an NSPanel. A normal NSWindow gets pulled
            // back to its own Space by the system when the app is activated; an NSPanel
            // (non-activating) can float above any full-screen app without stealing the menu bar
            // (the floating semantics of ADR-0008).
            #[cfg(target_os = "macos")]
            if let Some(window) = handle.get_webview_window("panel") {
                match window.to_panel::<MoePanel>() {
                    Ok(panel) => {
                        panel.set_level(PanelLevel::Floating.value());
                        panel.set_collection_behavior(
                            CollectionBehavior::new()
                                .can_join_all_spaces()
                                .full_screen_auxiliary()
                                .value(),
                        );
                        if let Err(err) = panel.add_style_mask(
                            tauri_nspanel::objc2_app_kit::NSWindowStyleMask::NonactivatingPanel,
                        ) {
                            eprintln!("moe: failed to set NonactivatingPanel style: {err:?}");
                        }
                        // NSPanels hide on deactivation by default; visibility is controlled by the summon key
                        panel.set_hides_on_deactivate(false);
                    }
                    Err(err) => eprintln!("moe: failed to convert to NSPanel (falling back to a normal window): {err}"),
                }
            }

            // The side view window gets the same floating treatment: never activates, can appear on any Space (including above full-screen apps).
            #[cfg(target_os = "macos")]
            if let Some(window) = handle.get_webview_window("chat") {
                match window.to_panel::<MoeChatPanel>() {
                    Ok(panel) => {
                        panel.set_level(PanelLevel::Floating.value());
                        panel.set_collection_behavior(
                            CollectionBehavior::new()
                                .can_join_all_spaces()
                                .full_screen_auxiliary()
                                .value(),
                        );
                        if let Err(err) = panel.add_style_mask(
                            tauri_nspanel::objc2_app_kit::NSWindowStyleMask::NonactivatingPanel,
                        ) {
                            eprintln!("moe: failed to set NonactivatingPanel style for chat: {err:?}");
                        }
                        panel.set_hides_on_deactivate(false);
                    }
                    Err(err) => eprintln!("moe: failed to convert chat to NSPanel (falling back to a normal window): {err}"),
                }
            }

            // Menu bar resident (IIE4AD-347): no Dock icon, not in ⌘-Tab (Raycast-style)
            #[cfg(target_os = "macos")]
            {
                let _ = handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }
            if let Err(err) = build_tray(app) {
                eprintln!("moe: failed to create tray: {err}");
            }

            // `moe --toggle` cold start: no old instance to forward to, so show the panel directly
            if toggle_on_start {
                toggle_panel(&handle);
            }

            // With the tray in place, startup no longer shows the panel unconditionally: it shows
            // only when the summon listener needs guidance (missing macOS permission / double-tap
            // unavailable on Linux Wayland etc.). The listener thread starts within milliseconds
            // but asynchronously, so an immediate check would hit the initial state and misfire —
            // hence a 600ms grace period before deciding.
            let probe = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(600));
                let attention_needed = {
                    let state = probe.state::<AppState>();
                    let status = state.listener.lock().expect("listener poisoned").status();
                    needs_attention(status, &state.config.summon.key)
                };
                if !attention_needed {
                    return;
                }
                let show_handle = probe.clone();
                let _ = probe.run_on_main_thread(move || {
                    if let Some(window) = show_handle.get_webview_window("panel") {
                        show_panel_blocking(&window);
                    }
                });
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            // Side view: remember the position and size after the user drags/resizes (IIE4AD-369); it does not participate in blur-to-dismiss
            if window.label() == "chat" {
                if matches!(
                    event,
                    tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_)
                ) && let Some(webview) = window.app_handle().get_webview_window("chat")
                {
                    schedule_chat_frame_save(&webview);
                }
                return;
            }
            // Dismiss on blur (Raycast-style); the tray provides a way back, so it's safe to enable.
            // Ignore blur within 300ms of showing: a non-activating panel becoming the key window
            // produces a transient Focused(false), and without this guard it would "flash and vanish".
            if window.label() != "panel" || !window.is_visible().unwrap_or(false) {
                return;
            }
            if let tauri::WindowEvent::Focused(false) = event {
                let just_shown = window
                    .app_handle()
                    .try_state::<AppState>()
                    .and_then(|state| *state.last_shown.lock().expect("last_shown poisoned"))
                    .is_some_and(|at| at.elapsed() < Duration::from_millis(300));
                if just_shown {
                    return;
                }
                let _ = window.hide();
                eprintln!("moe: blurred, panel dismissed");
            }
        })
        .invoke_handler(tauri::generate_handler![
            keymap,
            search_commands,
            entry_command,
            resize_panel,
            invoke_command,
            run_item_action,
            delete_item,
            delete_all,
            delete_suggestion,
            clear_suggestions,
            toggle_favorite,
            is_favorite,
            extension_meta,
            open_external,
            hide_panel,
            stop_generation,
            resolve_attachment,
            side_messages,
            side_conversations,
            side_send,
            summon_status,
            open_input_monitoring_settings,
            open_accessibility_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running Moe");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combos_map_to_plugin_accelerators() {
        let key = SummonKey::parse("cmd+shift+space").unwrap();
        assert_eq!(accelerator(&key).as_deref(), Some("Command+Shift+Space"));
        let key = SummonKey::parse("ctrl+k").unwrap();
        assert_eq!(accelerator(&key).as_deref(), Some("Control+K"));
        let key = SummonKey::parse("double-cmd").unwrap();
        assert_eq!(accelerator(&key), None);
    }

    #[test]
    fn centers_window_in_monitor() {
        assert_eq!(centered_on((0, 0), (1920, 1080), (680, 420)), (620, 330));
        // A second monitor (macOS allows placing it left of the main screen, with negative coordinates) must also be correct
        assert_eq!(
            centered_on((-1920, 0), (1920, 1080), (680, 420)),
            (-1300, 330)
        );
        // A window larger than the screen stays in bounds (saturating zeroes the offset)
        assert_eq!(centered_on((0, 0), (600, 400), (680, 420)), (0, 0));
    }

    #[test]
    fn key_labels_round_trip_the_config_forms() {
        assert_eq!(
            key_label(&SummonKey::parse("double-cmd").unwrap()),
            "double-cmd"
        );
        assert_eq!(
            key_label(&SummonKey::parse("double-option").unwrap()),
            "double-option"
        );
        assert_eq!(
            key_label(&SummonKey::parse("cmd+space").unwrap()),
            "Command+Space"
        );
    }

    /// Startup guidance trigger (IIE4AD-350): when double-tap is unavailable, show the panel once to offer alternatives.
    #[test]
    fn startup_shows_panel_only_when_summon_needs_guidance() {
        let double = SummonKey::parse("double-cmd").unwrap();
        let combo = SummonKey::parse("cmd+shift+space").unwrap();
        assert!(needs_attention(SummonStatus::NeedsPermission, &double));
        assert!(needs_attention(SummonStatus::Unsupported, &double));
        assert!(!needs_attention(SummonStatus::Unsupported, &combo));
        assert!(!needs_attention(SummonStatus::Ready, &double));
    }

    #[test]
    fn docks_side_view_to_right_edge() {
        assert_eq!(right_docked_on((0, 0), (1920, 1080), (420, 800)), (1500, 0));
        // A second monitor (macOS allows placing it left of the main screen, with negative coordinates) must also be correct
        assert_eq!(
            right_docked_on((-1920, 0), (1920, 1080), (420, 800)),
            (-420, 0)
        );
        // A window wider than the screen stays in bounds (saturating zeroes the offset)
        assert_eq!(right_docked_on((0, 0), (300, 200), (420, 800)), (0, 0));
    }
}
