#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri 2 壳（ADR-0001）：只做窗口与 IPC，全部逻辑在 moe-core / moe-extensions /
//! moe-platform。呼出键按 config.toml 装配：双击修饰键走 CGEventTap 监听，
//! 组合键走 global-shortcut 插件（ADR-0008）。

use std::sync::Mutex;
use std::time::Duration;

use moe_core::contract::Emitter as CommandEmitter;
use moe_core::contract::{Action, ActionResult, CommandEvent, CommandMeta, Item};
use moe_core::frecency::Frecency;
use moe_core::keymap::SystemKey;
use moe_core::registry::Registry;
use moe_platform::config::{MoeConfig, SummonKey};
use moe_platform::summon::{Modifier, SummonEvent, SummonListener, SummonStatus};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Builder as ShortcutBuilder, ShortcutState};

#[cfg(target_os = "macos")]
use tauri_nspanel::{CollectionBehavior, ManagerExt, PanelLevel, WebviewWindowExt};

// 面板形态的 NSPanel（ADR-0008 的浮层语义）：非激活应用、可进入全屏应用的
// Space、且能成为 key window 接收键盘。`can_become_key_window` 是打字的前提。
#[cfg(target_os = "macos")]
tauri_nspanel::tauri_panel! {
    panel!(MoePanel {
        config: {
            can_become_key_window: true,
            is_floating_panel: true
        }
    })
}

// 侧栏（chat）同款浮层语义：非激活、可进入全屏应用的 Space、且能成为 key window 接收输入。
// 宏会引入一批 `use` 导入，同一模块只能调用一次，因此放进子模块再导出类型。
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

/// 面板居中到鼠标所在显示器（含全屏应用所在的虚拟屏）。
fn centered_on(
    monitor_pos: (i32, i32),
    monitor_size: (u32, u32),
    window_size: (u32, u32),
) -> (i32, i32) {
    let x = monitor_pos.0 + (monitor_size.0.saturating_sub(window_size.0) / 2) as i32;
    let y = monitor_pos.1 + (monitor_size.1.saturating_sub(window_size.1) / 2) as i32;
    (x, y)
}

/// 右停靠：贴显示器右缘、顶部对齐（侧栏窗口用）。
fn right_docked_on(
    monitor_pos: (i32, i32),
    monitor_size: (u32, u32),
    window_size: (u32, u32),
) -> (i32, i32) {
    let x = monitor_pos.0 + monitor_size.0.saturating_sub(window_size.0) as i32;
    (x, monitor_pos.1)
}

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

/// 把 Command 的增量事件转发到面板（streaming 在 worker 线程发事件）。
struct TauriEventEmitter(AppHandle);

impl CommandEmitter for TauriEventEmitter {
    fn emit(&self, event: CommandEvent) {
        let _ = self.0.emit("command-event", event);
    }
}

