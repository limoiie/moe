#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri 2 壳（ADR-0001）：只做窗口与 IPC，全部逻辑在 moe-core / moe-extensions。

use std::sync::Mutex;

use moe_core::contract::{Action, ActionResult, CommandMeta, Item};
use moe_core::keymap::SystemKey;
use moe_core::registry::Registry;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_global_shortcut::{Builder as ShortcutBuilder, ShortcutState};

struct AppState(Mutex<Registry>);

fn toggle_panel(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("panel") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

#[tauri::command]
fn keymap() -> Vec<(&'static str, SystemKey)> {
    moe_core::keymap::default_keymap()
}

#[tauri::command]
fn search_commands(state: State<'_, AppState>, query: String) -> Vec<CommandMeta> {
    state.0.lock().expect("registry poisoned").search(&query)
}

#[tauri::command]
fn invoke_command(
    state: State<'_, AppState>,
    command_id: String,
    query: Option<String>,
) -> Result<ActionResult, String> {
    // M2: selection 参数将从 moe-platform::TextTarget 抓取后传入。
    state
        .0
        .lock()
        .expect("registry poisoned")
        .invoke(&command_id, query.as_deref(), None)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn run_item_action(
    state: State<'_, AppState>,
    command_id: String,
    item: Item,
    action: Action,
) -> Result<ActionResult, String> {
    state
        .0
        .lock()
        .expect("registry poisoned")
        .run_item_action(&command_id, &item, &action)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn hide_panel(window: tauri::WebviewWindow) {
    let _ = window.hide();
}

fn main() {
    let mut registry = Registry::new();
    moe_extensions::install(&mut registry);

    tauri::Builder::default()
        .plugin(
            ShortcutBuilder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_panel(app);
                    }
                })
                .build(),
        )
        .manage(AppState(Mutex::new(registry)))
        .setup(|app| {
            use tauri_plugin_global_shortcut::GlobalShortcutExt;
            // 临时呼出键：默认方案「双击 ⌘」需 CGEventTap + 辅助功能授权（ADR-0008），
            // 是 M1 的下一个 ticket；在此之前用 ⌥Space 让面板可用。
            app.global_shortcut().register("Alt+Space")?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            keymap,
            search_commands,
            invoke_command,
            run_item_action,
            hide_panel
        ])
        .run(tauri::generate_context!())
        .expect("error while running Moe");
}
