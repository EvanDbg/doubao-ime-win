//! Text Inserter using Windows SendInput API
//!
//! Inserts text into the currently focused window using keyboard simulation.

use anyhow::Result;
use std::mem::size_of;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    VIRTUAL_KEY, VK_BACK,
};

/// Text inserter service using Windows SendInput API
pub struct TextInserter;

impl TextInserter {
    /// Create a new text inserter
    pub fn new() -> Self {
        Self
    }

    /// Insert text into the currently focused window
    pub fn insert(&self, text: &str) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        let mut inputs: Vec<INPUT> = Vec::new();

        for ch in text.encode_utf16() {
            // Key down
            inputs.push(self.create_unicode_input(ch, true));
            // Key up
            inputs.push(self.create_unicode_input(ch, false));
        }

        self.send_inputs(&inputs)?;
        Ok(())
    }

    /// Delete specified number of characters (simulate backspace)
    pub fn delete_chars(&self, count: usize) -> Result<()> {
        if count == 0 {
            return Ok(());
        }

        let mut inputs: Vec<INPUT> = Vec::new();

        for _ in 0..count {
            // Backspace key down
            inputs.push(self.create_key_input(VK_BACK, true));
            // Backspace key up
            inputs.push(self.create_key_input(VK_BACK, false));
        }

        self.send_inputs(&inputs)?;
        Ok(())
    }

    /// Create a Unicode character input
    fn create_unicode_input(&self, ch: u16, key_down: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: ch,
                    dwFlags: if key_down {
                        KEYEVENTF_UNICODE
                    } else {
                        KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    /// Create a virtual key input
    fn create_key_input(&self, vk: VIRTUAL_KEY, key_down: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if key_down {
                        windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0)
                    } else {
                        KEYEVENTF_KEYUP
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    /// Send inputs using Windows SendInput API
    fn send_inputs(&self, inputs: &[INPUT]) -> Result<()> {
        if inputs.is_empty() {
            return Ok(());
        }

        let sent = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };

        if sent != inputs.len() as u32 {
            tracing::warn!(
                "SendInput sent {} of {} inputs",
                sent,
                inputs.len()
            );
        }

        Ok(())
    }
}

impl Default for TextInserter {
    fn default() -> Self {
        Self::new()
    }
}
