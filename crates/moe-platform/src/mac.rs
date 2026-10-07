//! macOS summon listener: listen-only CGEventTap (ADR-0008).
//!
//! Event translation (flagsChanged → [`Input`]) is thin glue; the detection semantics live in
//! [`crate::summon::DoubleTapDetector`]. Creating the tap fails without permission, so we retry with backoff here —
//! it takes effect once the user grants permission, without a restart.

use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::mach_port::CFMachPortRef;
use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes};
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType,
};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSEvent, NSScreen, NSWindow};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use std::cell::RefCell;

use crate::summon::{
    DoubleTapDetector, Input, Modifier, SummonEvent, SummonListener, SummonStatus,
};

const STATUS_NEEDS_PERMISSION: u8 = 0;
const STATUS_READY: u8 = 1;

unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> u8;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    /// The correct permission gate (10.15+) for listen-only keyboard event monitoring: Input Monitoring.
    fn CGPreflightListenEventAccess() -> u8;
    fn CGRequestListenEventAccess() -> u8;
    /// core-graphics doesn't export the declaration; declare it ourselves to re-enable after TapDisabled.
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
}

/// Accessibility permission (gate for write back/selection reading; M2).
pub fn is_accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

/// Show the system prompt once and add this app to the "Accessibility" list.
pub fn prompt_accessibility_permission() {
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let options = CFDictionary::from_CFType_pairs(&[(
            key.as_CFType(),
            CFBoolean::true_value().as_CFType(),
        )]);
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef());
    }
}

/// Whether Input Monitoring is granted — the gate for the listen-only keyboard tap.
pub fn is_input_monitoring_granted() -> bool {
    unsafe { CGPreflightListenEventAccess() != 0 }
}

/// Make a transparent window recompute its AppKit window shadow (ADR-0016).
///
/// The shadow is generated from the window content's alpha and cached: the webview may not have finished painting before a transparent window's first show,
/// so the shadow would be computed from "empty/rectangular content"; call this once after each show to refresh it to the current shape.
/// CSS shadows get clipped at window bounds in a transparent window, so the system shadow provides the "lift".
///
/// # Safety
/// `ns_window` must be a valid `NSWindow` pointer (Tauri's `Window::ns_window()`).
pub unsafe fn refresh_window_shadow(ns_window: *mut std::ffi::c_void) {
    if ns_window.is_null() {
        return;
    }
    let window: &objc2::runtime::AnyObject = unsafe { &*ns_window.cast() };
    unsafe {
        let _: () = objc2::msg_send![window, setHasShadow: true];
        let _: () = objc2::msg_send![window, invalidateShadow];
    }
}

/// Show the system prompt once when unauthorized, and add this app to the "Input Monitoring" list.
pub fn request_input_monitoring() -> bool {
    unsafe { CGRequestListenEventAccess() != 0 }
}

