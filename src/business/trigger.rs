//! Unified trigger detection for hotkey events.
//!
//! Both listening paths — the `global-hotkey` channel and the Windows
//! low-level hooks — feed key transitions into a [`TriggerDetector`], which
//! owns the single implementation of single-tap, double-tap, hold and chord
//! suppression semantics. Keeping the state here (instead of in listener
//! locals or hook thread-locals) lets `reconfigure` reset it and recover a
//! hanging hold.

use std::time::{Duration, Instant};

use super::hotkey_manager::{HotkeyEvent, RawKeyBinding};
use crate::data::TriggerMode;

/// Identity of the key a transition belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerKey {
    /// A `global-hotkey` registration id.
    Standard(u32),
    /// A physical Windows key or mouse side button seen by the hooks.
    Raw(RawKeyBinding),
}

/// A key transition reported by a listening path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerInput {
    Press(TriggerKey),
    Release(TriggerKey),
    /// Some other key was pressed; while our key is held this marks the
    /// press as a chord so a pure modifier binding does not fire.
    ForeignPress,
}

/// Which transition edge fires single/double taps. Registered hotkeys and
/// raw keys fire on press; pure modifier keys fire on release so chords
/// (e.g. Ctrl+C while bound to Ctrl) can be suppressed first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FireEdge {
    Press,
    Release,
}

/// Stateful tap/hold detector shared by one listening path.
#[derive(Debug, Default)]
pub struct TriggerDetector {
    pressed: Option<TriggerKey>,
    chorded: bool,
    hold_active: bool,
    last_tap: Option<(TriggerKey, Instant)>,
}