struct AppState {
    registry: Mutex<Registry>,
    config: MoeConfig,
    listener: Mutex<Box<dyn SummonListener>>,
    /// 呼出面板前抓取的选区（非激活面板成为 key 后 AX 焦点会离开目标应用）。
    selection: Mutex<Option<String>>,
    /// 选区读取与回写的平台实现（ADR-0002）。
    text_target: Box<dyn moe_platform::TextTarget>,
    /// 命令使用记录（平台级，IIE4AD-346）；锁顺序：先 frecency 后 registry。
    frecency: Mutex<Frecency>,
    /// 最近一次展示面板的时刻（失焦收起需忽略展示瞬态）。
    last_shown: Mutex<Option<std::time::Instant>>,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SummonStatusPayload {
    /// "ready" | "needsPermission" | "unsupported"
    status: &'static str,
    key: String,
    double_tap_ms: u64,
    /// 辅助功能授权（选区/回写用；非 macOS 恒为 true）。
    accessibility: bool,
}

/// 把面板移到鼠标所在显示器（含全屏虚拟屏）并居中。
fn place_on_active_screen(window: &tauri::WebviewWindow) {
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
    let (x, y) = centered_on(
        (pos.x, pos.y),
        (msize.width, msize.height),
        (size.width, size.height),
    );
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

/// 菜单栏在屏幕顶部占用的高度（物理像素）。
/// NSScreen 要求主线程，调用点已在主线程闭包内。
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

/// 把侧栏移到鼠标所在显示器（含全屏虚拟屏）的右侧、占满高度。
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
    // 高度减去菜单栏：macOS 会把窗口顶边约束到可见区域之内，
    // 否则多出的高度会把底边（输入框）推到屏幕之外。
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

/// 必须在主线程调用：先抓选区，再定位并展示面板。
fn show_panel_blocking(window: &tauri::WebviewWindow) {
    if let Some(state) = window.app_handle().try_state::<AppState>() {
        // 先抓选区再显面板（面板成为 key 后系统焦点离开目标应用）
        let selection = state.text_target.read_selection().unwrap_or(None);
        *state.selection.lock().expect("selection poisoned") = selection;
        *state.last_shown.lock().expect("last_shown poisoned") = Some(std::time::Instant::now());
    }
    place_on_active_screen(window);
    #[cfg(target_os = "macos")]
    if let Ok(panel) = window.app_handle().get_webview_panel(window.label()) {
        // NSPanel：不激活应用、不切 Space，直接成为 key window 接收输入
        panel.show_and_make_key();
        eprintln!("moe: 面板已显示（NSPanel）");
        return;
    }
    let _ = window.show();
    let _ = window.set_focus();
    eprintln!("moe: 面板已显示");
}

/// 必须在主线程调用。
fn hide_panel_blocking(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    if let Ok(panel) = app.get_webview_panel("panel") {
        panel.hide();
        eprintln!("moe: 面板已隐藏（NSPanel）");
        return;
    }
    if let Some(window) = app.get_webview_window("panel") {
        let _ = window.hide();
        eprintln!("moe: 面板已隐藏");
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

/// Materialize：收起面板 → 右停靠展示 chat 窗口 → 交付载荷（会话 id 等）。
fn open_side_view(app: &AppHandle, payload: serde_json::Value) {
    let handle = app.clone();
    // 窗口操作（含 AppKit 的 orderOut/orderFront）都要在主线程。
    let _ = app.run_on_main_thread(move || {
        hide_panel_blocking(&handle);
        let Some(window) = handle.get_webview_window("chat") else {
            eprintln!("moe: chat 窗口不存在");
            return;
        };
        place_side_view(&window);

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
        eprintln!("moe: 侧栏已展示");
    });
}

/// WriteBack 投递：权限前置检查 → 收面板 → 等焦点交还 → AX/剪贴板写入。
fn deliver_writeback(app: &AppHandle, text: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    if !moe_platform::mac::is_accessibility_trusted() {
        moe_platform::mac::prompt_accessibility_permission();
        return Err("需要「辅助功能」授权才能回写（已弹出系统引导，授权后无需重启）".into());
    }
    hide_panel_blocking(app);
    let handle = app.clone();
    std::thread::spawn(move || {
        // 等非激活面板关闭、焦点交还目标应用
        std::thread::sleep(Duration::from_millis(120));
        let state = handle.state::<AppState>();
        match state.text_target.write_text(&text) {
            Ok(()) => eprintln!("moe: 已回写（{} 字符）", text.chars().count()),
            Err(err) => eprintln!("moe: 回写失败: {err}"),
        }
    });
    Ok(())
}

#[tauri::command]
fn keymap() -> Vec<(&'static str, SystemKey)> {
    moe_core::keymap::default_keymap()
}

#[tauri::command]
fn search_commands(state: State<'_, AppState>, query: String) -> Vec<CommandMeta> {
    // 锁顺序：先 frecency 后 registry（invoke 路径不嵌套持锁）
    let frecency = state.frecency.lock().expect("frecency poisoned");
    state
        .registry
        .lock()
        .expect("registry poisoned")
        .search(&query, &*frecency)
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
        .invoke_streaming(&command_id, query.as_deref(), selection.as_deref(), emitter)
        .map_err(|e| e.to_string())?;

    // Live 列表随输入重跑时不算一次「启动」（frecency 语义，IIE4AD-360）。
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

#[tauri::command]
fn hide_panel(app: AppHandle) {
    hide_panel_blocking(&app);
}

/// 附件路径校验（IIE4AD-358）：只探测类型与大小上限，不读入内容；
/// 错误直接回给面板/侧栏做内联提示。
#[tauri::command]
fn resolve_attachment(path: String) -> Result<moe_extensions::attachment::AttachmentInfo, String> {
    moe_extensions::attachment::inspect(std::path::Path::new(&path))
}

/// 侧栏历史：某会话的全部消息（按时间正序）。
#[tauri::command]
fn side_messages(conversation_id: String) -> Result<Vec<moe_core::conversation::Message>, String> {
    moe_platform::db::Db::open_default()?.messages(&conversation_id)
}

/// 侧栏续聊：空会话 id 会在 `ai` Namespace 新建会话；返回（可能新建的）会话 id。
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
        // 「输入监控」面板：listen-only 键盘 tap 的门槛
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
            .spawn();
    }
}

#[tauri::command]
fn open_accessibility_settings() {
    #[cfg(target_os = "macos")]
    {
        // 「辅助功能」面板：AX 读写选区/回写的门槛
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
    }
}

/// 菜单栏常驻：tray 图标 + 菜单（显示面板 / 打开配置文件 / 退出）。
fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;

    let show = MenuItem::with_id(app, "tray.show", "显示面板", true, None::<&str>)?;
    let config = MenuItem::with_id(app, "tray.config", "打开配置文件", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray.quit", "退出 Moe", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &config, &quit])?;

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray.show" => toggle_panel(app),
            "tray.config" => {
                if let Err(err) = moe_platform::config::open_in_editor() {
                    eprintln!("moe: 打开配置文件失败: {err}");
                }
            }
            "tray.quit" => app.exit(0),
            _ => {}
        });

    match tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png")) {
        Ok(icon) => builder = builder.icon(icon),
        Err(err) => eprintln!("moe: tray 图标解码失败: {err}"),
    }
    #[cfg(target_os = "macos")]
    {
        // 模板图：菜单栏按明暗自动渲染
        builder = builder.icon_as_template(true);
    }
    builder.build(app)?;
    eprintln!("moe: tray 已创建");
    Ok(())
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
    eprintln!("moe: 启动，呼出键 = {summon_label}");