/// Place a window top-anchored and horizontally centered on the screen under the mouse cursor
/// (ADR-0032, summon time: the user just summoned from that screen). macOS-native by necessity:
/// tao's `cursor_position()` mixes logical and physical units, so `monitor_from_point` misses on
/// multi-display setups and the panel used to fall back to the primary screen.
///
/// # Safety
/// `ns_window` must be a valid `NSWindow` pointer (Tauri's `Window::ns_window()`); call on the
/// main thread.
pub unsafe fn place_window_near_top(ns_window: *mut std::ffi::c_void, width_points: f64) {
    if ns_window.is_null() {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(screen) = screen_containing(mtm, NSEvent::mouseLocation()) else {
        return;
    };
    // SAFETY: the caller guarantees a valid NSWindow pointer (see # Safety).
    let window: &NSWindow = unsafe { &*ns_window.cast() };
    anchor_near_top(window, &screen, width_points);
}

/// Re-anchor a window top-centered on the screen it currently sits on (ADR-0032, resize time: the
/// pointer may have wandered to another display while the user is typing, so it must not drag the
/// panel along).
///
/// # Safety
/// Same as [`place_window_near_top`].
pub unsafe fn place_window_near_top_keeping_screen(
    ns_window: *mut std::ffi::c_void,
    width_points: f64,
) {
    if ns_window.is_null() {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    // SAFETY: the caller guarantees a valid NSWindow pointer (see # Safety).
    let window: &NSWindow = unsafe { &*ns_window.cast() };
    let frame = window.frame();
    let center = NSPoint::new(
        frame.origin.x + frame.size.width / 2.0,
        frame.origin.y + frame.size.height / 2.0,
    );
    let Some(screen) = screen_containing(mtm, center) else {
        return;
    };
    anchor_near_top(window, &screen, width_points);
}

/// Top-center a window on one screen: the shared body of the two placements above.
fn anchor_near_top(window: &NSWindow, screen: &NSScreen, width_points: f64) {
    let visible = screen.visibleFrame();
    let anchor = crate::screen::panel_anchor(
        visible.origin.x,
        visible.size.width,
        visible.size.height,
        width_points,
    );
    let top_left = NSPoint::new(
        anchor.left,
        visible.origin.y + visible.size.height - anchor.top_drop,
    );
    let frame = screen.frame();
    eprintln!(
        "moe: panel anchored on screen ({:.0},{:.0} {:.0}x{:.0}) at ({:.0}, {:.0})",
        frame.origin.x, frame.origin.y, frame.size.width, frame.size.height, top_left.x, top_left.y,
    );
    window.setFrameTopLeftPoint(top_left);
}

/// Right-dock a window on the screen under the mouse cursor: flush to the right edge, full visible
/// height and width `width_points` (the Side View's default frame, ADR-0032).
///
/// # Safety
/// Same as [`place_window_near_top`].
pub unsafe fn place_window_right_docked(ns_window: *mut std::ffi::c_void, width_points: f64) {
    if ns_window.is_null() {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(screen) = screen_containing(mtm, NSEvent::mouseLocation()) else {
        return;
    };
    let visible = screen.visibleFrame();
    let frame = NSRect::new(
        NSPoint::new(
            visible.origin.x + visible.size.width - width_points,
            visible.origin.y,
        ),
        NSSize::new(width_points, visible.size.height),
    );
    // SAFETY: the caller guarantees a valid NSWindow pointer (see # Safety).
    let window: &NSWindow = unsafe { &*ns_window.cast() };
    window.setFrame_display(frame, true);
}

/// The screen containing `point`, falling back to the main screen (the point sits outside every
/// screen only in exotic arrangements or races).
fn screen_containing(
    mtm: MainThreadMarker,
    point: NSPoint,
) -> Option<objc2::rc::Retained<NSScreen>> {
    for screen in NSScreen::screens(mtm).iter() {
        let frame = screen.frame();
        let inside = point.x >= frame.origin.x
            && point.x < frame.origin.x + frame.size.width
            && point.y >= frame.origin.y
            && point.y < frame.origin.y + frame.size.height;
        if inside {
            return Some(screen);
        }
    }
    NSScreen::mainScreen(mtm)
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
        "moe: summon listener started (double-tap {}, max gap {:?})",
        key.label(),
        max_gap
    );
    eprintln!(
        "moe: input monitoring preflight = {}; accessibility = {}",
        is_input_monitoring_granted(),
        is_accessibility_trusted()
    );
    if !is_input_monitoring_granted() {
        eprintln!("moe: requesting system permission (input monitoring)…");
        request_input_monitoring();
    }

    // RefCell suffices: tap callbacks run on the same thread as the run loop.
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
                        eprintln!("moe: detected double-tap {} → summoning", key.label());
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
                eprintln!(
                    "moe: CGEventTap mounted; double-tap {} to summon",
                    key.label()
                );
                logged_failure = false;
                handler(SummonEvent::Authorized);
                let source = tap
                    .mach_port
                    .create_runloop_source(0)
                    .expect("failed to create run loop source");
                CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });
                tap.enable();
                CFRunLoop::run_current();
                // Tap became invalid (e.g. after system sleep): clear the port and rebuild
                state.borrow_mut().port = None;
                status.store(STATUS_NEEDS_PERMISSION, Ordering::SeqCst);
            }
            Err(()) => {
                // Not authorized / temporary failure: retry with backoff; no restart needed once permission takes effect (some system versions still need one restart)
                if !logged_failure {
                    eprintln!(
                        "moe: CGEventTap creation failed (usually because \"Input Monitoring\" is not granted), retrying every second…"
                    );
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
