//! X11 double-tap modifier summon (IIE4AD-350): XRecord captures global keys, no permission needed.
//!
//! The top half is pure logic (key event translation, keysym→keycode mapping), independent of the X11 library,
//! so it compiles and unit-tests cross-platform; the bottom half (`cfg(target_os = "linux")`) is the X11 glue.
//! Detection semantics don't live here: always reuse [`crate::summon::DoubleTapDetector`].
//!
//! Under a Wayland session XRecord only sees XWayland clients, so summoning is unreliable: don't start the listener,
//! report `Unsupported`, and let the panel guide the user to a combo key or a WM binding for `moe --toggle`.

use crate::summon::Modifier;

/// Modifier → candidate keysyms (⌘ corresponds to Super on Linux).
pub fn keysyms_of(modifier: Modifier) -> &'static [u32] {
    match modifier {
        // Super_L / Super_R
        Modifier::Meta => &[0xffeb, 0xffec],
        // Alt_L / Alt_R
        Modifier::Alt => &[0xffe9, 0xffea],
        // Control_L / Control_R
        Modifier::Control => &[0xffe3, 0xffe4],
        // Shift_L / Shift_R
        Modifier::Shift => &[0xffe1, 0xffe2],
    }
}

/// Find every keycode offset for the modifier in the `GetKeyboardMapping` result
/// (`mapping` is in ascending keycode order, `per_keycode` keysyms per keycode).
pub fn keycodes_for(mapping: &[u32], per_keycode: usize, modifier: Modifier) -> Vec<u8> {
    let targets = keysyms_of(modifier);
    if per_keycode == 0 {
        return Vec::new();
    }
    mapping
        .chunks(per_keycode)
        .enumerate()
        .filter(|(_, keysyms)| keysyms.iter().any(|keysym| targets.contains(keysym)))
        .map(|(index, _)| index as u8)
        .collect()
}

/// One raw key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawKey {
    pub keycode: u8,
    pub pressed: bool,
}

/// XRecord data → key event: only the core protocol's KeyPress(2) / KeyRelease(3);
/// frames such as StartOfData/EndOfData (empty or non-key data) return None.
pub fn parse_key_event(data: &[u8]) -> Option<RawKey> {
    let kind = *data.first()?;
    if kind != 2 && kind != 3 {
        return None;
    }
    Some(RawKey {
        keycode: *data.get(1)?,
        pressed: kind == 2,
    })
}

#[cfg(target_os = "linux")]
mod linux {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::time::{Duration, Instant};

    use x11rb::connection::{Connection, RequestConnection};
    use x11rb::protocol::record;
    use x11rb::protocol::record::ConnectionExt as _;
    use x11rb::protocol::xproto::ConnectionExt as _;

    use super::{keycodes_for, parse_key_event};
    use crate::summon::{
        DoubleTapDetector, Input, Modifier, SummonEvent, SummonListener, SummonStatus,
    };

    const STATUS_UNSUPPORTED: u8 = 0;
    const STATUS_READY: u8 = 1;

