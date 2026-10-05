//! macOS 呼出监听：listen-only CGEventTap（ADR-0008）。
//!
//! 事件翻译（flagsChanged → [`Input`]）是薄胶水；判定语义在
//! [`crate::summon::DoubleTapDetector`]。未授权时创建 tap 会失败，这里退避
//! 重试——用户授权后无需重启即可生效。

use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use core_foundation::base::TCFType;
use core_foundation::mach_port::CFMachPortRef;
use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType,
};
use std::cell::RefCell;

use crate::summon::{
    DoubleTapDetector, Input, Modifier, SummonEvent, SummonListener, SummonStatus,
};

const STATUS_NEEDS_PERMISSION: u8 = 0;
const STATUS_READY: u8 = 1;

unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
    /// listen-only 键盘事件监听的正确权限门（10.15+）：输入监控。
    fn CGPreflightListenEventAccess() -> u8;
    fn CGRequestListenEventAccess() -> u8;
    /// core-graphics 的声明不对外导出；自行声明以支持 TapDisabled 后重新启用。
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
}

/// 辅助功能授权（M2 的 TextTarget 会用；这里仅用于诊断输出）。
pub fn is_accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

/// 输入监控（Input Monitoring）是否已授权——listen-only 键盘 tap 的门槛。
pub fn is_input_monitoring_granted() -> bool {
    unsafe { CGPreflightListenEventAccess() != 0 }
}

/// 未授权时弹一次系统引导，并把本应用加入「输入监控」列表。
pub fn request_input_monitoring() -> bool {
    unsafe { CGRequestListenEventAccess() != 0 }
}

pub struct MacSummonListener {
    key: Modifier,
    max_gap: Duration,
    status: Arc<AtomicU8>,
}

impl MacSummonListener {
    pub fn new(key: Modifier, max_gap: Duration) -> Self {
        Self {
            key,
            max_gap,
            status: Arc::new(AtomicU8::new(STATUS_NEEDS_PERMISSION)),
        }
    }
}

impl SummonListener for MacSummonListener {
    fn status(&self) -> SummonStatus {
        if self.status.load(Ordering::SeqCst) == STATUS_READY {
            SummonStatus::Ready
        } else {
            SummonStatus::NeedsPermission
        }
    }

    fn start(&mut self, handler: Box<dyn Fn(SummonEvent) + Send + 'static>) {
        let key = self.key;
        let max_gap = self.max_gap;
        let status = Arc::clone(&self.status);
        std::thread::Builder::new()
            .name("moe-summon".into())
            .spawn(move || run_listener(key, max_gap, status, handler))
            .expect("failed to spawn moe-summon thread");
    }
}

/// 让面板浮在别的全屏应用（独立虚拟屏）之上。
///
/// tao 的 `visible_on_all_workspaces` 只设了 `CanJoinAllSpaces`，不足以进入
/// 全屏应用独占的 Space；这里补 `FullScreenAuxiliary`。
/// 必须在主线程调用（调用方用 `run_on_main_thread` 保证）。
pub fn enable_fullscreen_auxiliary(ns_window: *mut std::ffi::c_void) {
    if ns_window.is_null() {
        return;
    }
    use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};
    // SAFETY: 指针来自 tauri 的 `ns_window()`，且调用方保证在主线程。
    unsafe {
        let window: &NSWindow = &*ns_window.cast::<NSWindow>();
        let behavior = window.collectionBehavior()
            | NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary;
        window.setCollectionBehavior(behavior);
    }
}

struct TapState {
    detector: DoubleTapDetector,
    flags: CGEventFlags,
    port: Option<CFMachPortRef>,
}

fn run_listener(
    key: Modifier,
    max_gap: Duration,
    status: Arc<AtomicU8>,
    handler: Box<dyn Fn(SummonEvent) + Send>,
) {
    eprintln!(
        "moe: 呼出监听启动（双击 {}，间隔上限 {:?}）",
        key.label(),
        max_gap
    );
    eprintln!(
        "moe: 输入监控 preflight = {}；辅助功能 = {}",
        is_input_monitoring_granted(),
        is_accessibility_trusted()
    );
    if !is_input_monitoring_granted() {
        eprintln!("moe: 请求系统授权（输入监控）…");
        request_input_monitoring();
    }

    // RefCell 足够：tap 回调与 run loop 同线程。
    let state = Rc::new(RefCell::new(TapState {
        detector: DoubleTapDetector::new(key, max_gap),
        flags: CGEventFlags::empty(),
        port: None,
    }));

    let mut logged_failure = false;
    loop {
        let tap_state = Rc::clone(&state);
        let handler_ref: &(dyn Fn(SummonEvent) + Send) = &*handler;
        let callback = move |_proxy, etype: CGEventType, event: &CGEvent| -> Option<CGEvent> {
            match etype {
                CGEventType::FlagsChanged => {
                    let mut s = tap_state.borrow_mut();
                    if let Some(input) = translate_flags(&mut s.flags, event.get_flags())
                        && s.detector.feed(input, Instant::now())
                    {
                        drop(s);
                        eprintln!("moe: 检测到双击 {} → 呼出", key.label());
                        handler_ref(SummonEvent::Summon);
                    }
                }
                CGEventType::KeyDown => {
                    tap_state
                        .borrow_mut()
                        .detector
                        .feed(Input::Other, Instant::now());
                }
                CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
                    if let Some(port) = tap_state.borrow().port {
                        unsafe { CGEventTapEnable(port, true) };
                    }
                }
                _ => {}
            }
            None
        };

        match CGEventTap::new(
            CGEventTapLocation::Session,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            vec![CGEventType::FlagsChanged, CGEventType::KeyDown],
            callback,
        ) {
            Ok(tap) => {
                state.borrow_mut().port = Some(tap.mach_port.as_concrete_TypeRef());
                status.store(STATUS_READY, Ordering::SeqCst);
                eprintln!("moe: CGEventTap 已挂载，双击 {} 可呼出", key.label());
                logged_failure = false;
                handler(SummonEvent::Authorized);
                let source = tap
                    .mach_port
                    .create_runloop_source(0)
                    .expect("failed to create run loop source");
                CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });
                tap.enable();
                CFRunLoop::run_current();
                // tap 失效（如系统睡眠后）：清除端口并重建
                state.borrow_mut().port = None;
                status.store(STATUS_NEEDS_PERMISSION, Ordering::SeqCst);
            }
            Err(()) => {
                // 未授权/暂时失败：退避重试，授权生效后无需重启（个别系统版本仍需重启一次）
                if !logged_failure {
                    eprintln!("moe: CGEventTap 创建失败（多半是「输入监控」未授权），每秒重试…");
                    logged_failure = true;
                }
                std::thread::sleep(Duration::from_millis(1000));
            }
        }
    }
}

fn translate_flags(before: &mut CGEventFlags, after: CGEventFlags) -> Option<Input> {
    let was = *before;
    *before = after;
    let masks = [
        (CGEventFlags::CGEventFlagCommand, Modifier::Meta),
        (CGEventFlags::CGEventFlagAlternate, Modifier::Alt),
        (CGEventFlags::CGEventFlagControl, Modifier::Control),
        (CGEventFlags::CGEventFlagShift, Modifier::Shift),
    ];
    for (mask, modifier) in masks {
        let held_before = was.contains(mask);
        let held_after = after.contains(mask);
        if held_before != held_after {
            return Some(if held_after {
                Input::Down(modifier)
            } else {
                Input::Up(modifier)
            });
        }
    }
    None
}
