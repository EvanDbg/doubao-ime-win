//! Global and raw keyboard shortcut management.
//!
//! Standard shortcuts continue to use `global-hotkey`.  On Windows, raw
//! bindings are observed with low-level keyboard and mouse hooks so vendor
//! keys and mouse side buttons which do not have a `global-hotkey::Code` can
//! still be configured.  Both paths feed a shared [`TriggerDetector`] which
//! owns the tap/hold semantics.

use anyhow::{anyhow, Result};
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "windows")]
use std::sync::mpsc;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use super::trigger::{FireEdge, TriggerDetector, TriggerInput, TriggerKey};
use crate::data::{HotkeyAction, HotkeyBinding, HotkeyConfig, RawKeyConfig};

/// Events emitted by a hotkey listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    /// Toggle recording once.
    Toggle,
    /// Start recording for a press-and-hold binding.
    Start,
    /// Stop recording when a press-and-hold binding is released.
    Stop,
}

/// Identity of a Windows raw keyboard event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawKeyBinding {
    pub vk_code: u32,
    pub scan_code: u32,
    pub extended: bool,
}

/// Key injection requested for one of the official Doubao input modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficialDoubaoAction {
    HoldStart,
    HoldStop,
    HandsFree,
}

/// A pure modifier key configured as the standard trigger.  These cannot be
/// registered through `global-hotkey`, so the keyboard hook matches them by
/// virtual-key code (left and right variants included).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModifierKey {
    Control,
    Shift,
    Alt,
    Win,
}

impl ModifierKey {
    fn from_key_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Some(Self::Control),
            "shift" => Some(Self::Shift),
            "alt" => Some(Self::Alt),
            "win" | "super" | "meta" => Some(Self::Win),
            _ => None,
        }
    }

    #[cfg(target_os = "windows")]
    fn matches_vk(self, vk_code: u32) -> bool {
        match self {
            Self::Control => matches!(vk_code, 0x11 | 0xA2 | 0xA3),
            Self::Shift => matches!(vk_code, 0x10 | 0xA0 | 0xA1),
            Self::Alt => matches!(vk_code, 0x12 | 0xA4 | 0xA5),
            Self::Win => matches!(vk_code, 0x5B | 0x5C),
        }
    }
}

/// Copyable per-event view of the hotkey configuration for the hook thread.
/// The hook callback runs for every keystroke on the system; rebuilding this
/// on reconfigure keeps the callback free of string clones so Windows does
/// not remove the hook for being slow.
#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy)]
struct HookSnapshot {
    action: HotkeyAction,
    mode: crate::data::TriggerMode,
    interval: Duration,
    /// `Some` only when the raw binding is active.
    raw_key: Option<RawKeyConfig>,
    /// `Some` only when the standard binding targets a pure modifier key.
    modifier: Option<ModifierKey>,
    /// Whether the standard binding is active (chord suppression scope).
    standard_binding: bool,
}

#[cfg(target_os = "windows")]
impl HookSnapshot {
    fn from_config(config: &HotkeyConfig) -> Self {
        let standard_binding = config.binding == HotkeyBinding::Standard;
        Self {
            action: config.action,
            mode: config.effective_mode(),
            interval: Duration::from_millis(config.double_tap_interval_ms),
            raw_key: (config.binding == HotkeyBinding::Raw)
                .then_some(config.raw_key)
                .flatten(),
            modifier: standard_binding
                .then(|| ModifierKey::from_key_name(&config.standard_key))
                .flatten(),
            standard_binding,
        }
    }
}

type EventCallback = Arc<dyn Fn(HotkeyEvent) + Send + Sync>;

/// Hotkey manager for global hotkey handling.
#[derive(Clone)]
pub struct HotkeyManager {
    manager: Arc<GlobalHotKeyManager>,
    registered_hotkey: Arc<Mutex<Option<HotKey>>>,
    config: Arc<RwLock<HotkeyConfig>>,
    is_active: Arc<AtomicBool>,
    listener_started: Arc<AtomicBool>,
    event_callback: Arc<Mutex<Option<EventCallback>>>,
    standard_detector: Arc<Mutex<TriggerDetector>>,
    #[cfg(target_os = "windows")]
    raw_detector: Arc<Mutex<TriggerDetector>>,
    #[cfg(target_os = "windows")]
    hook_snapshot: Arc<RwLock<HookSnapshot>>,
    #[cfg(target_os = "windows")]
    capture_in_progress: Arc<AtomicBool>,
}