    /// Wayland session flag: XRecord only covers XWayland clients, so double-tap is unreliable.
    fn is_wayland_session() -> bool {
        std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|kind| kind == "wayland")
    }

    pub struct X11SummonListener {
        key: Modifier,
        max_gap: Duration,
        status: Arc<AtomicU8>,
    }

    impl X11SummonListener {
        pub fn new(key: Modifier, max_gap: Duration) -> Self {
            Self {
                key,
                max_gap,
                status: Arc::new(AtomicU8::new(STATUS_UNSUPPORTED)),
            }
        }
    }

    impl SummonListener for X11SummonListener {
        fn status(&self) -> SummonStatus {
            if self.status.load(Ordering::SeqCst) == STATUS_READY {
                SummonStatus::Ready
            } else {
                SummonStatus::Unsupported
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

    /// XRecord event loop: reads the `EnableContext` reply stream one frame at a time;
    /// x11rb's `RecordEnableContextCookie` handles frame splitting and ends at EndOfData.
    fn run_listener(
        key: Modifier,
        max_gap: Duration,
        status: Arc<AtomicU8>,
        handler: Box<dyn Fn(SummonEvent) + Send>,
    ) {
        if is_wayland_session() {
            eprintln!(
                "moe: detected a Wayland session — XRecord only sees XWayland clients; double-tap {} summoning is unreliable.\n\
                 moe: two alternatives: use a combo for [summon] key in config.toml; or bind `moe --toggle` in your WM",
                key.label()
            );
            return;
        }
        let (conn, _screen) = match x11rb::connect(None) {
            Ok(pair) => pair,
            Err(err) => {
                eprintln!(
                    "moe: cannot connect to X11 ({err}) — use a combo key instead, or bind `moe --toggle` in your WM"
                );
                return;
            }
        };
        if conn
            .extension_information(record::X11_EXTENSION_NAME)
            .ok()
            .flatten()
            .is_none()
        {
            eprintln!(
                "moe: X server lacks the RECORD extension; double-tap summoning is unavailable — use a combo key instead"
            );
            return;
        }

        // keycode → modifier (query the keyboard mapping once at startup, then only compare integers)
        let setup = conn.setup();
        let min_keycode = setup.min_keycode;
        let count = setup.max_keycode.saturating_sub(setup.min_keycode) + 1;
        let mapping = match conn
            .get_keyboard_mapping(min_keycode, count)
            .map_err(|err| err.to_string())
            .and_then(|cookie| cookie.reply().map_err(|err| err.to_string()))
        {
            Ok(reply) => reply,
            Err(err) => {
                eprintln!(
                    "moe: failed to read the keyboard mapping ({err}); double-tap summoning is unavailable"
                );
                return;
            }
        };
        let mut code_to_modifier: HashMap<u8, Modifier> = HashMap::new();
        let per_keycode = mapping.keysyms_per_keycode as usize;
        for modifier in [
            Modifier::Meta,
            Modifier::Alt,
            Modifier::Control,
            Modifier::Shift,
        ] {
            for offset in keycodes_for(&mapping.keysyms, per_keycode, modifier) {
                // When one keycode matches multiple modifiers, keep the first (Meta > Alt > Control > Shift)
                code_to_modifier
                    .entry(min_keycode + offset)
                    .or_insert(modifier);
            }
        }
        if !code_to_modifier.values().any(|m| *m == key) {
            eprintln!(
                "moe: no key for {} in the keyboard mapping (Super_L/R etc.); double-tap summoning is unavailable — use a combo key instead",
                key.label()
            );
            return;
        }

        // XRecord: all clients (client_spec = 0), only core KeyPress..KeyRelease
        let ranges = [record::Range {
            device_events: record::Range8 { first: 2, last: 3 },
            ..Default::default()
        }];
        let context = 0u32; // just needs to be unique within this client
        if let Err(err) = conn.record_create_context(context, 0, &[0u32], &ranges) {
            eprintln!("moe: failed to create XRecord context ({err})");
            return;
        }
        let replies = match conn.record_enable_context(context) {
            Ok(cookie) => cookie,
            Err(err) => {
                eprintln!("moe: failed to enable XRecord context ({err})");
                return;
            }
        };
        if let Err(err) = conn.flush() {
            eprintln!("moe: X11 flush failed ({err})");
            return;
        }

        status.store(STATUS_READY, Ordering::SeqCst);
        eprintln!("moe: XRecord mounted; double-tap {} to summon", key.label());
        handler(SummonEvent::Authorized);

        let mut detector = DoubleTapDetector::new(key, max_gap);
        for reply in replies {
            let reply = match reply {
                Ok(reply) => reply,
                Err(err) => {
                    eprintln!("moe: XRecord read failed ({err})");
                    break;
                }
            };
            let Some(raw) = parse_key_event(&reply.data) else {
                continue;
            };
            let input = match code_to_modifier.get(&raw.keycode) {
                Some(modifier) => {
                    if raw.pressed {
                        Input::Down(*modifier)
                    } else {
                        Input::Up(*modifier)
                    }
                }
                // Non-modifier key presses cancel the double-tap cycle (consistent with KeyDown → Other on macOS)
                None => {
                    if raw.pressed {
                        Input::Other
                    } else {
                        continue;
                    }
                }
            };
            if detector.feed(input, Instant::now()) {
                eprintln!("moe: detected double-tap {} → summoning", key.label());
                handler(SummonEvent::Summon);
            }
        }
        // EndOfData / X connection dropped (logout, lock, etc.): return to not-ready; the panel will show guidance again
        status.store(STATUS_UNSUPPORTED, Ordering::SeqCst);
        eprintln!("moe: XRecord listener ended (X connection dropped?)");
    }
}

#[cfg(target_os = "linux")]
pub use linux::X11SummonListener;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_modifier_keysyms_to_keycodes() {
        // 2 keysyms per keycode: shift / a / super
        let mapping = vec![0xffe1, 0xffe2, 0x0061, 0x0041, 0xffeb, 0xffec];
        assert_eq!(keycodes_for(&mapping, 2, Modifier::Shift), vec![0]);
        assert_eq!(keycodes_for(&mapping, 2, Modifier::Meta), vec![2]);
        assert!(keycodes_for(&mapping, 2, Modifier::Alt).is_empty());
        // per_keycode = 0 must not panic
        assert!(keycodes_for(&mapping, 0, Modifier::Meta).is_empty());
    }

    #[test]
    fn parses_key_events_only() {
        // KeyPress: type=2, detail(keycode)=38
        let mut press = vec![0u8; 32];
        press[0] = 2;
        press[1] = 38;
        assert_eq!(
            parse_key_event(&press),
            Some(RawKey {
                keycode: 38,
                pressed: true
            })
        );

        // KeyRelease: type=3
        let mut release = vec![0u8; 32];
        release[0] = 3;
        release[1] = 38;
        assert_eq!(
            parse_key_event(&release),
            Some(RawKey {
                keycode: 38,
                pressed: false
            })
        );

        // StartOfData/EndOfData (empty data) and data that is too short
        assert_eq!(parse_key_event(&[]), None);
        assert_eq!(parse_key_event(&[0]), None);
        assert_eq!(parse_key_event(&[2]), None);
        let mut other = vec![0u8; 32];
        other[0] = 4; // e.g. ButtonPress
        assert_eq!(parse_key_event(&other), None);
    }
}
