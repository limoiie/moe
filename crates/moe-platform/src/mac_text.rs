//! macOS TextTarget glue: Accessibility read/write of the selection + NSPasteboard/CGEvent clipboard fallback.
//!
//! Testable strategy lives in [`crate::text_target`]; this file only translates platform calls.

use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
use objc2_foundation::{NSData, NSString};

use crate::PlatformError;
use crate::mac::is_accessibility_trusted;
use crate::text_target::{Ax, Clipboard, HybridTextTarget};

type AXUIElementRef = *const std::ffi::c_void;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout_in_seconds: f32) -> i32;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> i32;
    fn AXValueCreate(the_type: u32, value_ptr: *const std::ffi::c_void) -> CFTypeRef;
    fn AXValueGetValue(value: CFTypeRef, the_type: u32, value_ptr: *mut std::ffi::c_void) -> u8;
}

/// Type enum for AX range values (kAXValueTypeCFRange is a compile-time enum with no linked symbol);
/// CFRange's members are all CFIndex (i64 on 64-bit platforms).
const AX_VALUE_CF_RANGE: u32 = 4;

#[repr(C)]
struct CFRange {
    location: i64,
    length: i64,
}

/// AX attribute names. In modern SDKs, `kAX*Attribute` are `CFSTR("...")` macros rather than exported symbols,
/// so construct a CFString with the same name directly (the values are a public, stable API contract).
fn attribute(name: &str) -> CFString {
    CFString::new(name)
}

/// Per-query ceiling on AX *reads*: a healthy app answers in well under a millisecond, so this only
/// turns a stalled — or lazily woken and slow — AX server into a fast failure. Losing a read to the
/// timeout costs the empty-range gate at worst, never the capture: the clipboard fallback is the
/// safety net (MOE-0013). Measured caveat: the *first* message to an app (AX connection setup) is
/// not capped by this timeout (~170–190 ms on the apps tested) — that hang is why the app-tree
/// focus probe was retired rather than timeout-guarded.
const AX_MESSAGE_TIMEOUT: f32 = 0.05;
/// Wall-clock budget for one `AXSelectedText` walk (6 levels × 2 queries); past it the walk gives up
/// and the fallback decides — the walk exists to find selections AX won't hand over quickly, and
/// the fallback catches what the walk didn't reach.
const AX_READ_BUDGET: std::time::Duration = std::time::Duration::from_millis(100);

/// Bound how long a message to this element's process may take (the default is seconds-scale).
/// Applied along the read path; the write path's value-setting messages keep the default, where
/// waiting longer only delays an already-async write back.
fn bound_message_timeout(element: AXUIElementRef) {
    unsafe {
        AXUIElementSetMessagingTimeout(element, AX_MESSAGE_TIMEOUT);
    }
}

/// `AXFocusedUIElement` on `element` (copy rule +1 on success; the caller owns the CFRelease).
///
/// # Safety
/// `element` must be a valid AX element.
unsafe fn focused_ui_element(element: AXUIElementRef) -> Option<AXUIElementRef> {
    let name = attribute("AXFocusedUIElement");
    let mut focused: CFTypeRef = std::ptr::null();
    let err =
        unsafe { AXUIElementCopyAttributeValue(element, name.as_concrete_TypeRef(), &mut focused) };
    if err != 0 || focused.is_null() {
        return None;
    }
    Some(focused as AXUIElementRef)
}

/// The current system-focused element (copy rule +1; the caller owns the CFRelease).
fn focused_element() -> Result<Option<AXUIElementRef>, PlatformError> {
    if !is_accessibility_trusted() {
        return Err(PlatformError::PermissionRequired);
    }
    // SAFETY: standard AX query; the returned reference is held per the copy rule and released by the caller.
    unsafe {
        let system = AXUIElementCreateSystemWide();
        bound_message_timeout(system);
        let focused = focused_ui_element(system);
        CFRelease(system);
        Ok(focused)
    }
}

pub struct MacAx;