impl HotkeyManager {
    /// Create a new hotkey manager based on configuration.
    pub fn new(config: &HotkeyConfig) -> Result<Self> {
        validate_config(config)?;

        let manager = Arc::new(
            GlobalHotKeyManager::new()
                .map_err(|e| anyhow!("Failed to create hotkey manager: {}", e))?,
        );
        let registered_hotkey = if config.binding == HotkeyBinding::Raw {
            None
        } else if let Some(hotkey) = configured_standard_hotkey(config)? {
            manager
                .register(hotkey)
                .map_err(|e| anyhow!("Failed to register hotkey: {}", e))?;
            tracing::info!("Registered standard hotkey: {}", config.standard_key);
            Some(hotkey)
        } else {
            tracing::info!("Standard modifier binding will use the Windows keyboard hook");
            None
        };

        if let Some(raw_key) = config
            .raw_key
            .filter(|_| config.binding == HotkeyBinding::Raw)
        {
            tracing::info!(
                "Configured raw key binding: vk=0x{:X}, scan={}, extended={}",
                raw_key.vk_code,
                raw_key
                    .scan_code
                    .map_or_else(|| "any".to_string(), |scan| format!("0x{scan:X}")),
                raw_key.extended
            );
        }

        Ok(Self {
            manager,
            registered_hotkey: Arc::new(Mutex::new(registered_hotkey)),
            config: Arc::new(RwLock::new(config.clone())),
            is_active: Arc::new(AtomicBool::new(true)),
            listener_started: Arc::new(AtomicBool::new(false)),
            event_callback: Arc::new(Mutex::new(None)),
            standard_detector: Arc::new(Mutex::new(TriggerDetector::new())),
            #[cfg(target_os = "windows")]
            raw_detector: Arc::new(Mutex::new(TriggerDetector::new())),
            #[cfg(target_os = "windows")]
            hook_snapshot: Arc::new(RwLock::new(HookSnapshot::from_config(config))),
            #[cfg(target_os = "windows")]
            capture_in_progress: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Reconfigure the active binding without restarting the application.
    pub fn reconfigure(&self, new_config: &HotkeyConfig) -> Result<()> {
        validate_config(new_config)?;

        let new_hotkey = if new_config.binding == HotkeyBinding::Raw {
            None
        } else {
            configured_standard_hotkey(new_config)?
        };

        let mut current = self
            .registered_hotkey
            .lock()
            .map_err(|_| anyhow!("Hotkey registration state is poisoned"))?;

        if *current != new_hotkey {
            if let Some(hotkey) = new_hotkey {
                self.manager
                    .register(hotkey)
                    .map_err(|e| anyhow!("Failed to register new hotkey: {}", e))?;
            }

            if let Some(old_hotkey) = *current {
                // Registration of the new shortcut succeeded, so a failure to
                // unregister the old one is reported rather than silently
                // leaving two active standard bindings.
                if let Err(error) = self.manager.unregister(old_hotkey) {
                    if let Some(hotkey) = new_hotkey {
                        let _ = self.manager.unregister(hotkey);
                    }
                    return Err(anyhow!("Failed to unregister old hotkey: {}", error));
                }
            }

            *current = new_hotkey;
        }
        drop(current);

        *self
            .config
            .write()
            .map_err(|_| anyhow!("Hotkey configuration state is poisoned"))? = new_config.clone();
        #[cfg(target_os = "windows")]
        {
            *self
                .hook_snapshot
                .write()
                .map_err(|_| anyhow!("Hotkey hook state is poisoned"))? =
                HookSnapshot::from_config(new_config);
        }

        // Reset the trigger state so the new configuration starts clean: a
        // hold key still physically pressed must deliver its Stop now instead
        // of leaving the recording running forever.
        let mut pending = Vec::new();
        if let Ok(mut detector) = self.standard_detector.lock() {
            pending.extend(detector.reset());
        }
        #[cfg(target_os = "windows")]
        if let Ok(mut detector) = self.raw_detector.lock() {
            pending.extend(detector.reset());
        }
        if !pending.is_empty() {
            let callback = self
                .event_callback
                .lock()
                .ok()
                .and_then(|slot| slot.clone());
            if let Some(callback) = callback {
                for event in pending {
                    callback(event);
                }
            }
        }

        tracing::info!("Hotkey configuration applied immediately");
        Ok(())
    }

    /// Whether the configured key should invoke the official Doubao input
    /// method instead of this application's recording pipeline.
    pub fn invokes_official_doubao(&self) -> bool {
        self.config
            .read()
            .map(|config| config.action.is_official())
            .unwrap_or(false)
    }

    /// Resolve a physical hotkey event to the corresponding official action.
    pub fn official_doubao_action(&self, event: HotkeyEvent) -> Option<OfficialDoubaoAction> {
        self.config
            .read()
            .ok()
            .and_then(|config| official_doubao_action(&config, event))
    }

    /// Set a callback for hotkey events.
    pub fn on_event<F>(&self, callback: F)
    where
        F: Fn(HotkeyEvent) + Send + Sync + 'static,
    {
        if self.listener_started.swap(true, Ordering::SeqCst) {
            tracing::warn!("Hotkey listener was already started");
            return;
        }

        let callback: EventCallback = Arc::new(callback);
        if let Ok(mut slot) = self.event_callback.lock() {
            *slot = Some(callback.clone());
        }

        // Standard events are delivered through the global-hotkey channel.
        let standard_config = self.config.clone();
        let standard_active = self.is_active.clone();
        let standard_callback = callback.clone();
        let standard_registered = self.registered_hotkey.clone();
        let standard_detector = self.standard_detector.clone();
        thread::spawn(move || {
            let receiver = GlobalHotKeyEvent::receiver();

            loop {
                if !standard_active.load(Ordering::SeqCst) {
                    break;
                }

                let event = match receiver.recv() {
                    Ok(event) => event,
                    Err(_) => break,
                };

                if !standard_active.load(Ordering::SeqCst) {
                    continue;
                }
                let registered = match standard_registered.lock() {
                    Ok(registered) => *registered,
                    Err(_) => continue,
                };
                if registered.map(|hotkey| hotkey.id()) != Some(event.id) {
                    continue;
                }
                let (mode, interval) = match standard_config.read() {
                    Ok(config) => (
                        config.effective_mode(),
                        Duration::from_millis(config.double_tap_interval_ms),
                    ),
                    Err(_) => continue,
                };

                let key = TriggerKey::Standard(event.id);
                let input = match event.state {
                    HotKeyState::Pressed => TriggerInput::Press(key),
                    HotKeyState::Released => TriggerInput::Release(key),
                };
                let emitted = match standard_detector.lock() {
                    Ok(mut detector) => {
                        detector.handle(input, mode, interval, FireEdge::Press, Instant::now())
                    }
                    Err(_) => continue,
                };
                if let Some(event) = emitted {
                    standard_callback(event);
                }
            }
        });

        // Raw hooks are Windows-only.  The hook stays installed while the
        // application is alive and simply ignores events when standard mode
        // is selected, allowing runtime switching without a restart.
        #[cfg(target_os = "windows")]
        {
            let snapshot = self.hook_snapshot.clone();
            let raw_active = self.is_active.clone();
            let raw_detector = self.raw_detector.clone();
            thread::spawn(move || {
                run_raw_key_hook(snapshot, raw_active, callback, raw_detector);
            });
        }

        #[cfg(not(target_os = "windows"))]
        if self
            .config
            .read()
            .map(|config| config.binding == HotkeyBinding::Raw)
            .unwrap_or(false)
        {
            tracing::warn!("Raw keyboard bindings are only supported on Windows");
        }
    }

    /// Stop the hotkey listeners and unregister the standard binding.
    pub fn stop(&self) {
        self.is_active.store(false, Ordering::SeqCst);
        if let Ok(mut current) = self.registered_hotkey.lock() {
            if let Some(hotkey) = current.take() {
                let _ = self.manager.unregister(hotkey);
            }
        }
    }

    /// Capture the next physical Windows key for use as a raw binding.
    ///
    /// The capture installs its own short-lived hooks instead of reusing the
    /// resident binding hook: a dedicated hook is called first in the hook
    /// chain, keeps working even if Windows silently removed the resident
    /// hook for being slow, and may accept injected input which the binding
    /// hook must ignore.
    #[cfg(target_os = "windows")]
    pub fn capture_raw_key(&self, timeout: Duration) -> Result<RawKeyBinding> {
        if self.capture_in_progress.swap(true, Ordering::SeqCst) {
            return Err(anyhow!("A raw-key capture is already in progress"));
        }
        let result = capture_raw_key_blocking(timeout);
        self.capture_in_progress.store(false, Ordering::SeqCst);
        result
    }
}

/// Run one raw-key capture on a dedicated hook thread and wait for it.
#[cfg(target_os = "windows")]
fn capture_raw_key_blocking(timeout: Duration) -> Result<RawKeyBinding> {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};

    let (ready_tx, ready_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    thread::spawn(move || run_capture_hook(ready_tx, result_tx));

    let hook_thread_id = ready_rx
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| anyhow!("The raw-key capture thread did not start"))?
        .map_err(|error| anyhow!("Failed to install the capture keyboard hook: {error}"))?;
    tracing::info!("Raw-key capture armed for {timeout:?}");

    let result = result_rx.recv_timeout(timeout);
    // Ask the hook thread to exit; harmless when it already quit on its own
    // after delivering a capture.
    unsafe {
        let _ = PostThreadMessageW(hook_thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
    }
    // A key delivered right at the deadline still counts.
    match result.or_else(|_| result_rx.try_recv()) {
        Ok(binding) => {
            tracing::info!(
                "Raw-key capture delivered: vk=0x{:X}, scan=0x{:X}, extended={}",
                binding.vk_code,
                binding.scan_code,
                binding.extended
            );
            Ok(binding)
        }
        Err(_) => {
            tracing::warn!("Raw-key capture timed out after {timeout:?}");
            Err(anyhow!(
                "No physical key or mouse side-button was captured before timeout"
            ))
        }
    }
}

/// Dedicated short-lived hooks that record the next key-down or mouse side
/// button.  Unlike the resident binding hook this accepts injected input, so
/// keys synthesized by vendor drivers or remote-desktop software can be
/// recorded too, and it consumes the captured press so it does not leak into
/// the focused window.
#[cfg(target_os = "windows")]
fn run_capture_hook(
    ready_tx: mpsc::Sender<std::result::Result<u32, String>>,
    result_tx: mpsc::Sender<RawKeyBinding>,
) {
    use std::cell::RefCell;
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, PostQuitMessage, SetWindowsHookExW,
        UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL,
        WM_KEYUP, WM_SYSKEYUP, WM_XBUTTONDOWN,
    };

    thread_local! {
        static CAPTURE_SENDER: RefCell<Option<mpsc::Sender<RawKeyBinding>>> =
            const { RefCell::new(None) };
    }

    fn deliver(binding: RawKeyBinding) -> bool {
        let delivered = CAPTURE_SENDER.with(|sender| match sender.borrow_mut().take() {
            Some(sender) => sender.send(binding).is_ok(),
            None => false,
        });
        if delivered {
            tracing::info!(
                "Captured raw key: vk=0x{:X}, scan=0x{:X}, extended={}",
                binding.vk_code,
                binding.scan_code,
                binding.extended
            );
            // Delivered exactly once: end this capture's message loop.
            unsafe { PostQuitMessage(0) };
        }
        delivered
    }

    unsafe extern "system" fn capture_keyboard_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        use windows::Win32::UI::WindowsAndMessaging::{LLKHF_EXTENDED, LLKHF_UP};

        if code >= 0 {
            let keyboard = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            let flags = keyboard.flags;
            let is_up = flags.contains(LLKHF_UP)
                || wparam.0 as u32 == WM_KEYUP
                || wparam.0 as u32 == WM_SYSKEYUP;
            if !is_up {
                let identity = RawKeyBinding {
                    vk_code: keyboard.vkCode,
                    scan_code: keyboard.scanCode,
                    extended: flags.contains(LLKHF_EXTENDED),
                };
                if deliver(identity) {
                    return LRESULT(1);
                }
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }

    unsafe extern "system" fn capture_mouse_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code >= 0 && wparam.0 as u32 == WM_XBUTTONDOWN {
            let mouse = &*(lparam.0 as *const MSLLHOOKSTRUCT);
            if let Some(identity) = mouse_side_button_binding(mouse.mouseData) {
                if deliver(identity) {
                    return LRESULT(1);
                }
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }

    CAPTURE_SENDER.with(|sender| *sender.borrow_mut() = Some(result_tx));

    let keyboard_hook =
        match unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(capture_keyboard_proc), None, 0) } {
            Ok(hook) => hook,
            Err(error) => {
                let _ = ready_tx.send(Err(format!("{error:?}")));
                return;
            }
        };
    let mouse_hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(capture_mouse_proc), None, 0) }
        .map_err(|error| {
            tracing::warn!("Capture mouse side-button hook is unavailable: {error:?}");
        })
        .ok();
    let _ = ready_tx.send(Ok(unsafe { GetCurrentThreadId() }));

