//! macOS summon listener: listen-only CGEventTap (ADR-0008).
//!
//! Event translation (flagsChanged → [`Input`]) is thin glue; the detection semantics live in
//! [`crate::summon::DoubleTapDetector`]. Creating the tap fails without permission, so we retry with backoff here —
//! it takes effect once the user grants permission, without a restart.

use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
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
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, NSObject, NSObjectProtocol};
use objc2::{AnyThread, DefinedClass, MainThreadMarker};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSEvent, NSEventMask, NSScreen, NSTrackingArea,
    NSTrackingAreaOptions, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
    NSVisualEffectState, NSVisualEffectView, NSWindow, NSWindowOrderingMode,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use std::cell::RefCell;
use std::ptr::NonNull;

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

/// The Side View card's inset and corner radius in points — must match ui/chat.html (`m-2`, 8px)
/// and styles.css (`rounded-2xl`, 16px).
const SIDE_VIEW_MATERIAL_INSET: f64 = 8.0;
const SIDE_VIEW_MATERIAL_RADIUS: f64 = 16.0;

/// Install the Side View's native frost behind its webview: an `NSVisualEffectView` inset and
/// rounded to the card's geometry, pinned to the *active* look.
///
/// macOS fades a window's behind-window backdrop out while the window is not key — the behavior
/// `NSVisualEffectView` exposes as `FollowsWindowActiveState`. The card's CSS `backdrop-filter`
/// samples that same backdrop, so the Side View lost its frost a moment after clicking outside
/// (the desktop showed through crisp and unblurred) and only recovered when clicked again. CSS has
/// no opt-out; a native effect view does (`state = active`), so on macOS the Side View's frost
/// lives here and the stylesheet drops its `backdrop-filter` for this window
/// (`html[data-platform="macos"]`).
///
/// # Safety
/// `ns_view` must be a valid webview `NSView` pointer (Tauri's `WebviewWindow::ns_view()`); call on
/// the main thread.
pub unsafe fn install_side_view_material(ns_view: *mut std::ffi::c_void) {
    if ns_view.is_null() {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let webview: &NSView = unsafe { &*ns_view.cast() };
    // SAFETY: the caller guarantees a valid webview `NSView` (see the doc comment).
    unsafe {
        let Some(host) = webview.superview() else {
            return;
        };
        let card = {
            let frame = webview.frame();
            NSRect::new(
                NSPoint::new(
                    frame.origin.x + SIDE_VIEW_MATERIAL_INSET,
                    frame.origin.y + SIDE_VIEW_MATERIAL_INSET,
                ),
                NSSize::new(
                    (frame.size.width - 2.0 * SIDE_VIEW_MATERIAL_INSET).max(0.0),
                    (frame.size.height - 2.0 * SIDE_VIEW_MATERIAL_INSET).max(0.0),
                ),
            )
        };
        // Idempotent: a second call (the show path re-asserts it) only re-frames the material.
        let material_class =
            AnyClass::get(c"NSVisualEffectView").expect("NSVisualEffectView exists");
        for subview in host.subviews().iter() {
            if subview.isKindOfClass(material_class) {
                subview.setFrame(card);
                return;
            }
        }
        let material = NSVisualEffectView::initWithFrame(mtm.alloc(), card);
        material.setMaterial(NSVisualEffectMaterial::UnderWindowBackground);
        material.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        material.setState(NSVisualEffectState::Active);
        // The insets are the struts: growing the window grows the material, keeping the 8px
        // margin.
        material.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        // Rounds the material itself (private but long-stable — window-vibrancy ships the same
        // call). Required: square material corners would leak frost into the card's corner cutouts
        // and turn the system shadow's rounded shape into a rectangle (ADR-0016).
        let _: () = objc2::msg_send![&material, setCornerRadius: SIDE_VIEW_MATERIAL_RADIUS];
        host.addSubview_positioned_relativeTo(
            &material,
            NSWindowOrderingMode::Below,
            Some(webview),
        );
    }
}

/// Make our panels key on the first click — a local mouse-down monitor.
///
/// AppKit's click-to-key path for a non-activating panel is not reliable: every so often a click
/// delivered to the Side View leaves the panel non-key, so the click lands (hover states, buttons)
/// but no caret appears and typing goes nowhere until the outside/inside ritual is repeated. The
/// show path proves `makeKeyWindow` always works; the monitor runs before dispatch, keys the
/// clicked window, and the click is then handled like a normal click in a key window.
///
/// A local monitor only sees events bound for this app, so this cannot touch other apps' windows,
/// and `makeKeyWindow` does not activate the app — the panels stay non-activating.
pub fn install_panel_click_to_key() {
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let handler = block2::RcBlock::new(|event: NonNull<NSEvent>| {
        // SAFETY: AppKit hands the monitor a valid event for the duration of the call.
        let event = unsafe { event.as_ref() };
        if let Some(mtm) = MainThreadMarker::new()
            && let Some(window) = event.window(mtm)
            && !window.isKeyWindow()
        {
            window.makeKeyWindow();
        }
        event as *const NSEvent as *mut NSEvent
    });
    unsafe {
        let monitor = NSEvent::addLocalMonitorForEventsMatchingMask_handler(
            NSEventMask::LeftMouseDown,
            &handler,
        );
        // The monitor and its block live for the process (there is nothing to uninstall them for).
        std::mem::forget(handler);
        if let Some(monitor) = monitor {
            std::mem::forget(monitor);
        }
    }
}

/// The tracking-area owner: AppKit sends `mouseEntered:` / `mouseExited:` to whatever object the
/// area names — it need not be a view — and with `ActiveAlways` those fire for the window under
/// the cursor even while the app is inactive.
#[derive(Default)]
struct SideViewPointerIvars {
    on_change: Option<Box<dyn Fn(bool)>>,
}

objc2::define_class!(
    #[unsafe(super(NSObject))]
    #[ivars = SideViewPointerIvars]
    struct SideViewPointerOwner;

    impl SideViewPointerOwner {
        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _event: &NSEvent) {
            if let Some(on_change) = &self.ivars().on_change {
                on_change(true);
            }
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            if let Some(on_change) = &self.ivars().on_change {
                on_change(false);
            }
        }
    }
);