impl Ax for MacAx {
    fn selected_text(&self) -> Result<Option<String>, PlatformError> {
        let Some(focused) = focused_element()? else {
            return Ok(None);
        };
        // SAFETY: focused comes from the copy rule; the value read is also copy rule, taken over with the create rule.
        unsafe {
            let selected = attribute("AXSelectedText");
            let parent = attribute("AXParent");
            let mut element: AXUIElementRef = focused;
            bound_message_timeout(element);
            // Some apps (browsers/rich-text editors) put system focus on a child element while the selection lives on an ancestor:
            // walk up AXParent (at most 5 levels), logging each level to distinguish "no selection" from "not supported".
            // The walk is wall-clock bounded: each query on a stalled AX server costs up to the
            // messaging timeout, and the clipboard fallback catches a selection the walk didn't reach.
            let deadline = std::time::Instant::now() + AX_READ_BUDGET;
            for depth in 0..=5 {
                if std::time::Instant::now() >= deadline {
                    eprintln!(
                        "moe: AXSelectedText walk hit its {} ms budget at level {depth} (falling back)",
                        AX_READ_BUDGET.as_millis()
                    );
                    CFRelease(element);
                    return Ok(None);
                }
                let mut value: CFTypeRef = std::ptr::null();
                let err = AXUIElementCopyAttributeValue(
                    element,
                    selected.as_concrete_TypeRef(),
                    &mut value,
                );
                if err == 0 && !value.is_null() {
                    let text = CFString::wrap_under_create_rule(value as CFStringRef).to_string();
                    CFRelease(element);
                    if !text.is_empty() {
                        if depth > 0 {
                            eprintln!("moe: AXSelectedText hit at ancestor level {depth}");
                        }
                        return Ok(Some(text));
                    }
                    eprintln!("moe: AXSelectedText exists but is empty (level {depth})");
                    return Ok(None);
                }
                eprintln!("moe: AXSelectedText failed at level {depth} (AXError {err})");
                let mut up: CFTypeRef = std::ptr::null();
                let err =
                    AXUIElementCopyAttributeValue(element, parent.as_concrete_TypeRef(), &mut up);
                CFRelease(element);
                if err != 0 || up.is_null() {
                    return Ok(None);
                }
                element = up as AXUIElementRef;
                bound_message_timeout(element);
            }
            CFRelease(element);
            Ok(None)
        }
    }

    fn set_selected_text(&self, text: &str) -> Result<(), PlatformError> {
        let Some(focused) = focused_element()? else {
            return Err(PlatformError::Unsupported("no focused element"));
        };
        // SAFETY: same as above; on write failure, HybridTextTarget falls back to the clipboard.
        unsafe {
            let name = attribute("AXSelectedText");
            let cf = CFString::new(text);
            let err = AXUIElementSetAttributeValue(
                focused,
                name.as_concrete_TypeRef(),
                cf.as_concrete_TypeRef() as CFTypeRef,
            );
            CFRelease(focused);
            if err != 0 {
                eprintln!("moe: AXSelectedText write failed (AXError {err})");
                Err(PlatformError::Unsupported("AXSelectedText write rejected"))
            } else {
                Ok(())
            }
        }
    }

    fn selected_text_range(&self) -> Option<(i64, i64)> {
        // Best-effort: failures/unsupported both return None (skip selecting after write back)
        let focused = match focused_element() {
            Ok(Some(f)) => f,
            Ok(None) => {
                eprintln!("moe: reading AX range: no focused element");
                return None;
            }
            Err(err) => {
                eprintln!("moe: reading AX range: {err}");
                return None;
            }
        };
        // SAFETY: value is held per the copy rule; AXValue unpacks a CFRange.
        unsafe {
            bound_message_timeout(focused);
            let name = attribute("AXSelectedTextRange");
            let mut value: CFTypeRef = std::ptr::null();
            let err =
                AXUIElementCopyAttributeValue(focused, name.as_concrete_TypeRef(), &mut value);
            CFRelease(focused);
            if err != 0 || value.is_null() {
                eprintln!(
                    "moe: AXSelectedTextRange read failed (AXError {err}, null value {})",
                    value.is_null()
                );
                return None;
            }
            let mut range = CFRange {
                location: 0,
                length: 0,
            };
            let ok = AXValueGetValue(
                value,
                AX_VALUE_CF_RANGE,
                &mut range as *mut CFRange as *mut std::ffi::c_void,
            );
            CFRelease(value);
            if ok == 0 {
                eprintln!("moe: AXValueGetValue(CFRange) unpack failed");
                return None;
            }
            eprintln!(
                "moe: read AX range = ({}, {})",
                range.location, range.length
            );
            Some((range.location, range.length))
        }
    }

    fn set_selected_range(&self, location: i64, length: i64) {
        // Some apps commit edits asynchronously: setting the range immediately gets overridden by the cursor move that follows
        std::thread::sleep(std::time::Duration::from_millis(60));
        let Ok(Some(focused)) = focused_element() else {
            eprintln!("moe: setting AX range: no focused element");
            return;
        };
        // SAFETY: standard AX write; failures are logged but don't count as a write-back failure.
        unsafe {
            let range = CFRange { location, length };
            let value = AXValueCreate(
                AX_VALUE_CF_RANGE,
                &range as *const CFRange as *const std::ffi::c_void,
            );
            if value.is_null() {
                eprintln!("moe: AXValueCreate(CFRange) failed");
                CFRelease(focused);
                return;
            }
            let name = attribute("AXSelectedTextRange");
            let err = AXUIElementSetAttributeValue(focused, name.as_concrete_TypeRef(), value);
            CFRelease(value);
            CFRelease(focused);
            eprintln!("moe: set AX range = ({location}, {length}) → AXError {err}");
        }
    }
}