    let builder = tauri::Builder::default()
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
            last_shown: Mutex::new(None),
        });

    // NSPanel 支持（macOS）：必须在 to_panel 之前注册
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    builder
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
                    // 授权生效：通知面板收起引导条
                    SummonEvent::Authorized => {
                        let _ = summon_handle.emit("summon-authorized", ());
                    }
                }));
            }

            // macOS：把窗口原地换成 NSPanel。普通 NSWindow 在应用被激活时会被
            // 系统拉回自己的 Space；NSPanel（非激活）才能浮在任何全屏应用之上，
            // 并且不把菜单栏抢走（ADR-0008 的浮层语义）。
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
                            eprintln!("moe: NonactivatingPanel 样式设置失败: {err:?}");
                        }
                        // NSPanel 默认失活即隐藏；显隐由呼出键控制
                        panel.set_hides_on_deactivate(false);
                    }
                    Err(err) => eprintln!("moe: 转换为 NSPanel 失败（回退普通窗口）: {err}"),
                }
            }

            // 侧栏窗口同款浮层：不抢激活、可出现在任意 Space（含全屏应用之上）。
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
                            eprintln!("moe: chat 的 NonactivatingPanel 样式设置失败: {err:?}");
                        }
                        panel.set_hides_on_deactivate(false);
                    }
                    Err(err) => eprintln!("moe: chat 转换为 NSPanel 失败（回退普通窗口）: {err}"),
                }
            }

            // 菜单栏常驻（IIE4AD-347）：无 Dock 图标、不参与 ⌘-Tab（Raycast 同款）
            #[cfg(target_os = "macos")]
            {
                let _ = handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }
            if let Err(err) = build_tray(app) {
                eprintln!("moe: tray 创建失败: {err}");
            }

            // 有 tray 后启动不再无条件展示面板：仅当呼出监听未就绪（缺「输入监控」
            // 授权）时展示引导。监听线程挂 tap 是毫秒级但异步，直接查会命中初始
            // 状态而误弹，因此给 600ms 宽限期后再决定。
            let probe = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(600));
                let needs_attention = probe
                    .state::<AppState>()
                    .listener
                    .lock()
                    .expect("listener poisoned")
                    .status()
                    == SummonStatus::NeedsPermission;
                if !needs_attention {
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
            // 失焦即收起（Raycast 同款）；tray 已提供返回入口，可安全启用。
            // 刚展示的 300ms 内忽略失焦：非激活面板成为 key window 的过程
            // 会产生瞬态 Focused(false)，不设护栏会「闪一下就消失」。
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
                eprintln!("moe: 失焦，面板已收起");
            }
        })
        .invoke_handler(tauri::generate_handler![
            keymap,
            search_commands,
            invoke_command,
            run_item_action,
            hide_panel,
            resolve_attachment,
            side_messages,
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
        // 第二块屏（macOS 允许排布在主屏左侧，坐标为负）也要正确
        assert_eq!(
            centered_on((-1920, 0), (1920, 1080), (680, 420)),
            (-1300, 330)
        );
        // 窗口比屏幕大时不越界（saturating 归零偏移）
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

    #[test]
    fn docks_side_view_to_right_edge() {
        assert_eq!(right_docked_on((0, 0), (1920, 1080), (420, 800)), (1500, 0));
        // 第二块屏（macOS 允许排布在主屏左侧，坐标为负）也要正确
        assert_eq!(
            right_docked_on((-1920, 0), (1920, 1080), (420, 800)),
            (-420, 0)
        );
        // 窗口比屏幕宽时不越界（saturating 归零偏移）
        assert_eq!(right_docked_on((0, 0), (300, 200), (420, 800)), (0, 0));
    }
}