    let mut msg = MSG::default();
    unsafe {
        loop {
            // 0 is WM_QUIT, -1 is an error; both end the capture.
            if GetMessageW(&mut msg, None, 0, 0).0 <= 0 {
                break;
            }
            DispatchMessageW(&msg);
        }
        if let Some(mouse_hook) = mouse_hook {
            let _ = UnhookWindowsHookEx(mouse_hook);
        }
        let _ = UnhookWindowsHookEx(keyboard_hook);
    }
}

fn validate_config(config: &HotkeyConfig) -> Result<()> {
    if config.action.is_official() && config.binding != HotkeyBinding::Raw {
        return Err(anyhow!(
            "Official Doubao actions require a raw custom-key binding"
        ));
    }
    match config.binding {
        HotkeyBinding::Raw => {
            if config.raw_key.is_none() {
                return Err(anyhow!("Raw binding requires a captured key"));
            }
            Ok(())
        }
        HotkeyBinding::Standard => {
            let _ = configured_standard_hotkey(config)?;
            Ok(())
        }
    }
}

fn official_doubao_action(
    config: &HotkeyConfig,
    event: HotkeyEvent,
) -> Option<OfficialDoubaoAction> {
    match (config.action, event) {
        (HotkeyAction::OfficialHold, HotkeyEvent::Start) => Some(OfficialDoubaoAction::HoldStart),
        (HotkeyAction::OfficialHold, HotkeyEvent::Stop) => Some(OfficialDoubaoAction::HoldStop),
        (HotkeyAction::OfficialHandsFree, HotkeyEvent::Toggle) => {
            Some(OfficialDoubaoAction::HandsFree)
        }
        _ => None,
    }
}

