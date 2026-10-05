#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri 2 壳（ADR-0001）：只做窗口与 IPC，全部逻辑在 moe-core / moe-extensions /
//! moe-platform。呼出键按 config.toml 装配：双击修饰键走 CGEventTap 监听，
//! 组合键走 global-shortcut 插件（ADR-0008）。

use std::sync::Mutex;
#[cfg(target_os = "macos")]
use std::time::Duration;

use moe_core::contract::{Action, ActionResult, CommandMeta, Item};
use moe_core::keymap::SystemKey;
use moe_core::registry::Registry;
use moe_platform::config::{MoeConfig, SummonKey};
use moe_platform::summon::{Modifier, SummonEvent, SummonListener, SummonStatus};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Builder as ShortcutBuilder, ShortcutState};

/// 组合键写法 → global-shortcut 插件的加速器语法。
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
                // 单字符按键：插件加速器语法用大写字母/数字
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

struct AppState {
    registry: Mutex<Registry>,
    config: MoeConfig,
    listener: Mutex<Box<dyn SummonListener>>,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SummonStatusPayload {
    /// "ready" | "needsPermission" | "unsupported"
    status: &'static str,
    key: String,
    double_tap_ms: u64,
}

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
    state
        .registry
        .lock()
        .expect("registry poisoned")
        .search(&query)
}

#[tauri::command]
fn invoke_command(
    state: State<'_, AppState>,
    command_id: String,
    query: Option<String>,
) -> Result<ActionResult, String> {
    // M2: selection 参数将从 moe-platform::TextTarget 抓取后传入。
    state
        .registry
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
        .registry
        .lock()
        .expect("registry poisoned")
        .run_item_action(&command_id, &item, &action)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn hide_panel(window: tauri::WebviewWindow) {
    let _ = window.hide();
}

#[tauri::command]
fn summon_status(state: State<'_, AppState>) -> SummonStatusPayload {
    let status = match state.listener.lock().expect("listener poisoned").status() {
        SummonStatus::Ready => "ready",
        SummonStatus::NeedsPermission => "needsPermission",
        SummonStatus::Unsupported => "unsupported",
    };
    SummonStatusPayload {
        status,
        key: key_label(&state.config.summon.key),
        double_tap_ms: state.config.summon.double_tap_ms,
    }
}

#[tauri::command]
fn open_accessibility_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
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
        // 组合键由 global-shortcut 插件承担，监听器留空。
        SummonKey::Combo { .. } => Box::new(moe_platform::UnsupportedSummon),
    };
    #[cfg(not(target_os = "macos"))]
    let listener: Box<dyn SummonListener> = match &config.summon.key {
        // X11 监听是 M4 Linux 验证的一部分；Wayland 只能 WM 绑定（README）。
        SummonKey::DoubleTap(_) => Box::new(moe_platform::UnsupportedSummon),
        SummonKey::Combo { .. } => Box::new(moe_platform::UnsupportedSummon),
    };

    let mut registry = Registry::new();
    moe_extensions::install(&mut registry);

    let accelerator = accelerator(&config.summon.key);
    let is_double_tap = matches!(config.summon.key, SummonKey::DoubleTap(_));

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
        .manage(AppState {
            registry: Mutex::new(registry),
            config,
            listener: Mutex::new(listener),
        })
        .setup(move |app| {
            let handle = app.handle().clone();

            if let Some(accel) = accelerator {
                use tauri_plugin_global_shortcut::GlobalShortcutExt;
                if let Err(err) = app.global_shortcut().register(accel.as_str()) {
                    eprintln!("moe: 无法注册呼出组合键 {accel}: {err}");
                }
            }

            if is_double_tap {
                let state = handle.state::<AppState>();
                let mut listener = state.listener.lock().expect("listener poisoned");
                let summon_handle = handle.clone();
                listener.start(Box::new(move |event| match event {
                    SummonEvent::Summon => toggle_panel(&summon_handle),
                    // 授权生效（无需重启）：通知面板收起引导条
                    SummonEvent::Authorized => {
                        let _ = summon_handle.emit("summon-authorized", ());
                    }
                }));
                // 未授权时没有可用呼出方式（tray 见 IIE4AD-347）：开机即展示面板与引导
                if listener.status() == SummonStatus::NeedsPermission
                    && let Some(window) = handle.get_webview_window("panel")
                {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            keymap,
            search_commands,
            invoke_command,
            run_item_action,
            hide_panel,
            summon_status,
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
}
