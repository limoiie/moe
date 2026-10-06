//! Summon-key detection (ADR-0008): platform-agnostic double-tap modifier detection.
//!
//! Platform glue (macOS CGEventTap / X11) only translates native events into [`Input`];
//! all the detection semantics live in this testable pure state machine.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    Meta,
    Alt,
    Control,
    Shift,
}

impl Modifier {
    /// Display symbol (logs and Hints).
    pub fn label(&self) -> &'static str {
        match self {
            Self::Meta => "⌘",
            Self::Alt => "⌥",
            Self::Control => "⌃",
            Self::Shift => "⇧",
        }
    }
}

/// One input from the platform keyboard listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Down(Modifier),
    Up(Modifier),
    /// Any non-modifier key press, or a flag change on another modifier.
    Other,
}

enum State {
    Idle,
    FirstHeld(Instant),
    AwaitSecond(Instant),
    SecondHeld,
}

/// Double-tap modifier detection: `feed` returning true means the Command Panel should be summoned.
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
                // The window expired: this press starts a new cycle
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
            // Any interfering input (other keys, other modifier changes) cancels the current cycle
            _ => {
                self.state = State::Idle;
                false
            }
        }
    }
}

/// Events the listener layer passes to the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummonEvent {
    /// Summon (a double-tap has been detected).
    Summon,
    /// The listener is actually mounted for the first time (macOS permission has taken effect; no restart needed).
    Authorized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummonStatus {
    Ready,
    NeedsPermission,
    Unsupported,
}

/// Summon listener (default: double-tap ⌘, ADR-0008). Implementations translate platform events into [`Input`].
pub trait SummonListener: Send + Sync {
    fn status(&self) -> SummonStatus;
    fn start(&mut self, handler: Box<dyn Fn(SummonEvent) + Send + 'static>);
}

/// Placeholder implementation: for platforms where the listener hasn't landed.
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

        // Single tap: press and release once; must not summon
        assert!(!d.feed(Input::Down(Modifier::Meta), t(t0, 0)));
        assert!(!d.feed(Input::Up(Modifier::Meta), t(t0, 30)));

        // The second press (within the threshold) summons immediately
        assert!(d.feed(Input::Down(Modifier::Meta), t(t0, 120)));
    }

    #[test]
    fn slow_double_tap_does_not_summon_and_rolling_window_rearms() {
        let t0 = Instant::now();
        let mut d = detector();

        d.feed(Input::Down(Modifier::Meta), t(t0, 0));
        d.feed(Input::Up(Modifier::Meta), t(t0, 30));

        // Second press is 970ms after the first release > 400ms threshold: no summon
        assert!(!d.feed(Input::Down(Modifier::Meta), t(t0, 1000)));
        d.feed(Input::Up(Modifier::Meta), t(t0, 1030));

        // The third press follows the second closely: with the second as the new start, it should summon
        assert!(d.feed(Input::Down(Modifier::Meta), t(t0, 1120)));
    }

    #[test]
    fn interference_and_long_hold_cancel_the_cycle() {
        let t0 = Instant::now();

        // Pressing another key after the first tap (e.g. C of ⌘C): the second tap must not summon
        let mut d = detector();
        d.feed(Input::Down(Modifier::Meta), t(t0, 0));
        d.feed(Input::Up(Modifier::Meta), t(t0, 30));
        d.feed(Input::Other, t(t0, 60));
        assert!(!d.feed(Input::Down(Modifier::Meta), t(t0, 100)));

        // Pressing another modifier also cancels (⌥ right after ⌘ means a shortcut, not a tap)
        let mut d = detector();
        d.feed(Input::Down(Modifier::Meta), t(t0, 0));
        d.feed(Input::Up(Modifier::Meta), t(t0, 30));
        d.feed(Input::Down(Modifier::Alt), t(t0, 50));
        assert!(!d.feed(Input::Down(Modifier::Meta), t(t0, 90)));

        // A hold longer than max_gap is not a tap: the immediately following press is not a double-tap
        let mut d = detector();
        d.feed(Input::Down(Modifier::Meta), t(t0, 0));
        d.feed(Input::Up(Modifier::Meta), t(t0, 500));
        assert!(!d.feed(Input::Down(Modifier::Meta), t(t0, 520)));
    }

    #[test]
    fn double_tap_hold_emits_at_press_and_triple_tap_summons_once() {
        let t0 = Instant::now();

        // Double-tap and hold: the second press summons immediately; releasing 2s later triggers nothing
        let mut d = detector();
        d.feed(Input::Down(Modifier::Meta), t(t0, 0));
        d.feed(Input::Up(Modifier::Meta), t(t0, 30));
        assert!(d.feed(Input::Down(Modifier::Meta), t(t0, 120)));
        assert!(!d.feed(Input::Up(Modifier::Meta), t(t0, 2120)));

        // Triple tap: summons exactly once
        let mut d = detector();
        d.feed(Input::Down(Modifier::Meta), t(t0, 0));
        d.feed(Input::Up(Modifier::Meta), t(t0, 30));
        assert!(d.feed(Input::Down(Modifier::Meta), t(t0, 100)));
        assert!(!d.feed(Input::Up(Modifier::Meta), t(t0, 130)));
        assert!(!d.feed(Input::Down(Modifier::Meta), t(t0, 200)));
        assert!(!d.feed(Input::Up(Modifier::Meta), t(t0, 230)));
    }
}