/// Whether a physical event identity matches the captured raw key.
#[cfg(target_os = "windows")]
fn raw_key_matches(raw: RawKeyConfig, identity: RawKeyBinding) -> bool {
    raw.vk_code == identity.vk_code
        && raw.scan_code.is_none_or(|scan| scan == identity.scan_code)
        && raw.extended == identity.extended
}

/// Inject the shortcut for an official Doubao input mode.
#[cfg(target_os = "windows")]
pub fn invoke_official_doubao(action: OfficialDoubaoAction) -> Result<()> {
    use std::mem::size_of;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VK_LCONTROL, VK_LWIN, VK_RMENU,
    };

    fn key_input(
        key: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY,
        key_up: bool,
    ) -> INPUT {
        let mut flags: KEYBD_EVENT_FLAGS = Default::default();
        if key == VK_LWIN || key == VK_RMENU {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        if key_up {
            flags |= KEYEVENTF_KEYUP;
        }
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        }
    }

    let inputs = match action {
        OfficialDoubaoAction::HoldStart => vec![key_input(VK_RMENU, false)],
        OfficialDoubaoAction::HoldStop => vec![key_input(VK_RMENU, true)],
        OfficialDoubaoAction::HandsFree => vec![
            key_input(VK_LCONTROL, false),
            key_input(VK_LWIN, false),
            key_input(VK_LWIN, true),
            key_input(VK_LCONTROL, true),
        ],
    };
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        return Err(anyhow!(
            "SendInput sent {sent} of {} events for the official Doubao action",
            inputs.len()
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn invoke_official_doubao(_action: OfficialDoubaoAction) -> Result<()> {
    Err(anyhow!(
        "The official Doubao input method shortcut is only available on Windows"
    ))
}

/// The `global-hotkey` registration for a standard binding, or `None` when
/// the configured key is a pure modifier handled by the keyboard hook.
fn configured_standard_hotkey(config: &HotkeyConfig) -> Result<Option<HotKey>> {
    if ModifierKey::from_key_name(&config.standard_key).is_some() {
        Ok(None)
    } else {
        Ok(Some(parse_standard_binding(&config.standard_key)?))
    }
}

fn parse_standard_binding(key: &str) -> Result<HotKey> {
    if key.contains('+') {
        parse_combo_key(key)
    } else {
        Ok(HotKey::new(None, parse_key_code(key)?))
    }
}

/// Windows low-level keyboard hook for raw bindings.
#[cfg(target_os = "windows")]
fn run_raw_key_hook(
    snapshot: Arc<RwLock<HookSnapshot>>,
    is_active: Arc<AtomicBool>,
    callback: EventCallback,
    detector: Arc<Mutex<TriggerDetector>>,
) {
    use std::cell::RefCell;
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
        KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYUP, WM_SYSKEYUP,
        WM_XBUTTONDOWN, WM_XBUTTONUP,
    };

    struct HookState {
        snapshot: Arc<RwLock<HookSnapshot>>,
        is_active: Arc<AtomicBool>,
        callback: EventCallback,
        detector: Arc<Mutex<TriggerDetector>>,
    }

    impl HookState {
        /// Feed one transition into the shared detector and deliver the
        /// resulting event.  The detector lock is released before the
        /// callback runs.
        fn dispatch(&self, input: TriggerInput, snapshot: HookSnapshot, edge: FireEdge) {
            let emitted = match self.detector.lock() {
                Ok(mut detector) => detector.handle(
                    input,
                    snapshot.mode,
                    snapshot.interval,
                    edge,
                    Instant::now(),
                ),
                Err(_) => None,
            };
            if let Some(event) = emitted {
                (self.callback)(event);
            }
        }
    }

    thread_local! {
        static HOOK_STATE: RefCell<Option<HookState>> = const { RefCell::new(None) };
    }

    HOOK_STATE.with(|state| {
        *state.borrow_mut() = Some(HookState {
            snapshot,
            is_active,
            callback,
            detector,
        });
    });

    unsafe extern "system" fn keyboard_hook_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        use windows::Win32::UI::WindowsAndMessaging::{LLKHF_EXTENDED, LLKHF_INJECTED, LLKHF_UP};

        if code >= 0 {
            let keyboard = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            let flags = keyboard.flags;
            let swallow = HOOK_STATE.with(|state| {
                let state = state.borrow();
                let Some(hook) = state.as_ref() else {
                    return false;
                };
                if !hook.is_active.load(Ordering::SeqCst) {
                    return false;
                }

                // Injected keys (including our own official-Doubao
                // injection) never trigger bindings.
                if flags.contains(LLKHF_INJECTED) {
                    return false;
                }

                let identity = RawKeyBinding {
                    vk_code: keyboard.vkCode,
                    scan_code: keyboard.scanCode,
                    extended: flags.contains(LLKHF_EXTENDED),
                };
                let is_up = flags.contains(LLKHF_UP)
                    || wparam.0 as u32 == WM_KEYUP
                    || wparam.0 as u32 == WM_SYSKEYUP;

                let snapshot = match hook.snapshot.read() {
                    Ok(snapshot) => *snapshot,
                    Err(_) => return false,
                };
                let raw_matches = snapshot
                    .raw_key
                    .is_some_and(|raw| raw_key_matches(raw, identity));
                let modifier_matches = snapshot
                    .modifier
                    .is_some_and(|modifier| modifier.matches_vk(identity.vk_code));
                // Official actions consume the physical key, including
                // auto-repeats, so the official input method only sees
                // the injected shortcut. Otherwise a second hands-free
                // press would end the session as "any key" and the
                // injected Ctrl+Win would immediately restart it.
                let swallow = raw_matches && snapshot.action.is_official();

                if raw_matches || modifier_matches {
                    // Scan code and extended flag can differ between the
                    // press and release of the same physical key; keying the
                    // detector by virtual-key code keeps both transitions on
                    // one identity.
                    let key = TriggerKey::Raw(RawKeyBinding {
                        vk_code: identity.vk_code,
                        scan_code: 0,
                        extended: false,
                    });
                    let input = if is_up {
                        TriggerInput::Release(key)
                    } else {
                        TriggerInput::Press(key)
                    };
                    let edge = if modifier_matches {
                        FireEdge::Release
                    } else {
                        FireEdge::Press
                    };
                    hook.dispatch(input, snapshot, edge);
                } else if !is_up && snapshot.standard_binding {
                    // A pure modifier only counts when it was pressed and
                    // released without participating in another shortcut.
                    hook.dispatch(TriggerInput::ForeignPress, snapshot, FireEdge::Release);
                }

                swallow
            });
            if swallow {
                return LRESULT(1);
            }
        }

        CallNextHookEx(None, code, wparam, lparam)
    }

    unsafe extern "system" fn mouse_hook_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code >= 0 && matches!(wparam.0 as u32, WM_XBUTTONDOWN | WM_XBUTTONUP) {
            let mouse = &*(lparam.0 as *const MSLLHOOKSTRUCT);
            if let Some(identity) = mouse_side_button_binding(mouse.mouseData) {
                let swallow = HOOK_STATE.with(|state| {
                    let state = state.borrow();
                    let Some(hook) = state.as_ref() else {
                        return false;
                    };
                    if !hook.is_active.load(Ordering::SeqCst) {
                        return false;
                    }
                    let is_up = wparam.0 as u32 == WM_XBUTTONUP;
                    let snapshot = match hook.snapshot.read() {
                        Ok(snapshot) => *snapshot,
                        Err(_) => return false,
                    };
                    let raw_matches = snapshot
                        .raw_key
                        .is_some_and(|raw| raw_key_matches(raw, identity));
                    let swallow = raw_matches && snapshot.action.is_official();
                    if raw_matches {
                        let key = TriggerKey::Raw(identity);
                        let input = if is_up {
                            TriggerInput::Release(key)
                        } else {
                            TriggerInput::Press(key)
                        };
                        hook.dispatch(input, snapshot, FireEdge::Press);
                    }
                    swallow
                });
                if swallow {
                    return LRESULT(1);
                }
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }

    let keyboard_hook =
        unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0) };
    match keyboard_hook {
        Ok(keyboard_hook) => {
            tracing::info!("Raw keyboard hook installed");
            let mouse_hook =
                unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), None, 0) }
                    .map_err(|error| {
                        tracing::warn!("Mouse side-button hook is unavailable: {:?}", error);
                    })
                    .ok();
            if mouse_hook.is_some() {
                tracing::info!("Mouse side-button hook installed");
            }
            let mut msg = MSG::default();
            unsafe {
                loop {
                    // 0 is WM_QUIT; -1 is an error and must not be
                    // dispatched as a message.
                    if GetMessageW(&mut msg, None, 0, 0).0 <= 0 {
                        break;
                    }
                    DispatchMessageW(&msg);
                }
                if let Some(mouse_hook) = mouse_hook {
                    let _ = UnhookWindowsHookEx(mouse_hook);
                }
                let _ = UnhookWindowsHookEx(keyboard_hook);
            }
            tracing::warn!("Raw key hook thread exited; raw bindings are no longer monitored");
        }
        Err(error) => tracing::error!("Failed to install raw keyboard hook: {:?}", error),
    }
}

