//! Hotkey Manager
//!
//! Manages global hotkeys for triggering voice input.
//! Supports combo keys and single-tap, double-tap or hold gestures.

use anyhow::{anyhow, Result};
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::{FireEdge, KeyInput, TriggerDetector, TriggerEvent, TriggerStyle};
use crate::data::HotkeyConfig;

#[derive(Default)]
struct HotkeyHandlers {
    toggle: Option<Arc<dyn Fn() + Send + Sync>>,
    start: Option<Arc<dyn Fn() + Send + Sync>>,
    stop: Option<Arc<dyn Fn() + Send + Sync>>,
}

fn dispatch_event(handlers: &Arc<Mutex<HotkeyHandlers>>, event: TriggerEvent) {
    let callback = {
        let handlers = handlers.lock().unwrap();
        match event {
            TriggerEvent::Toggle => handlers.toggle.clone(),
            TriggerEvent::Start => handlers.start.clone(),
            TriggerEvent::Stop => handlers.stop.clone(),
        }
    };
    if let Some(callback) = callback {
        callback();
    }
}

/// Hotkey mode
#[derive(Debug, Clone, PartialEq)]
pub enum HotkeyMode {
    /// Combination key mode (e.g., Ctrl+Shift+V)
    Combo,
    /// Tap-key mode with a configurable trigger gesture
    DoubleTap,
}

/// Hotkey manager for global hotkey handling
pub struct HotkeyManager {
    _manager: Option<GlobalHotKeyManager>,
    mode: HotkeyMode,
    double_tap_interval: Duration,
    double_tap_key: String,
    trigger_style: TriggerStyle,
    handlers: Arc<Mutex<HotkeyHandlers>>,
    is_active: Arc<AtomicBool>,
}

impl HotkeyManager {
    /// Create a new hotkey manager based on configuration
    pub fn new(config: &HotkeyConfig) -> Result<Self> {
        let mode = if config.mode == "combo" {
            HotkeyMode::Combo
        } else {
            HotkeyMode::DoubleTap
        };

        let trigger_style = match config.trigger.to_lowercase().as_str() {
            "single" => TriggerStyle::Single,
            "hold" => TriggerStyle::Hold,
            "double" => TriggerStyle::Double,
            _ => {
                tracing::warn!(
                    "Unknown hotkey trigger '{}'; falling back to double-tap",
                    config.trigger
                );
                TriggerStyle::Double
            }
        };

        let manager = GlobalHotKeyManager::new()
            .map_err(|e| anyhow!("Failed to create hotkey manager: {}", e))?;

        // Register hotkey based on mode
        match mode {
            HotkeyMode::Combo => {
                // Parse combo key (default: Ctrl+Shift+V)
                let hotkey = parse_combo_key(&config.combo_key)?;
                manager
                    .register(hotkey)
                    .map_err(|e| anyhow!("Failed to register hotkey: {}", e))?;
                tracing::info!("Registered combo hotkey: {}", config.combo_key);
            }
            HotkeyMode::DoubleTap => {
                // For modifier keys like Ctrl, we use low-level keyboard hook
                // For regular keys, we can use global_hotkey
                let key_lower = config.double_tap_key.to_lowercase();
                if key_lower == "ctrl" || key_lower == "shift" || key_lower == "alt" {
                    // Will use Windows keyboard hook for modifier keys
                    tracing::info!(
                        "Trigger modifier key: {} (using keyboard hook)",
                        config.double_tap_key
                    );
                } else {
                    // Regular key - can use global_hotkey
                    let hotkey = HotKey::new(None, parse_key_code(&config.double_tap_key)?);
                    manager
                        .register(hotkey)
                        .map_err(|e| anyhow!("Failed to register hotkey: {}", e))?;
                    tracing::info!("Registered trigger hotkey: {}", config.double_tap_key);
                }
            }
        }

        let hotkey_manager = Self {
            _manager: Some(manager),
            mode,
            double_tap_interval: Duration::from_millis(config.double_tap_interval.clamp(100, 1000)),
            double_tap_key: config.double_tap_key.clone(),
            trigger_style,
            handlers: Arc::new(Mutex::new(HotkeyHandlers::default())),
            is_active: Arc::new(AtomicBool::new(true)),
        };
        hotkey_manager.start_listener();
        Ok(hotkey_manager)
    }

    /// Set callback for when hotkey is triggered
    pub fn on_trigger<F>(&self, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.handlers.lock().unwrap().toggle = Some(Arc::new(callback));
    }