impl TriggerDetector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one transition and return the event to emit, if any.
    pub fn handle(
        &mut self,
        input: TriggerInput,
        mode: TriggerMode,
        interval: Duration,
        edge: FireEdge,
        now: Instant,
    ) -> Option<HotkeyEvent> {
        match input {
            TriggerInput::Press(key) => match self.pressed {
                // Keyboard auto-repeat of the held key.
                Some(current) if current == key => None,
                // A second matching key (e.g. the other Ctrl) is a chord.
                Some(_) => {
                    self.chorded = true;
                    None
                }
                None => {
                    self.pressed = Some(key);
                    self.chorded = false;
                    if mode == TriggerMode::Hold {
                        self.hold_active = true;
                        Some(HotkeyEvent::Start)
                    } else if edge == FireEdge::Press {
                        self.tap(mode, key, interval, now)
                    } else {
                        None
                    }
                }
            },
            TriggerInput::ForeignPress => {
                if self.pressed.is_some() {
                    self.chorded = true;
                }
                None
            }
            TriggerInput::Release(key) => {
                // A release with a different identity (other device, other
                // key) must not clear our held state.
                if self.pressed != Some(key) {
                    return None;
                }
                self.pressed = None;
                let chorded = std::mem::replace(&mut self.chorded, false);
                if self.hold_active {
                    self.hold_active = false;
                    return Some(HotkeyEvent::Stop);
                }
                if mode != TriggerMode::Hold && edge == FireEdge::Release && !chorded {
                    return self.tap(mode, key, interval, now);
                }
                None
            }
        }
    }

    /// Clear all state, e.g. after a reconfiguration. Returns the Stop event
    /// the caller must deliver when a hold was in progress, so saving the
    /// settings while the key is held cannot leave a recording running.
    pub fn reset(&mut self) -> Option<HotkeyEvent> {
        let hold_active = self.hold_active;
        *self = Self::default();
        hold_active.then_some(HotkeyEvent::Stop)
    }

    fn tap(
        &mut self,
        mode: TriggerMode,
        key: TriggerKey,
        interval: Duration,
        now: Instant,
    ) -> Option<HotkeyEvent> {
        match mode {
            TriggerMode::SingleTap => Some(HotkeyEvent::Toggle),
            TriggerMode::DoubleTap => {
                if self.last_tap.is_some_and(|(last_key, last_at)| {
                    last_key == key && now.duration_since(last_at) <= interval
                }) {
                    self.last_tap = None;
                    Some(HotkeyEvent::Toggle)
                } else {
                    self.last_tap = Some((key, now));
                    None
                }
            }
            TriggerMode::Hold => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTERVAL: Duration = Duration::from_millis(300);

    fn key(id: u32) -> TriggerKey {
        TriggerKey::Standard(id)
    }

    fn raw(vk_code: u32) -> TriggerKey {
        TriggerKey::Raw(RawKeyBinding {
            vk_code,
            scan_code: 0x1E,
            extended: false,
        })
    }

    fn press(
        detector: &mut TriggerDetector,
        key: TriggerKey,
        mode: TriggerMode,
        edge: FireEdge,
        now: Instant,
    ) -> Option<HotkeyEvent> {
        detector.handle(TriggerInput::Press(key), mode, INTERVAL, edge, now)
    }

    fn release(
        detector: &mut TriggerDetector,
        key: TriggerKey,
        mode: TriggerMode,
        edge: FireEdge,
        now: Instant,
    ) -> Option<HotkeyEvent> {
        detector.handle(TriggerInput::Release(key), mode, INTERVAL, edge, now)
    }

    #[test]
    fn single_tap_fires_on_press_edge() {
        let mut detector = TriggerDetector::new();
        let now = Instant::now();

        let mode = TriggerMode::SingleTap;
        assert_eq!(
            press(&mut detector, key(1), mode, FireEdge::Press, now),
            Some(HotkeyEvent::Toggle)
        );
        assert_eq!(
            release(&mut detector, key(1), mode, FireEdge::Press, now),
            None
        );
    }

    #[test]
    fn single_tap_fires_on_release_edge_for_modifiers() {
        let mut detector = TriggerDetector::new();
        let now = Instant::now();

        let mode = TriggerMode::SingleTap;
        assert_eq!(
            press(&mut detector, raw(0x11), mode, FireEdge::Release, now),
            None
        );
        assert_eq!(
            release(&mut detector, raw(0x11), mode, FireEdge::Release, now),
            Some(HotkeyEvent::Toggle)
        );
    }

    #[test]
    fn auto_repeat_press_is_ignored() {
        let mut detector = TriggerDetector::new();
        let now = Instant::now();

        let mode = TriggerMode::SingleTap;
        assert_eq!(
            press(&mut detector, key(1), mode, FireEdge::Press, now),
            Some(HotkeyEvent::Toggle)
        );
        assert_eq!(
            press(&mut detector, key(1), mode, FireEdge::Press, now),
            None
        );
        assert_eq!(
            press(&mut detector, key(1), mode, FireEdge::Press, now),
            None
        );
    }

    #[test]
    fn double_tap_requires_same_key_within_interval() {
        let mut detector = TriggerDetector::new();
        let t0 = Instant::now();

        let mode = TriggerMode::DoubleTap;
        assert_eq!(
            press(&mut detector, key(1), mode, FireEdge::Press, t0),
            None
        );
        assert_eq!(
            release(&mut detector, key(1), mode, FireEdge::Press, t0),
            None
        );
        assert_eq!(
            press(
                &mut detector,
                key(1),
                mode,
                FireEdge::Press,
                t0 + Duration::from_millis(200)
            ),
            Some(HotkeyEvent::Toggle)
        );
    }

    #[test]
    fn double_tap_outside_interval_restarts_the_window() {
        let mut detector = TriggerDetector::new();
        let t0 = Instant::now();

        let mode = TriggerMode::DoubleTap;
        assert_eq!(
            press(&mut detector, key(1), mode, FireEdge::Press, t0),
            None
        );
        assert_eq!(
            release(&mut detector, key(1), mode, FireEdge::Press, t0),
            None
        );
        let late = t0 + Duration::from_millis(400);
        assert_eq!(
            press(&mut detector, key(1), mode, FireEdge::Press, late),
            None
        );
        assert_eq!(
            release(&mut detector, key(1), mode, FireEdge::Press, late),
            None
        );
        assert_eq!(
            press(
                &mut detector,
                key(1),
                mode,
                FireEdge::Press,
                late + Duration::from_millis(100)
            ),
            Some(HotkeyEvent::Toggle)
        );
    }

    #[test]
    fn double_tap_across_different_keys_does_not_fire() {
        let mut detector = TriggerDetector::new();
        let t0 = Instant::now();

        let mode = TriggerMode::DoubleTap;
        assert_eq!(
            press(&mut detector, raw(0x05), mode, FireEdge::Press, t0),
            None
        );
        assert_eq!(
            release(&mut detector, raw(0x05), mode, FireEdge::Press, t0),
            None
        );
        // A different key inside the window must not complete the double tap.
        assert_eq!(
            press(
                &mut detector,
                raw(0x06),
                mode,
                FireEdge::Press,
                t0 + Duration::from_millis(100)
            ),
            None
        );
    }

    #[test]
    fn hold_emits_start_and_stop() {
        let mut detector = TriggerDetector::new();
        let now = Instant::now();

        let mode = TriggerMode::Hold;
        assert_eq!(
            press(&mut detector, key(1), mode, FireEdge::Press, now),
            Some(HotkeyEvent::Start)
        );
        assert_eq!(
            release(
                &mut detector,
                key(1),
                mode,
                FireEdge::Press,
                now + Duration::from_secs(1)
            ),
            Some(HotkeyEvent::Stop)
        );
    }

    #[test]
    fn chord_suppresses_release_edge_tap() {
        let mut detector = TriggerDetector::new();
        let now = Instant::now();

        // Bound to a pure modifier: Ctrl+C must not trigger voice input.
        let mode = TriggerMode::SingleTap;
        assert_eq!(
            press(&mut detector, raw(0x11), mode, FireEdge::Release, now),
            None
        );
        assert_eq!(
            detector.handle(
                TriggerInput::ForeignPress,
                mode,
                INTERVAL,
                FireEdge::Release,
                now
            ),
            None
        );
        assert_eq!(
            release(&mut detector, raw(0x11), mode, FireEdge::Release, now),
            None
        );
        // The next clean tap fires again.
        assert_eq!(
            press(&mut detector, raw(0x11), mode, FireEdge::Release, now),
            None
        );
        assert_eq!(
            release(&mut detector, raw(0x11), mode, FireEdge::Release, now),
            Some(HotkeyEvent::Toggle)
        );
    }

    #[test]
    fn chord_does_not_suppress_hold_stop() {
        let mut detector = TriggerDetector::new();
        let now = Instant::now();

        let mode = TriggerMode::Hold;
        assert_eq!(
            press(&mut detector, raw(0x11), mode, FireEdge::Release, now),
            Some(HotkeyEvent::Start)
        );
        assert_eq!(
            detector.handle(
                TriggerInput::ForeignPress,
                mode,
                INTERVAL,
                FireEdge::Release,
                now
            ),
            None
        );
        assert_eq!(
            release(&mut detector, raw(0x11), mode, FireEdge::Release, now),
            Some(HotkeyEvent::Stop)
        );
    }

    #[test]
    fn foreign_press_without_held_key_is_ignored() {
        let mut detector = TriggerDetector::new();
        let now = Instant::now();

        let mode = TriggerMode::SingleTap;
        assert_eq!(
            detector.handle(
                TriggerInput::ForeignPress,
                mode,
                INTERVAL,
                FireEdge::Release,
                now
            ),
            None
        );
        // State is untouched: the next tap still fires.
        assert_eq!(
            press(&mut detector, raw(0x11), mode, FireEdge::Release, now),
            None
        );
        assert_eq!(
            release(&mut detector, raw(0x11), mode, FireEdge::Release, now),
            Some(HotkeyEvent::Toggle)
        );
    }

    #[test]
    fn mismatched_release_does_not_clear_held_state() {
        let mut detector = TriggerDetector::new();
        let now = Instant::now();

        let mode = TriggerMode::Hold;
        assert_eq!(
            press(&mut detector, raw(0x05), mode, FireEdge::Press, now),
            Some(HotkeyEvent::Start)
        );
        // A keyboard release with a different identity must not end the hold.
        assert_eq!(
            release(&mut detector, raw(0x06), mode, FireEdge::Press, now),
            None
        );
        assert_eq!(
            release(&mut detector, raw(0x05), mode, FireEdge::Press, now),
            Some(HotkeyEvent::Stop)
        );
    }

    #[test]
    fn second_matching_key_marks_a_chord() {
        let mut detector = TriggerDetector::new();
        let now = Instant::now();

        // Both Ctrl keys map to the binding but have distinct identities.
        let mode = TriggerMode::SingleTap;
        assert_eq!(
            press(&mut detector, raw(0xA2), mode, FireEdge::Release, now),
            None
        );
        assert_eq!(
            press(&mut detector, raw(0xA3), mode, FireEdge::Release, now),
            None
        );
        assert_eq!(
            release(&mut detector, raw(0xA2), mode, FireEdge::Release, now),
            None
        );
    }

    #[test]
    fn reset_returns_stop_for_hanging_hold() {
        let mut detector = TriggerDetector::new();
        let now = Instant::now();

        assert_eq!(
            press(
                &mut detector,
                key(1),
                TriggerMode::Hold,
                FireEdge::Press,
                now
            ),
            Some(HotkeyEvent::Start)
        );
        assert_eq!(detector.reset(), Some(HotkeyEvent::Stop));
        // Fully cleared: a stale release is ignored.
        assert_eq!(
            release(
                &mut detector,
                key(1),
                TriggerMode::Hold,
                FireEdge::Press,
                now
            ),
            None
        );
    }

    #[test]
    fn reset_without_hold_returns_nothing_and_clears_taps() {
        let mut detector = TriggerDetector::new();
        let t0 = Instant::now();

        let mode = TriggerMode::DoubleTap;
        assert_eq!(
            press(&mut detector, key(1), mode, FireEdge::Press, t0),
            None
        );
        assert_eq!(detector.reset(), None);
        // The pending first tap was discarded by the reset.
        assert_eq!(
            press(
                &mut detector,
                key(1),
                mode,
                FireEdge::Press,
                t0 + Duration::from_millis(100)
            ),
            None
        );
    }
}
