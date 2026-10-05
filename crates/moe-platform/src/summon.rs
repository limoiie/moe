//! 呼出键检测（ADR-0008）：与平台无关的双击修饰键判定。
//!
//! 平台胶水（macOS CGEventTap / X11）只负责把原生事件翻译成 [`Input`]，
//! 判定语义全部在这个可测的纯状态机里。

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    Meta,
    Alt,
    Control,
    Shift,
}

impl Modifier {
    /// 展示用符号（日志与 Hints）。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Meta => "⌘",
            Self::Alt => "⌥",
            Self::Control => "⌃",
            Self::Shift => "⇧",
        }
    }
}

/// 来自平台键盘监听的一条输入。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Down(Modifier),
    Up(Modifier),
    /// 任何非修饰键按下，或其他修饰键的 flag 变化。
    Other,
}

enum State {
    Idle,
    FirstHeld(Instant),
    AwaitSecond(Instant),
    SecondHeld,
}

/// 双击修饰键检测：`feed` 返回 true 表示应当呼出 Command Panel。
pub struct DoubleTapDetector {
    key: Modifier,
    max_gap: Duration,
    state: State,
}

impl DoubleTapDetector {
    pub fn new(key: Modifier, max_gap: Duration) -> Self {
        Self {
            key,
            max_gap,
            state: State::Idle,
        }
    }

    pub fn feed(&mut self, input: Input, at: Instant) -> bool {
        match input {
            Input::Down(m) if m == self.key => match self.state {
                State::Idle => {
                    self.state = State::FirstHeld(at);
                    false
                }
                State::AwaitSecond(deadline) if at <= deadline => {
                    self.state = State::SecondHeld;
                    true
                }
                // 窗口已过期：本次按下成为新一轮的起点
                State::AwaitSecond(_) => {
                    self.state = State::FirstHeld(at);
                    false
                }
                _ => false,
            },
            Input::Up(m) if m == self.key => {
                self.state = match self.state {
                    State::FirstHeld(pressed) if at - pressed <= self.max_gap => {
                        State::AwaitSecond(at + self.max_gap)
                    }
                    _ => State::Idle,
                };
                false
            }
            // 任何干扰输入（其他按键、其他修饰键变化）取消当前周期
            _ => {
                self.state = State::Idle;
                false
            }
        }
    }
}

/// 监听层向应用传递的事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummonEvent {
    /// 呼出（双击已判定）。
    Summon,
    /// 首次真正挂上监听（macOS 授权已生效，无需重启）。
    Authorized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummonStatus {
    Ready,
    NeedsPermission,
    Unsupported,
}

/// 呼出监听（默认：双击 ⌘，ADR-0008）。实现负责把平台事件翻译成 [`Input`]。
pub trait SummonListener: Send + Sync {
    fn status(&self) -> SummonStatus;
    fn start(&mut self, handler: Box<dyn Fn(SummonEvent) + Send + 'static>);
}

/// 占位实现：监听未落地的平台。
pub struct UnsupportedSummon;

impl SummonListener for UnsupportedSummon {
    fn status(&self) -> SummonStatus {
        SummonStatus::Unsupported
    }

    fn start(&mut self, _handler: Box<dyn Fn(SummonEvent) + Send + 'static>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    fn detector() -> DoubleTapDetector {
        DoubleTapDetector::new(Modifier::Meta, Duration::from_millis(400))
    }

    #[test]
    fn double_tap_summons_single_tap_does_not() {
        let t0 = Instant::now();
        let mut d = detector();

        // 单击：只按下-松开一次，不应呼出
        assert!(!d.feed(Input::Down(Modifier::Meta), t(t0, 0)));
        assert!(!d.feed(Input::Up(Modifier::Meta), t(t0, 30)));

        // 第二次按下（在阈值内）立即呼出
        assert!(d.feed(Input::Down(Modifier::Meta), t(t0, 120)));
    }

    #[test]
    fn slow_double_tap_does_not_summon_and_rolling_window_rearms() {
        let t0 = Instant::now();
        let mut d = detector();

        d.feed(Input::Down(Modifier::Meta), t(t0, 0));
        d.feed(Input::Up(Modifier::Meta), t(t0, 30));

        // 第二次点击距第一次松开 970ms > 400ms 阈值：不呼出
        assert!(!d.feed(Input::Down(Modifier::Meta), t(t0, 1000)));
        d.feed(Input::Up(Modifier::Meta), t(t0, 1030));

        // 第三次点击紧跟第二次：以第二次为新起点，应当呼出
        assert!(d.feed(Input::Down(Modifier::Meta), t(t0, 1120)));
    }

    #[test]
    fn interference_and_long_hold_cancel_the_cycle() {
        let t0 = Instant::now();

        // 第一击后按了其他键（如 ⌘C 的 C）：第二次点击不应呼出
        let mut d = detector();
        d.feed(Input::Down(Modifier::Meta), t(t0, 0));
        d.feed(Input::Up(Modifier::Meta), t(t0, 30));
        d.feed(Input::Other, t(t0, 60));
        assert!(!d.feed(Input::Down(Modifier::Meta), t(t0, 100)));

        // 其他修饰键的按下同样取消（⌘ 后紧接着 ⌥，是在按快捷键而不是轻击）
        let mut d = detector();
        d.feed(Input::Down(Modifier::Meta), t(t0, 0));
        d.feed(Input::Up(Modifier::Meta), t(t0, 30));
        d.feed(Input::Down(Modifier::Alt), t(t0, 50));
        assert!(!d.feed(Input::Down(Modifier::Meta), t(t0, 90)));

        // 长按超过 max_gap 不算一次轻击：此后紧接的一击不构成双击
        let mut d = detector();
        d.feed(Input::Down(Modifier::Meta), t(t0, 0));
        d.feed(Input::Up(Modifier::Meta), t(t0, 500));
        assert!(!d.feed(Input::Down(Modifier::Meta), t(t0, 520)));
    }

    #[test]
    fn double_tap_hold_emits_at_press_and_triple_tap_summons_once() {
        let t0 = Instant::now();

        // 双击并按住：第二次按下立即呼出，2 秒后松开不再触发
        let mut d = detector();
        d.feed(Input::Down(Modifier::Meta), t(t0, 0));
        d.feed(Input::Up(Modifier::Meta), t(t0, 30));
        assert!(d.feed(Input::Down(Modifier::Meta), t(t0, 120)));
        assert!(!d.feed(Input::Up(Modifier::Meta), t(t0, 2120)));

        // 连击三次：只呼出一次
        let mut d = detector();
        d.feed(Input::Down(Modifier::Meta), t(t0, 0));
        d.feed(Input::Up(Modifier::Meta), t(t0, 30));
        assert!(d.feed(Input::Down(Modifier::Meta), t(t0, 100)));
        assert!(!d.feed(Input::Up(Modifier::Meta), t(t0, 130)));
        assert!(!d.feed(Input::Down(Modifier::Meta), t(t0, 200)));
        assert!(!d.feed(Input::Up(Modifier::Meta), t(t0, 230)));
    }
}
