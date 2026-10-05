//! macOS 的 TextTarget 胶水：Accessibility 读写选区 + NSPasteboard/CGEvent 剪贴板降级。
//!
//! 可测的策略在 [`crate::text_target`]；这里只做平台调用翻译。

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
}

/// AX 属性名。现代 SDK 里 `kAX*Attribute` 是 `CFSTR("...")` 宏而非导出符号，
/// 直接构造同名 CFString（值为公开稳定的 API 契约）。
fn attribute(name: &str) -> CFString {
    CFString::new(name)
}

/// 当前系统焦点元素（copy 规则 +1，调用方负责 CFRelease）。
fn focused_element() -> Result<Option<AXUIElementRef>, PlatformError> {
    if !is_accessibility_trusted() {
        return Err(PlatformError::PermissionRequired);
    }
    // SAFETY: 标准 AX system-wide 查询；返回的引用按 copy 规则持有，由调用方释放。
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
        // SAFETY: focused 来自 copy 规则；读到的 value 也是 copy 规则，用 create 规则接管。
        unsafe {
            let name = attribute("AXSelectedText");
            let mut value: CFTypeRef = std::ptr::null();
            let err =
                AXUIElementCopyAttributeValue(focused, name.as_concrete_TypeRef(), &mut value);
            CFRelease(focused);
            if err != 0 || value.is_null() {
                // 应用不支持 AXSelectedText 或确实没有选区：都走光标模式
                return Ok(None);
            }
            let text = CFString::wrap_under_create_rule(value as CFStringRef).to_string();
            if text.is_empty() {
                Ok(None)
            } else {
                Ok(Some(text))
            }
        }
    }

    fn set_selected_text(&self, text: &str) -> Result<(), PlatformError> {
        let Some(focused) = focused_element()? else {
            return Err(PlatformError::Unsupported("no focused element"));
        };
        // SAFETY: 同上；写失败时交由 HybridTextTarget 降级剪贴板。
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
                Err(PlatformError::Unsupported("AXSelectedText write rejected"))
            } else {
                Ok(())
            }
        }
    }
}

pub struct MacClipboard;

impl Clipboard for MacClipboard {
    /// 剪贴板内容：`(类型, 数据)` 展平列表。
    /// 多 item 剪贴板（如 Finder 多选文件）降级为单 item——只保留每种类型的首个出现。
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
        // SAFETY: extern static 的读取。
        let ty = unsafe { NSPasteboardTypeString };
        pasteboard.setString_forType(&NSString::from_str(text), ty);
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

    fn restore(&self, snapshot: Self::Snapshot) {
        // 合成事件是异步投递：给目标应用一点时间消费粘贴，再写回剪贴板
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
