//! X11 双击修饰键呼出（IIE4AD-350）：XRecord 抓全局按键，免授权。
//!
//! 上半部分是纯逻辑（按键事件翻译、keysym→keycode 映射），不依赖 X11 库，
//! 跨平台编译与单测；下半部分（`cfg(target_os = "linux")`）才是 X11 胶水。
//! 判定语义不在本文件：一律复用 [`crate::summon::DoubleTapDetector`]。
//!
//! Wayland 会话下 XRecord 只能看到 XWayland 客户端，呼出不可靠：不启动监听，
//! 状态报 `Unsupported`，由面板引导改用组合键或 WM 绑定 `moe --toggle`。

use crate::summon::Modifier;

/// 修饰键 → 候选 keysym（Linux 上 ⌘ 的对应物是 Super）。
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

/// 从 `GetKeyboardMapping` 结果中找出该修饰键的全部 keycode 偏移
/// （`mapping` 按 keycode 升序、每 keycode 占 `per_keycode` 个 keysym）。
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

/// 一次原始按键事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawKey {
    pub keycode: u8,
    pub pressed: bool,
}

/// XRecord 数据 → 按键事件：只认核心协议的 KeyPress(2) / KeyRelease(3)；
/// StartOfData/EndOfData 等帧（data 为空或非按键）返回 None。
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

    /// Wayland 会话标志：XRecord 只覆盖 XWayland 客户端，双击不可靠。
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

    /// XRecord 事件循环：`EnableContext` 的 reply 流逐条阻塞读取，
    /// 由 x11rb 的 `RecordEnableContextCookie` 负责拆帧并在 EndOfData 结束。
    fn run_listener(
        key: Modifier,
        max_gap: Duration,
        status: Arc<AtomicU8>,
        handler: Box<dyn Fn(SummonEvent) + Send>,
    ) {
        if is_wayland_session() {
            eprintln!(
                "moe: 检测到 Wayland 会话——XRecord 只能看到 XWayland 客户端，双击 {} 呼出不可靠。\n\
                 moe: 两种替代：config.toml 的 [summon] key 改用组合键；或 WM 绑定 `moe --toggle`",
                key.label()
            );
            return;
        }
        let (conn, _screen) = match x11rb::connect(None) {
            Ok(pair) => pair,
            Err(err) => {
                eprintln!(
                    "moe: 无法连接 X11（{err}）——请改用组合键呼出，或用 WM 绑定 `moe --toggle`"
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
            eprintln!("moe: X 服务缺少 RECORD 扩展，双击呼出不可用——请改用组合键");
            return;
        }

        // keycode → 修饰键（启动时查一次键盘映射，之后只做整数比较）
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
                eprintln!("moe: 读取键盘映射失败（{err}），双击呼出不可用");
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
                // 同一 keycode 命中多个修饰键时保留先到的（Meta > Alt > Control > Shift）
                code_to_modifier
                    .entry(min_keycode + offset)
                    .or_insert(modifier);
            }
        }
        if !code_to_modifier.values().any(|m| *m == key) {
            eprintln!(
                "moe: 键盘映射里没有 {} 对应的键（Super_L/R 等），双击呼出不可用——请改用组合键",
                key.label()
            );
            return;
        }

        // XRecord：全部客户端（client_spec = 0），只收核心 KeyPress..KeyRelease
        let ranges = [record::Range {
            device_events: record::Range8 { first: 2, last: 3 },
            ..Default::default()
        }];
        let context = 0u32; // 客户端内唯一即可
        if let Err(err) = conn.record_create_context(context, 0, &[0u32], &ranges) {
            eprintln!("moe: 创建 XRecord context 失败（{err}）");
            return;
        }
        let replies = match conn.record_enable_context(context) {
            Ok(cookie) => cookie,
            Err(err) => {
                eprintln!("moe: 启用 XRecord context 失败（{err}）");
                return;
            }
        };
        if let Err(err) = conn.flush() {
            eprintln!("moe: X11 flush 失败（{err}）");
            return;
        }

        status.store(STATUS_READY, Ordering::SeqCst);
        eprintln!("moe: XRecord 已挂载，双击 {} 可呼出", key.label());
        handler(SummonEvent::Authorized);

        let mut detector = DoubleTapDetector::new(key, max_gap);
        for reply in replies {
            let reply = match reply {
                Ok(reply) => reply,
                Err(err) => {
                    eprintln!("moe: XRecord 读取失败（{err}）");
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
                // 非修饰键的按下取消双击周期（与 macOS 侧 KeyDown → Other 一致）
                None => {
                    if raw.pressed {
                        Input::Other
                    } else {
                        continue;
                    }
                }
            };
            if detector.feed(input, Instant::now()) {
                eprintln!("moe: 检测到双击 {} → 呼出", key.label());
                handler(SummonEvent::Summon);
            }
        }
        // EndOfData / X 连接断开（注销、锁屏等）：回到未就绪，面板里会重新显示引导
        status.store(STATUS_UNSUPPORTED, Ordering::SeqCst);
        eprintln!("moe: XRecord 监听结束（X 连接已断开？）");
    }
}

#[cfg(target_os = "linux")]
pub use linux::X11SummonListener;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_modifier_keysyms_to_keycodes() {
        // 每 keycode 2 个 keysym：shift / a / super
        let mapping = vec![0xffe1, 0xffe2, 0x0061, 0x0041, 0xffeb, 0xffec];
        assert_eq!(keycodes_for(&mapping, 2, Modifier::Shift), vec![0]);
        assert_eq!(keycodes_for(&mapping, 2, Modifier::Meta), vec![2]);
        assert!(keycodes_for(&mapping, 2, Modifier::Alt).is_empty());
        // per_keycode = 0 不应 panic
        assert!(keycodes_for(&mapping, 0, Modifier::Meta).is_empty());
    }

    #[test]
    fn parses_key_events_only() {
        // KeyPress：type=2, detail(keycode)=38
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

        // KeyRelease：type=3
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

        // StartOfData/EndOfData（data 为空）与过短数据
        assert_eq!(parse_key_event(&[]), None);
        assert_eq!(parse_key_event(&[0]), None);
        assert_eq!(parse_key_event(&[2]), None);
        let mut other = vec![0u8; 32];
        other[0] = 4; // ButtonPress 之类
        assert_eq!(parse_key_event(&other), None);
    }
}