#[cfg(target_os = "windows")]
fn mouse_side_button_binding(mouse_data: u32) -> Option<RawKeyBinding> {
    let vk_code = match (mouse_data >> 16) as u16 {
        1 => 0x05, // VK_XBUTTON1
        2 => 0x06, // VK_XBUTTON2
        _ => return None,
    };
    Some(RawKeyBinding {
        vk_code,
        scan_code: 0,
        extended: false,
    })
}

/// Parse a combo key string like `Ctrl+Shift+V`.
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
            _ => key_code = Some(parse_key_code(part)?),
        }
    }

    let code = key_code.ok_or_else(|| anyhow!("No key specified in combo: {}", key_str))?;
    Ok(HotKey::new(Some(modifiers), code))
}

/// Parse a standard key name.  Raw vendor keys intentionally do not go
/// through this parser because they have no stable `Code` representation.
fn parse_key_code(key: &str) -> Result<Code> {
    let key = key.to_uppercase();
    let code = match key.as_str() {
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
        "F13" => Code::F13,
        "F14" => Code::F14,
        "F15" => Code::F15,
        "F16" => Code::F16,
        "F17" => Code::F17,
        "F18" => Code::F18,
        "F19" => Code::F19,
        "F20" => Code::F20,
        "F21" => Code::F21,
        "F22" => Code::F22,
        "F23" => Code::F23,
        "F24" => Code::F24,
        "VOLUMEUP" | "AUDIOVOLUMEUP" => Code::AudioVolumeUp,
        "VOLUMEDOWN" | "AUDIOVOLUMEDOWN" => Code::AudioVolumeDown,
        "VOLUMEMUTE" | "AUDIOVOLUMEMUTE" => Code::AudioVolumeMute,
        "MEDIAPLAY" => Code::MediaPlay,
        "MEDIAPAUSE" => Code::MediaPause,
        "MEDIAPLAYPAUSE" => Code::MediaPlayPause,
        "MEDIASTOP" => Code::MediaStop,
        "MEDIANEXT" | "MEDIATRACKNEXT" => Code::MediaTrackNext,
        "MEDIAPREV" | "MEDIATRACKPREV" => Code::MediaTrackPrevious,
        _ => return Err(anyhow!("Unknown key: {}", key)),
    };
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::TriggerMode;

    #[test]
    fn official_action_routes_press_events_but_not_hold_release() {
        let mut config = HotkeyConfig::default();
        config.action = HotkeyAction::OfficialHandsFree;

        assert_eq!(config.effective_mode(), TriggerMode::SingleTap);
        assert_eq!(
            official_doubao_action(&config, HotkeyEvent::Toggle),
            Some(OfficialDoubaoAction::HandsFree)
        );
        assert_eq!(official_doubao_action(&config, HotkeyEvent::Stop), None);
    }

    #[test]
    fn voice_input_action_keeps_all_events_in_the_local_pipeline() {
        let config = HotkeyConfig::default();

        assert_eq!(official_doubao_action(&config, HotkeyEvent::Toggle), None);
        assert_eq!(official_doubao_action(&config, HotkeyEvent::Start), None);
        assert_eq!(official_doubao_action(&config, HotkeyEvent::Stop), None);
    }

    #[test]
    fn official_hold_routes_raw_press_and_release() {
        let mut config = HotkeyConfig::default();
        config.action = HotkeyAction::OfficialHold;

        assert_eq!(config.effective_mode(), TriggerMode::Hold);
        assert_eq!(
            official_doubao_action(&config, HotkeyEvent::Start),
            Some(OfficialDoubaoAction::HoldStart)
        );
        assert_eq!(
            official_doubao_action(&config, HotkeyEvent::Stop),
            Some(OfficialDoubaoAction::HoldStop)
        );
    }

    #[test]
    fn validate_config_requires_captured_key_for_raw_binding() {
        let mut config = HotkeyConfig::default();
        config.binding = HotkeyBinding::Raw;

        assert!(validate_config(&config).is_err());

        config.raw_key = Some(RawKeyConfig {
            vk_code: 0xFF,
            scan_code: Some(0x72),
            extended: false,
        });
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn validate_config_rejects_unknown_standard_key() {
        let mut config = HotkeyConfig::default();
        config.standard_key = "NotAKey".to_string();

        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn pure_modifier_bindings_skip_global_hotkey_registration() {
        let mut config = HotkeyConfig::default();
        for key in ["Ctrl", "shift", "ALT", "Win", "super"] {
            config.standard_key = key.to_string();
            assert!(matches!(configured_standard_hotkey(&config), Ok(None)));
        }

        config.standard_key = "Ctrl+Shift+V".to_string();
        assert!(matches!(configured_standard_hotkey(&config), Ok(Some(_))));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn only_official_actions_swallow_the_matching_raw_binding() {
        let identity = RawKeyBinding {
            vk_code: 0xFF,
            scan_code: 0x1E,
            extended: false,
        };
        let mut config = HotkeyConfig::default();
        config.binding = HotkeyBinding::Raw;
        config.raw_key = Some(RawKeyConfig {
            vk_code: 0xFF,
            scan_code: Some(0x1E),
            extended: false,
        });

        let snapshot = HookSnapshot::from_config(&config);
        assert!(snapshot
            .raw_key
            .is_some_and(|raw| raw_key_matches(raw, identity)));
        assert!(!snapshot.action.is_official());

        config.action = HotkeyAction::OfficialHandsFree;
        assert!(HookSnapshot::from_config(&config).action.is_official());

        config.raw_key = Some(RawKeyConfig {
            vk_code: 0xFE,
            scan_code: Some(0x1E),
            extended: false,
        });
        let snapshot = HookSnapshot::from_config(&config);
        assert!(!snapshot
            .raw_key
            .is_some_and(|raw| raw_key_matches(raw, identity)));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn raw_binding_without_scan_code_matches_any_scan_code() {
        let raw = RawKeyConfig {
            vk_code: 0x05,
            scan_code: None,
            extended: false,
        };

        assert!(raw_key_matches(
            raw,
            RawKeyBinding {
                vk_code: 0x05,
                scan_code: 0,
                extended: false,
            }
        ));
        assert!(raw_key_matches(
            raw,
            RawKeyBinding {
                vk_code: 0x05,
                scan_code: 0x2A,
                extended: false,
            }
        ));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn hook_snapshot_scopes_raw_and_modifier_matching_by_binding() {
        let mut config = HotkeyConfig::default();
        config.standard_key = "Ctrl".to_string();
        config.raw_key = Some(RawKeyConfig {
            vk_code: 0xFF,
            scan_code: None,
            extended: false,
        });

        let standard = HookSnapshot::from_config(&config);
        assert_eq!(standard.modifier, Some(ModifierKey::Control));
        assert_eq!(standard.raw_key, None);
        assert!(standard.standard_binding);

        config.binding = HotkeyBinding::Raw;
        let raw = HookSnapshot::from_config(&config);
        assert_eq!(raw.modifier, None);
        assert!(raw.raw_key.is_some());
        assert!(!raw.standard_binding);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn win_key_matches_left_and_right_variants() {
        assert!(ModifierKey::Win.matches_vk(0x5B));
        assert!(ModifierKey::Win.matches_vk(0x5C));
        assert!(!ModifierKey::Win.matches_vk(0x11));
    }
}