    /// Set callbacks for press-and-hold recording
    pub fn on_hold<F1, F2>(&self, on_start: F1, on_stop: F2)
    where
        F1: Fn() + Send + Sync + 'static,
        F2: Fn() + Send + Sync + 'static,
    {
        let mut handlers = self.handlers.lock().unwrap();
        handlers.start = Some(Arc::new(on_start));
        handlers.stop = Some(Arc::new(on_stop));
    }

    fn start_listener(&self) {
        let mode = self.mode.clone();
        let double_tap_interval = self.double_tap_interval;
        let double_tap_key = self.double_tap_key.clone();
        let is_active = self.is_active.clone();
        let trigger_style = self.trigger_style;
        let handlers = self.handlers.clone();

        // Check if we need to use keyboard hook for modifier keys
        let key_lower = double_tap_key.to_lowercase();
        let use_keyboard_hook = mode == HotkeyMode::DoubleTap
            && (key_lower == "ctrl" || key_lower == "shift" || key_lower == "alt");

        if use_keyboard_hook {
            // Use Windows keyboard hook for modifier key gestures
            #[cfg(target_os = "windows")]
            {
                thread::spawn(move || {
                    run_modifier_trigger_hook(
                        key_lower,
                        trigger_style,
                        double_tap_interval,
                        is_active,
                        handlers,
                    );
                });
            }
            #[cfg(not(target_os = "windows"))]
            {
                tracing::warn!("Modifier key triggers not supported on this platform");
            }
        } else {
            // Use global_hotkey receiver
            thread::spawn(move || {
                let receiver = GlobalHotKeyEvent::receiver();
                let mut detector = TriggerDetector::new();
                let trigger_style = if mode == HotkeyMode::DoubleTap
                    && trigger_style == TriggerStyle::Hold
                {
                    tracing::warn!("hold trigger needs a modifier key; falling back to double-tap");
                    TriggerStyle::Double
                } else {
                    trigger_style
                };

                loop {
                    if !is_active.load(Ordering::SeqCst) {
                        thread::sleep(Duration::from_millis(100));
                        continue;
                    }

                    if let Ok(event) = receiver.recv() {
                        match mode {
                            HotkeyMode::Combo => {
                                dispatch_event(&handlers, TriggerEvent::Toggle);
                            }
                            HotkeyMode::DoubleTap => {
                                if event.state != HotKeyState::Pressed {
                                    continue;
                                }
                                let now = Instant::now();
                                let event = detector.handle(
                                    KeyInput::Press(0),
                                    trigger_style,
                                    double_tap_interval,
                                    FireEdge::Press,
                                    now,
                                );
                                // This adapter treats each Pressed event as a tap. Close
                                // its press without counting the channel's Released event.
                                let _ = detector.handle(
                                    KeyInput::Release(0),
                                    trigger_style,
                                    double_tap_interval,
                                    FireEdge::Press,
                                    now,
                                );
                                if let Some(event) = event {
                                    dispatch_event(&handlers, event);
                                }
                            }
                        }
                    }
                }
            });
        }
    }

    /// Stop the hotkey manager
    pub fn stop(&self) {
        self.is_active.store(false, Ordering::SeqCst);
    }
}

