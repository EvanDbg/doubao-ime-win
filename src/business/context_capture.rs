//! Best-effort capture of the foreground edit context.

// The reference TSF implementation reads at most 0x80 UTF-16 characters on
// either side of the caret. UI Automation is the cross-process equivalent
// available to this standalone executable.
const CONTEXT_CHAR_LIMIT: usize = 0x80;

#[cfg(target_os = "windows")]
const CONTEXT_CAPTURE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(250);

/// A window which was in the foreground when a voice session started.
///
/// The raw handle is kept platform-neutral so session records can safely move
/// between worker threads without carrying a Windows pointer type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TargetWindow {
    raw_handle: isize,
}

impl TargetWindow {
    pub fn raw_handle(self) -> isize {
        self.raw_handle
    }
}

/// Text surrounding the target application's current caret or selection.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContextSnapshot {
    pub target: Option<TargetWindow>,
    pub preceding_part: String,
    pub follows_below: String,
}

/// Capture the foreground target and up to 128 characters on either side of
/// its caret. Unsupported controls and UI Automation errors return empty text.
pub fn capture_context() -> ContextSnapshot {
    platform::capture_context()
}

fn last_chars(value: &str, limit: usize) -> String {
    let char_count = value.chars().count();
    value
        .chars()
        .skip(char_count.saturating_sub(limit))
        .collect()
}

fn first_chars(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

pub(crate) fn is_expected_foreground(
    target: TargetWindow,
    foreground: Option<TargetWindow>,
) -> bool {
    foreground == Some(target)
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{
        first_chars, last_chars, ContextSnapshot, TargetWindow, CONTEXT_CAPTURE_TIMEOUT,
        CONTEXT_CHAR_LIMIT,
    };
    use std::{ffi::c_void, sync::mpsc};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationTextPattern, IUIAutomationTextRange,
        TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start, TextUnit_Character,
        UIA_TextPatternId,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    impl TargetWindow {
        pub fn capture_foreground() -> Option<Self> {
            let hwnd = unsafe { GetForegroundWindow() };
            (!hwnd.0.is_null()).then_some(Self {
                raw_handle: hwnd.0 as isize,
            })
        }

        pub(crate) fn hwnd(self) -> HWND {
            HWND(self.raw_handle as *mut c_void)
        }
    }

    pub(super) fn capture_context() -> ContextSnapshot {
        let target = TargetWindow::capture_foreground();
        let Some(target_window) = target else {
            return ContextSnapshot::default();
        };

        let mut snapshot = ContextSnapshot {
            target,
            ..ContextSnapshot::default()
        };

        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = unsafe { capture_text(target_window.hwnd()) };
            let _ = sender.send(result);
        });

        match receiver.recv_timeout(CONTEXT_CAPTURE_TIMEOUT) {
            Ok(Ok((preceding_part, follows_below))) => {
                snapshot.preceding_part = preceding_part;
                snapshot.follows_below = follows_below;
            }
            Ok(Err(error)) => {
                tracing::debug!("Unable to capture target text context: {}", error);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                tracing::debug!("Timed out capturing target text context");
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                tracing::debug!("Text context capture worker stopped unexpectedly");
            }
        }
        snapshot
    }

    unsafe fn capture_text(hwnd: HWND) -> windows::core::Result<(String, String)> {
        struct ComGuard(bool);
        impl Drop for ComGuard {
            fn drop(&mut self) {
                if self.0 {
                    unsafe { CoUninitialize() };
                }
            }
        }

        let init_result = CoInitializeEx(None, COINIT_MULTITHREADED);
        let _com_guard = ComGuard(init_result.is_ok());
        if GetForegroundWindow() != hwnd {
            return Ok((String::new(), String::new()));
        }
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        // TextPattern normally belongs to the focused edit control, not the
        // application's top-level HWND captured for later focus restoration.
        let element = automation.GetFocusedElement()?;
        let pattern: IUIAutomationTextPattern = element.GetCurrentPatternAs(UIA_TextPatternId)?;
        let selections = pattern.GetSelection()?;
        if selections.Length()? == 0 {
            return Ok((String::new(), String::new()));
        }
        let selection = selections.GetElement(0)?;

        // Match ITfRange::Collapse(TF_ANCHOR_END): a non-empty selection is
        // treated as a caret at its end for both surrounding ranges.
        let preceding_range: IUIAutomationTextRange = selection.Clone()?;
        preceding_range.MoveEndpointByRange(
            TextPatternRangeEndpoint_Start,
            &selection,
            TextPatternRangeEndpoint_End,
        )?;
        preceding_range.MoveEndpointByUnit(
            TextPatternRangeEndpoint_Start,
            TextUnit_Character,
            -(CONTEXT_CHAR_LIMIT as i32),
        )?;
        let preceding = preceding_range
            .GetText(CONTEXT_CHAR_LIMIT as i32)?
            .to_string();

        let following_range: IUIAutomationTextRange = selection.Clone()?;
        following_range.MoveEndpointByRange(
            TextPatternRangeEndpoint_Start,
            &selection,
            TextPatternRangeEndpoint_End,
        )?;
        following_range.MoveEndpointByUnit(
            TextPatternRangeEndpoint_End,
            TextUnit_Character,
            CONTEXT_CHAR_LIMIT as i32,
        )?;
        let following = following_range
            .GetText(CONTEXT_CHAR_LIMIT as i32)?
            .to_string();

        let captured = (
            last_chars(&preceding, CONTEXT_CHAR_LIMIT),
            first_chars(&following, CONTEXT_CHAR_LIMIT),
        );
        if GetForegroundWindow() == hwnd {
            Ok(captured)
        } else {
            Ok((String::new(), String::new()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{first_chars, last_chars, CONTEXT_CHAR_LIMIT};

    #[test]
    fn context_limit_matches_reference_implementation() {
        assert_eq!(CONTEXT_CHAR_LIMIT, 0x80);
    }

    #[test]
    fn context_truncation_preserves_unicode_character_boundaries() {
        let value = format!("{}{}{}", "a".repeat(128), "\u{1f600}", "b".repeat(128));

        assert_eq!(
            first_chars(&value, 129),
            format!("{}\u{1f600}", "a".repeat(128))
        );
        assert_eq!(
            last_chars(&value, 129),
            format!("\u{1f600}{}", "b".repeat(128))
        );
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::{ContextSnapshot, TargetWindow};

    impl TargetWindow {
        pub fn capture_foreground() -> Option<Self> {
            None
        }
    }

    pub(super) fn capture_context() -> ContextSnapshot {
        ContextSnapshot::default()
    }
}
