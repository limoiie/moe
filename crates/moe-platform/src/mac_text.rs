//! macOS TextTarget glue: Accessibility read/write of the selection + NSPasteboard/CGEvent clipboard fallback.
//!
//! Testable strategy lives in [`crate::text_target`]; this file only translates platform calls.

use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use objc2::rc::Retained;
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
use objc2_foundation::{NSData, NSString};

use crate::PlatformError;
use crate::mac::is_accessibility_trusted;
use crate::text_target::{Ax, Clipboard, HybridTextTarget};

type AXUIElementRef = *const std::ffi::c_void;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
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

/// The current system-focused element (copy rule +1; the caller owns the CFRelease).
fn focused_element() -> Result<Option<AXUIElementRef>, PlatformError> {
    if !is_accessibility_trusted() {
        return Err(PlatformError::PermissionRequired);
    }
    // SAFETY: standard AX system-wide query; the returned reference is held per the copy rule and released by the caller.
    unsafe {
        let system = AXUIElementCreateSystemWide();
        let name = attribute("AXFocusedUIElement");
        let mut focused: CFTypeRef = std::ptr::null();
        let err = AXUIElementCopyAttributeValue(system, name.as_concrete_TypeRef(), &mut focused);
        CFRelease(system);
        if err != 0 || focused.is_null() {
            return Ok(None);
        }
        Ok(Some(focused as AXUIElementRef))
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
            // Some apps (browsers/rich-text editors) put system focus on a child element while the selection lives on an ancestor:
            // walk up AXParent (at most 5 levels), logging each level to distinguish "no selection" from "not supported".
            for depth in 0..=5 {
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

pub struct MacClipboard;

impl Clipboard for MacClipboard {
    /// Clipboard content: a flattened list of `(type, data)`.
    /// Multi-item clipboards (e.g. multi-file selection in Finder) degrade to one item — only the first occurrence of each type is kept.
    type Snapshot = Vec<(String, Retained<NSData>)>;

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
                    out.push((key, data));
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
    }

    fn copy(&self) {
        const VK_C: CGKeyCode = 8; // kVK_ANSI_C
        let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
            return;
        };
        for keydown in [true, false] {
            if let Ok(event) = CGEvent::new_keyboard_event(source.clone(), VK_C, keydown) {
                event.set_flags(CGEventFlags::CGEventFlagCommand);
                event.post(CGEventTapLocation::HID);
            }
        }
        // Synthesized events are delivered asynchronously: wait for the target app to put the selection on the clipboard before the caller reads it back
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    fn restore(&self, snapshot: Self::Snapshot) {
        // Synthesized events are delivered asynchronously: give the target app a moment to consume the paste, then write the clipboard back
        std::thread::sleep(std::time::Duration::from_millis(150));
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        for (ty, data) in &snapshot {
            pasteboard.setData_forType(Some(data), &NSString::from_str(ty));
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
