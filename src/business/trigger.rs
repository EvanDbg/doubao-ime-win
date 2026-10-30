//! Platform-independent detection of single-tap, double-tap and hold gestures.
//!
//! Design inspired by the TriggerDetector proposed by @d4renk in PR #2.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerStyle {
    Single,
    Double,
    Hold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FireEdge {
    Press,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyInput {
    Press(u32),
    Release(u32),
    ForeignPress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerEvent {
    Toggle,
    Start,
    Stop,
}

pub struct TriggerDetector {
    pressed: Option<u32>,
    chorded: bool,
    hold_active: bool,
    last_tap: Option<Instant>,
}

impl TriggerDetector {
    pub fn new() -> Self {
        Self {
            pressed: None,
            chorded: false,
            hold_active: false,
            last_tap: None,
        }
    }

    pub fn handle(
        &mut self,
        input: KeyInput,
        style: TriggerStyle,
        interval: Duration,
        edge: FireEdge,
        now: Instant,
    ) -> Option<TriggerEvent> {
        match input {
            KeyInput::Press(id) => {
                if self.pressed == Some(id) {
                    return None;
                }
                if self.pressed.is_some() {
                    self.chorded = true;
                    return None;
                }

                self.pressed = Some(id);
                self.chorded = false;
                if style == TriggerStyle::Hold {
                    self.hold_active = true;
                    Some(TriggerEvent::Start)
                } else if edge == FireEdge::Press {
                    self.tap(style, interval, now)
                } else {
                    None
                }
            }
            KeyInput::ForeignPress => {
                if self.pressed.is_some() {
                    self.chorded = true;
                }
                None
            }
            KeyInput::Release(id) => {
                if self.pressed != Some(id) {
                    return None;
                }

                self.pressed = None;
                let chorded = std::mem::take(&mut self.chorded);
                if self.hold_active {
                    self.hold_active = false;
                    Some(TriggerEvent::Stop)
                } else if style != TriggerStyle::Hold && edge == FireEdge::Release && !chorded {
                    self.tap(style, interval, now)
                } else {
                    None
                }
            }
        }
    }

    pub fn reset(&mut self) -> Option<TriggerEvent> {
        let was_holding = self.hold_active;
        *self = Self::new();
        if was_holding {
            Some(TriggerEvent::Stop)
        } else {
            None
        }
    }

    fn tap(
        &mut self,
        style: TriggerStyle,
        interval: Duration,
        now: Instant,
    ) -> Option<TriggerEvent> {
        match style {
            TriggerStyle::Single => Some(TriggerEvent::Toggle),
            TriggerStyle::Double => {
                if self
                    .last_tap
                    .map(|last| now.duration_since(last) <= interval)
                    .unwrap_or(false)
                {
                    self.last_tap = None;
                    Some(TriggerEvent::Toggle)
                } else {
                    self.last_tap = Some(now);
                    None
                }
            }
            TriggerStyle::Hold => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use FireEdge::{Press as OnPress, Release as OnRelease};
    use KeyInput::{ForeignPress, Press, Release};
    use TriggerEvent::{Start, Stop, Toggle};
    use TriggerStyle::{Double, Hold, Single};

    const INTERVAL: Duration = Duration::from_millis(300);

    struct Gesture {
        detector: TriggerDetector,
        style: TriggerStyle,
        edge: FireEdge,
        base: Instant,
    }

    impl Gesture {
        fn new(style: TriggerStyle, edge: FireEdge) -> Self {
            Self {
                detector: TriggerDetector::new(),
                style,
                edge,
                base: Instant::now(),
            }
        }

        fn input(&mut self, input: KeyInput, millis: u64) -> Option<TriggerEvent> {
            self.detector.handle(
                input,
                self.style,
                INTERVAL,
                self.edge,
                self.base + Duration::from_millis(millis),
            )
        }

        fn tap(&mut self, millis: u64) -> Option<TriggerEvent> {
            let press = self.input(Press(1), millis);
            let release = self.input(Release(1), millis);
            match self.edge {
                OnPress => {
                    assert_eq!(release, None);
                    press
                }
                OnRelease => {
                    assert_eq!(press, None);
                    release
                }
            }
        }
    }

    #[test]
    fn single_fires_on_release() {
        let mut gesture = Gesture::new(Single, OnRelease);
        assert_eq!(gesture.input(Press(1), 0), None);
        assert_eq!(gesture.input(Release(1), 10), Some(Toggle));
    }

    #[test]
    fn single_fires_on_press() {
        let mut gesture = Gesture::new(Single, OnPress);
        assert_eq!(gesture.input(Press(1), 0), Some(Toggle));
        assert_eq!(gesture.input(Press(1), 10), None);
        assert_eq!(gesture.input(Release(1), 20), None);
    }

    #[test]
    fn single_suppressed_after_foreign_press() {
        let mut gesture = Gesture::new(Single, OnRelease);
        for key in [Press(1), ForeignPress, Press(1), Release(1)] {
            assert_eq!(gesture.input(key, 0), None);
        }
        assert_eq!(gesture.tap(10), Some(Toggle));
    }

    #[test]
    fn single_suppressed_after_different_id_press() {
        let mut gesture = Gesture::new(Single, OnRelease);
        for key in [Press(1), Press(2), Release(2), Release(1)] {
            assert_eq!(gesture.input(key, 0), None);
        }
    }

    #[test]
    fn foreign_press_when_idle_does_not_chord_next_tap() {
        let mut gesture = Gesture::new(Single, OnRelease);
        assert_eq!(gesture.input(ForeignPress, 0), None);
        assert_eq!(gesture.tap(10), Some(Toggle));
    }

    #[test]
    fn auto_repeat_does_not_disturb_pending_tap() {
        for edge in [OnPress, OnRelease] {
            let mut gesture = Gesture::new(Double, edge);
            assert_eq!(gesture.tap(0), None);
            let press = gesture.input(Press(1), 200);
            assert_eq!(gesture.input(Press(1), 210), None);
            let release = gesture.input(Release(1), 220);
            match edge {
                OnPress => {
                    assert_eq!(press, Some(Toggle));
                    assert_eq!(release, None);
                }
                OnRelease => {
                    assert_eq!(press, None);
                    assert_eq!(release, Some(Toggle));
                }
            }
        }
    }

    #[test]
    fn double_fires_within_interval_and_at_boundary() {
        for edge in [OnPress, OnRelease] {
            for gap in [200, 300] {
                let mut gesture = Gesture::new(Double, edge);
                assert_eq!(gesture.tap(0), None);
                assert_eq!(gesture.tap(gap), Some(Toggle));
            }
        }
    }

    #[test]
    fn double_exceeding_interval_starts_a_new_pair() {
        for edge in [OnPress, OnRelease] {
            let mut gesture = Gesture::new(Double, edge);
            assert_eq!(gesture.tap(0), None);
            assert_eq!(gesture.tap(301), None);
            assert_eq!(gesture.tap(400), Some(Toggle));
        }
    }

    #[test]
    fn third_tap_after_successful_double_does_not_fire() {
        for edge in [OnPress, OnRelease] {
            let mut gesture = Gesture::new(Double, edge);
            assert_eq!(gesture.tap(0), None);
            assert_eq!(gesture.tap(100), Some(Toggle));
            assert_eq!(gesture.tap(200), None);
        }
    }

    #[test]
    fn chorded_release_preserves_previous_tap() {
        for chord in [ForeignPress, Press(2)] {
            let mut gesture = Gesture::new(Double, OnRelease);
            assert_eq!(gesture.tap(0), None);
            for key in [Press(1), chord, Release(1)] {
                assert_eq!(gesture.input(key, 100), None);
            }
            assert_eq!(gesture.tap(200), Some(Toggle));
        }
    }

    #[test]
    fn hold_starts_on_press_and_stops_on_release_for_both_edges() {
        for edge in [OnPress, OnRelease] {
            let mut gesture = Gesture::new(Hold, edge);
            assert_eq!(gesture.input(Press(1), 0), Some(Start));
            assert_eq!(gesture.input(Release(1), 100), Some(Stop));
        }
    }

    #[test]
    fn hold_still_stops_after_a_chord() {
        for chord in [ForeignPress, Press(2)] {
            let mut gesture = Gesture::new(Hold, OnRelease);
            assert_eq!(gesture.input(Press(1), 0), Some(Start));
            assert_eq!(gesture.input(chord, 10), None);
            assert_eq!(gesture.input(Release(1), 100), Some(Stop));
        }
    }

    #[test]
    fn hold_auto_repeat_does_not_start_twice() {
        let mut gesture = Gesture::new(Hold, OnRelease);
        assert_eq!(gesture.input(Press(1), 0), Some(Start));
        assert_eq!(gesture.input(Press(1), 10), None);
        assert_eq!(gesture.input(Release(1), 100), Some(Stop));
    }

    #[test]
    fn reset_during_active_hold_stops_and_clears_state() {
        let mut gesture = Gesture::new(Hold, OnRelease);
        assert_eq!(gesture.input(Press(1), 0), Some(Start));
        assert_eq!(gesture.input(ForeignPress, 10), None);
        assert_eq!(gesture.detector.reset(), Some(Stop));
        assert_eq!(gesture.input(Release(1), 20), None);
        assert_eq!(gesture.input(Press(1), 30), Some(Start));
        assert_eq!(gesture.input(Release(1), 40), Some(Stop));
        assert_eq!(gesture.detector.reset(), None);
    }

    #[test]
    fn reset_when_idle_returns_none() {
        assert_eq!(TriggerDetector::new().reset(), None);
    }

    #[test]
    fn reset_clears_pending_double_tap() {
        let mut gesture = Gesture::new(Double, OnRelease);
        assert_eq!(gesture.tap(0), None);
        assert_eq!(gesture.detector.reset(), None);
        assert_eq!(gesture.tap(100), None);
    }

    #[test]
    fn foreign_release_does_not_clear_our_press_or_hold() {
        for style in [Single, Hold] {
            let mut gesture = Gesture::new(style, OnRelease);
            let start = gesture.input(Press(1), 0);
            assert_eq!(start, if style == Hold { Some(Start) } else { None });
            assert_eq!(gesture.input(Release(2), 10), None);
            let release = gesture.input(Release(1), 20);
            assert_eq!(
                release,
                if style == Hold {
                    Some(Stop)
                } else {
                    Some(Toggle)
                }
            );
        }
    }
}