/// Windows keyboard hook for modifier key trigger detection
#[cfg(target_os = "windows")]
fn run_modifier_trigger_hook(
    key: String,
    style: TriggerStyle,
    interval: Duration,
    is_active: Arc<AtomicBool>,
    handlers: Arc<Mutex<HotkeyHandlers>>,
) {
    use std::cell::RefCell;
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_RCONTROL, VK_RMENU, VK_RSHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
        HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN,
        WM_SYSKEYUP,
    };

    // Determine which virtual keys to watch
    let target_vks: Vec<u16> = match key.as_str() {
        "ctrl" => vec![VK_CONTROL.0, VK_LCONTROL.0, VK_RCONTROL.0],
        "shift" => vec![VK_LSHIFT.0, VK_RSHIFT.0],
        "alt" => vec![VK_LMENU.0, VK_RMENU.0],
        _ => vec![],
    };

    if target_vks.is_empty() {
        tracing::error!("Unknown modifier key: {}", key);
        return;
    }

    tracing::info!("Starting keyboard hook for {:?} {} detection", style, key);

    // Thread-local state for hook callback
    thread_local! {
        static HOOK_STATE: RefCell<Option<HookState>> = RefCell::new(None);
    }

    struct HookState {
        target_vks: Vec<u16>,
        interval: Duration,
        style: TriggerStyle,
        detector: TriggerDetector,
        handlers: Arc<Mutex<HotkeyHandlers>>,
        is_active: Arc<AtomicBool>,
    }

    // Initialize thread-local state
    HOOK_STATE.with(|state| {
        *state.borrow_mut() = Some(HookState {
            target_vks,
            interval,
            style,
            detector: TriggerDetector::new(),
            handlers,
            is_active,
        });
    });

    // Low-level keyboard hook procedure
    unsafe extern "system" fn keyboard_hook_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code >= 0 {
            let kb_struct = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            let vk_code = kb_struct.vkCode as u16;
            let is_key_down = wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN;
            let is_key_up = wparam.0 as u32 == WM_KEYUP || wparam.0 as u32 == WM_SYSKEYUP;

            HOOK_STATE.with(|state| {
                if let Some(ref mut hook_state) = *state.borrow_mut() {
                    if hook_state.is_active.load(Ordering::SeqCst) {
                        let input = if hook_state.target_vks.contains(&vk_code) {
                            if is_key_down {
                                Some(KeyInput::Press(kb_struct.vkCode as u32))
                            } else if is_key_up {
                                Some(KeyInput::Release(kb_struct.vkCode as u32))
                            } else {
                                None
                            }
                        } else if is_key_down {
                            Some(KeyInput::ForeignPress)
                        } else {
                            None
                        };
                        if let Some(input) = input {
                            if let Some(event) = hook_state.detector.handle(
                                input,
                                hook_state.style,
                                hook_state.interval,
                                FireEdge::Release,
                                Instant::now(),
                            ) {
                                dispatch_event(&hook_state.handlers, event);
                            }
                        }
                    }
                }
            });
        }

        CallNextHookEx(HHOOK::default(), code, wparam, lparam)
    }

    // Install the hook
    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0) };

    match hook {
        Ok(h) => {
            tracing::info!("Keyboard hook installed successfully");

            // Message loop to keep hook alive
            let mut msg = MSG::default();
            unsafe {
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    DispatchMessageW(&msg);
                }
            }

            // Cleanup
            let _ = unsafe { UnhookWindowsHookEx(h) };
            tracing::info!("Keyboard hook uninstalled");
        }
        Err(e) => {
            tracing::error!("Failed to install keyboard hook: {:?}", e);
        }
    }
}

/// Parse a combo key string like "Ctrl+Shift+V"
fn parse_combo_key(key_str: &str) -> Result<HotKey> {
    let parts: Vec<&str> = key_str.split('+').map(|s| s.trim()).collect();

    let mut modifiers = Modifiers::empty();
    let mut key_code: Option<Code> = None;

    for part in parts {
        match part.to_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "shift" => modifiers |= Modifiers::SHIFT,
            "alt" => modifiers |= Modifiers::ALT,
            "super" | "win" | "meta" => modifiers |= Modifiers::SUPER,
            _ => {
                key_code = Some(parse_key_code(part)?);
            }
        }
    }

    let code = key_code.ok_or_else(|| anyhow!("No key specified in combo: {}", key_str))?;

    Ok(HotKey::new(Some(modifiers), code))
}

/// Parse a key code from string
fn parse_key_code(key: &str) -> Result<Code> {
    let code = match key.to_uppercase().as_str() {
        "A" => Code::KeyA,
        "B" => Code::KeyB,
        "C" => Code::KeyC,
        "D" => Code::KeyD,
        "E" => Code::KeyE,
        "F" => Code::KeyF,
        "G" => Code::KeyG,
        "H" => Code::KeyH,
        "I" => Code::KeyI,
        "J" => Code::KeyJ,
        "K" => Code::KeyK,
        "L" => Code::KeyL,
        "M" => Code::KeyM,
        "N" => Code::KeyN,
        "O" => Code::KeyO,
        "P" => Code::KeyP,
        "Q" => Code::KeyQ,
        "R" => Code::KeyR,
        "S" => Code::KeyS,
        "T" => Code::KeyT,
        "U" => Code::KeyU,
        "V" => Code::KeyV,
        "W" => Code::KeyW,
        "X" => Code::KeyX,
        "Y" => Code::KeyY,
        "Z" => Code::KeyZ,
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        "SPACE" => Code::Space,
        "ENTER" | "RETURN" => Code::Enter,
        "ESCAPE" | "ESC" => Code::Escape,
        "F1" => Code::F1,
        "F2" => Code::F2,
        "F3" => Code::F3,
        "F4" => Code::F4,
        "F5" => Code::F5,
        "F6" => Code::F6,
        "F7" => Code::F7,
        "F8" => Code::F8,
        "F9" => Code::F9,
        "F10" => Code::F10,
        "F11" => Code::F11,
        "F12" => Code::F12,
        _ => return Err(anyhow!("Unknown key: {}", key)),
    };

    Ok(code)
}