/// Synthesized ⌘V is delivered asynchronously: let the target app read the clipboard before the caller restores the snapshot.
const PASTE_SETTLE: std::time::Duration = std::time::Duration::from_millis(150);

#[derive(Clone)]
pub struct MacClipboard;

/// Watch over an in-flight synthesized ⌘C: the pasteboard's version counter at the copy and, once
/// the target app's write is observed, at it (kept so a later write by anyone else is visible).
pub struct MacCopyWatch {
    before: isize,
    landed_at: Option<isize>,
}

impl Clipboard for MacClipboard {
    /// Clipboard content: a flattened list of `(type, data)`.
    /// Multi-item clipboards (e.g. multi-file selection in Finder) degrade to one item — only the first occurrence of each type is kept.
    /// Data is owned bytes rather than `NSData` references: the snapshot may finish on another
    /// thread (MOE-0013), and `Retained<NSData>` is not Send (NSData has a mutable subclass).
    type Snapshot = Vec<(String, Vec<u8>)>;
    type CopyWatch = MacCopyWatch;

    fn snapshot(&self) -> Self::Snapshot {
        let pasteboard = NSPasteboard::generalPasteboard();
        let Some(items) = pasteboard.pasteboardItems() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for item in items.iter() {
            let types = item.types();
            for ty in types.iter() {
                let key = ty.to_string();
                if seen.contains(&key) {
                    continue;
                }
                if let Some(data) = item.dataForType(&ty) {
                    seen.push(key.clone());
                    out.push((key, data.to_vec()));
                }
            }
        }
        out
    }

    fn set_text(&self, text: &str) {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        // SAFETY: reading an extern static.
        let ty = unsafe { NSPasteboardTypeString };
        pasteboard.setString_forType(&NSString::from_str(text), ty);
    }

    fn read_text(&self) -> Option<String> {
        clipboard_read_text().filter(|text| !text.is_empty())
    }

    fn paste(&self) {
        const VK_V: CGKeyCode = 9; // kVK_ANSI_V
        let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
            return;
        };
        for keydown in [true, false] {
            if let Ok(event) = CGEvent::new_keyboard_event(source.clone(), VK_V, keydown) {
                event.set_flags(CGEventFlags::CGEventFlagCommand);
                event.post(CGEventTapLocation::HID);
            }
        }
        // Give the target app a moment to consume the paste before the caller writes the snapshot back
        std::thread::sleep(PASTE_SETTLE);
    }

    fn copy_begin(&self) -> MacCopyWatch {
        const VK_C: CGKeyCode = 8; // kVK_ANSI_C
        // Synthesized events are delivered asynchronously: the caller observes the pasteboard's
        // version counter — the target app's write is visible through it, "nothing selected"
        // simply never writes. Recording the counter here (before posting) is what lets the far
        // side of a split capture tell our copy apart from any later write.
        let before = NSPasteboard::generalPasteboard().changeCount();
        if let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) {
            for keydown in [true, false] {
                if let Ok(event) = CGEvent::new_keyboard_event(source.clone(), VK_C, keydown) {
                    event.set_flags(CGEventFlags::CGEventFlagCommand);
                    event.post(CGEventTapLocation::HID);
                }
            }
        }
        MacCopyWatch {
            before,
            landed_at: None,
        }
    }

    fn copy_landed(&self, watch: &mut MacCopyWatch) -> bool {
        if watch.landed_at.is_some() {
            return true;
        }
        let now = NSPasteboard::generalPasteboard().changeCount();
        if now == watch.before {
            return false;
        }
        watch.landed_at = Some(now);
        true
    }

    fn changed_since_copied(&self, watch: &MacCopyWatch) -> bool {
        match watch.landed_at {
            Some(at) => NSPasteboard::generalPasteboard().changeCount() != at,
            None => false, // never landed: nothing of ours to protect
        }
    }

    fn restore(&self, snapshot: Self::Snapshot) {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        for (ty, bytes) in &snapshot {
            let data = NSData::with_bytes(bytes);
            pasteboard.setData_forType(Some(&data), &NSString::from_str(ty));
        }
    }
}

pub type MacTextTarget = HybridTextTarget<MacAx, MacClipboard>;

pub fn mac_text_target() -> MacTextTarget {
    HybridTextTarget::new(MacAx, MacClipboard)
}

/// Write text to the system clipboard ("copy"-style actions and the write-back fallback share this implementation, IIE4AD-364).
pub fn copy_text(text: &str) {
    MacClipboard.set_text(text);
}

/// Read clipboard text back (for tests and diagnostics).
pub fn clipboard_read_text() -> Option<String> {
    let pasteboard = NSPasteboard::generalPasteboard();
    // SAFETY: reading an extern static.
    let ty = unsafe { NSPasteboardTypeString };
    pasteboard.stringForType(ty).map(|text| text.to_string())
}