impl SideViewPointerOwner {
    fn new(on_change: Box<dyn Fn(bool)>) -> Retained<Self> {
        let this = Self::alloc();
        let this = this.set_ivars(SideViewPointerIvars {
            on_change: Some(on_change),
        });
        unsafe { objc2::msg_send![super(this), init] }
    }
}

/// The Side View's hover as native tracking: while the window is not key the webview receives no
/// pointer events at all (WebKit gates its mouse tracking on key status), so hovering an unfocused
/// Side View did nothing. An `NSTrackingArea` with `ActiveAlways` delivers enter/exit for the
/// window under the cursor even when the app is inactive; each transition is evaluated back into
/// the page (`window.__moePointerInside`), driving the same `moe-pointer-inside` state the DOM
/// events drive while focused. `acceptsMouseMovedEvents` is switched on as well so the webview's
/// own hover styles recover too.
///
/// # Safety
/// `ns_view` must be a valid webview `NSView` pointer (Tauri's `WebviewWindow::ns_view()`); call on
/// the main thread.
pub unsafe fn install_side_view_pointer_tracking(
    ns_view: *mut std::ffi::c_void,
    on_change: Box<dyn Fn(bool)>,
) {
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    if ns_view.is_null() || MainThreadMarker::new().is_none() {
        return;
    }
    let webview: &NSView = unsafe { &*ns_view.cast() };
    let owner = SideViewPointerOwner::new(on_change);
    let area = unsafe {
        NSTrackingArea::initWithRect_options_owner_userInfo(
            NSTrackingArea::alloc(),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0)),
            NSTrackingAreaOptions::MouseEnteredAndExited
                | NSTrackingAreaOptions::ActiveAlways
                | NSTrackingAreaOptions::InVisibleRect,
            Some(&owner),
            None,
        )
    };
    webview.addTrackingArea(&area);
    if let Some(window) = webview.window() {
        window.setAcceptsMouseMovedEvents(true);
    }
    // AppKit does not retain the tracking area's owner: keep it alive for the process (one object).
    std::mem::forget(owner);
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
