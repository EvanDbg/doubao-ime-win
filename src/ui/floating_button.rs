//! Floating Button
//!
//! A floating button that shows the voice input status and allows user to trigger recording.
//! Uses Win32 API with timer-based drag tracking for smooth operation.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// Floating button state
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum ButtonState {
    /// Idle - not recording (purple)
    Idle = 0,
    /// Recording in progress (red)
    Recording = 1,
    /// Processing (waiting for ASR result) (blue)
    Processing = 2,
}

impl From<u8> for ButtonState {
    fn from(v: u8) -> Self {
        match v {
            1 => ButtonState::Recording,
            2 => ButtonState::Processing,
            _ => ButtonState::Idle,
        }
    }
}

/// Events from the floating button
#[derive(Debug, Clone)]
pub enum FloatingButtonEvent {
    /// User clicked the button to toggle recording
    ToggleRecording,
    /// User requested to exit
    Exit,
}

/// Floating button configuration
#[derive(Clone)]
pub struct FloatingButtonConfig {
    pub initial_x: i32,
    pub initial_y: i32,
    pub size: i32,
}

impl Default for FloatingButtonConfig {
    fn default() -> Self {
        Self {
            initial_x: 100,
            initial_y: 100,
            size: 56,
        }
    }
}

/// State setter for the floating button (thread-safe)
#[derive(Clone)]
pub struct FloatingButtonStateSetter {
    state: Arc<AtomicU8>,
    hwnd: Arc<AtomicI32>,
}

impl FloatingButtonStateSetter {
    /// Set the button state
    pub fn set_state(&self, state: ButtonState) {
        self.state.store(state as u8, Ordering::SeqCst);
        // Trigger repaint
        #[cfg(target_os = "windows")]
        {
            let hwnd_val = self.hwnd.load(Ordering::SeqCst);
            if hwnd_val != 0 {
                unsafe {
                    use windows::Win32::Foundation::*;
                    use windows::Win32::Graphics::Gdi::InvalidateRect;
                    let hwnd = HWND(hwnd_val as isize);
                    let _ = InvalidateRect(hwnd, None, TRUE);
                }
            }
        }
        tracing::debug!("Floating button state: {:?}", state);
    }

    /// Get the current state
    pub fn get_state(&self) -> ButtonState {
        self.state.load(Ordering::SeqCst).into()
    }
}

/// Floating button manager
pub struct FloatingButton {
    state: Arc<AtomicU8>,
    hwnd: Arc<AtomicI32>,
    event_tx: Sender<FloatingButtonEvent>,
    event_rx: Option<Receiver<FloatingButtonEvent>>,
}

impl FloatingButton {
    /// Create a new floating button
    pub fn new() -> Self {
        let (event_tx, event_rx) = channel();
        Self {
            state: Arc::new(AtomicU8::new(ButtonState::Idle as u8)),
            hwnd: Arc::new(AtomicI32::new(0)),
            event_tx,
            event_rx: Some(event_rx),
        }
    }

    /// Get a state setter that can be used from other threads
    pub fn state_setter(&self) -> FloatingButtonStateSetter {
        FloatingButtonStateSetter {
            state: self.state.clone(),
            hwnd: self.hwnd.clone(),
        }
    }

    /// Take the event receiver (can only be called once)
    pub fn take_event_receiver(&mut self) -> Option<Receiver<FloatingButtonEvent>> {
        self.event_rx.take()
    }

    /// Run the floating button (blocking, call from a dedicated thread)
    #[cfg(target_os = "windows")]
    pub fn run(self, config: FloatingButtonConfig) {
        use std::mem::size_of;
        use windows::core::w;
        use windows::Win32::Foundation::*;
        use windows::Win32::Graphics::Gdi::*;
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
        use windows::Win32::UI::WindowsAndMessaging::*;

        const DRAG_TIMER_ID: usize = 1;
        const BUTTON_RADIUS: i32 = 22;

        // Thread-local state
        static MOUSE_DOWN: AtomicBool = AtomicBool::new(false);
        static START_CURSOR_X: AtomicI32 = AtomicI32::new(0);
        static START_CURSOR_Y: AtomicI32 = AtomicI32::new(0);
        static START_WIN_X: AtomicI32 = AtomicI32::new(0);
        static START_WIN_Y: AtomicI32 = AtomicI32::new(0);

        // Store shared state in thread-local for wndproc access
        thread_local! {
            static SHARED_STATE: std::cell::RefCell<Option<Arc<AtomicU8>>> = const { std::cell::RefCell::new(None) };
            static EVENT_SENDER: std::cell::RefCell<Option<Sender<FloatingButtonEvent>>> = const { std::cell::RefCell::new(None) };
        }

        let state = self.state.clone();
        let hwnd_store = self.hwnd.clone();
        let event_tx = self.event_tx.clone();
        let window_size = config.size;

        SHARED_STATE.with(|s| *s.borrow_mut() = Some(state));
        EVENT_SENDER.with(|s| *s.borrow_mut() = Some(event_tx));

        unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
            use windows::Win32::Foundation::*;
            use windows::Win32::Graphics::Gdi::*;
            use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
            use windows::Win32::UI::WindowsAndMessaging::*;

            const WM_CREATE: u32 = 0x0001;
            const WM_DESTROY: u32 = 0x0002;
            const WM_PAINT: u32 = 0x000F;
            const WM_TIMER: u32 = 0x0113;
            const WM_LBUTTONDOWN: u32 = 0x0201;
            const WM_LBUTTONUP: u32 = 0x0202;
            const WM_RBUTTONUP: u32 = 0x0205;
            const DRAG_TIMER_ID: usize = 1;
            const BUTTON_RADIUS: i32 = 22;

            match msg {
                WM_CREATE => {
                    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0x00FF00), 0, LWA_COLORKEY);
                    LRESULT(0)
                }
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    let hdc = BeginPaint(hwnd, &mut ps);

                    // Get window size
                    let mut rect = RECT::default();
                    let _ = GetClientRect(hwnd, &mut rect);
                    let window_size = rect.right;
                    let center = window_size / 2;

                    // Green background (transparent)
                    let bg = CreateSolidBrush(COLORREF(0x00FF00));
                    FillRect(hdc, &rect, bg);
                    let _ = DeleteObject(bg);

                    // Get state
                    let state_val = SHARED_STATE.with(|s| {
                        s.borrow().as_ref().map(|st| st.load(Ordering::SeqCst)).unwrap_or(0)
                    });

                    // Color based on state - modern gradient colors matching tray icon
                    let (inner_color, outer_color) = match state_val {
                        1 => (COLORREF(0x5555EF), COLORREF(0x3535BF)), // Red for recording
                        2 => (COLORREF(0xF68230), COLORREF(0xC66020)), // Orange for processing
                        _ => (COLORREF(0xF65C8B), COLORREF(0xC64868)), // Purple/pink for idle
                    };

                    // Draw outer circle (shadow/border)
                    let outer_brush = CreateSolidBrush(outer_color);
                    let outer_pen = CreatePen(PS_NULL, 0, COLORREF(0));
                    let ob1 = SelectObject(hdc, outer_brush);
                    let op1 = SelectObject(hdc, outer_pen);
                    let _ = Ellipse(hdc, center - BUTTON_RADIUS - 2, center - BUTTON_RADIUS - 2,
                                   center + BUTTON_RADIUS + 2, center + BUTTON_RADIUS + 2);
                    SelectObject(hdc, ob1);
                    SelectObject(hdc, op1);
                    let _ = DeleteObject(outer_brush);
                    let _ = DeleteObject(outer_pen);

                    // Draw inner circle
                    let inner_brush = CreateSolidBrush(inner_color);
                    let white_pen = CreatePen(PS_SOLID, 2, COLORREF(0xFFFFFF));
                    let ob2 = SelectObject(hdc, inner_brush);
                    let op2 = SelectObject(hdc, white_pen);
                    let _ = Ellipse(hdc, center - BUTTON_RADIUS, center - BUTTON_RADIUS,
                                   center + BUTTON_RADIUS, center + BUTTON_RADIUS);
                    SelectObject(hdc, ob2);
                    SelectObject(hdc, op2);
                    let _ = DeleteObject(inner_brush);
                    let _ = DeleteObject(white_pen);

                    // Draw icon based on state with modern design
                    let icon_color = COLORREF(0xFFFFFF);
                    let icon_brush = CreateSolidBrush(icon_color);
                    let icon_pen = CreatePen(PS_SOLID, 3, icon_color);
                    let ob3 = SelectObject(hdc, icon_brush);
                    let op3 = SelectObject(hdc, icon_pen);

                    match state_val {
                        1 => {
                            // Recording: draw rounded stop square with border
                            let sq = 8;
                            let _ = RoundRect(hdc, center - sq, center - sq,
                                            center + sq, center + sq, 4, 4);
                        }
                        2 => {
                            // Processing: draw three animated-style dots
                            let dot_r = 4;
                            let spacing = 10;
                            // Left dot
                            let _ = Ellipse(hdc, center - spacing - dot_r, center - dot_r + 2,
                                          center - spacing + dot_r, center + dot_r + 2);
                            // Center dot (slightly higher for wave effect)
                            let _ = Ellipse(hdc, center - dot_r, center - dot_r - 2,
                                          center + dot_r, center + dot_r - 2);
                            // Right dot
                            let _ = Ellipse(hdc, center + spacing - dot_r, center - dot_r + 2,
                                          center + spacing + dot_r, center + dot_r + 2);
                        }
                        _ => {
                            // Idle: draw modern microphone icon
                            // Mic head (pill shape)
                            let _ = RoundRect(hdc, center - 5, center - 10,
                                            center + 5, center + 2, 6, 6);
                            // Mic arc (using lines for C-shape)
                            let arc_pen = CreatePen(PS_SOLID, 2, icon_color);
                            let op_arc = SelectObject(hdc, arc_pen);
                            // Left arc
                            let _ = MoveToEx(hdc, center - 8, center - 2, None);
                            let _ = LineTo(hdc, center - 8, center + 4);
                            // Bottom curve (approximated with lines)
                            let _ = LineTo(hdc, center - 6, center + 7);
                            let _ = LineTo(hdc, center, center + 8);
                            let _ = LineTo(hdc, center + 6, center + 7);
                            let _ = LineTo(hdc, center + 8, center + 4);
                            // Right arc
                            let _ = LineTo(hdc, center + 8, center - 2);
                            // Stem
                            let _ = MoveToEx(hdc, center, center + 8, None);
                            let _ = LineTo(hdc, center, center + 12);
                            // Base
                            let _ = MoveToEx(hdc, center - 5, center + 12, None);
                            let _ = LineTo(hdc, center + 5, center + 12);
                            SelectObject(hdc, op_arc);
                            let _ = DeleteObject(arc_pen);
                        }
                    }

                    SelectObject(hdc, ob3);
                    SelectObject(hdc, op3);
                    let _ = DeleteObject(icon_brush);
                    let _ = DeleteObject(icon_pen);

                    EndPaint(hwnd, &ps);
                    LRESULT(0)
                }
                WM_LBUTTONDOWN => {
                    MOUSE_DOWN.store(true, Ordering::SeqCst);

                    let mut pt = POINT::default();
                    let _ = GetCursorPos(&mut pt);
                    START_CURSOR_X.store(pt.x, Ordering::SeqCst);
                    START_CURSOR_Y.store(pt.y, Ordering::SeqCst);

                    let mut rect = RECT::default();
                    let _ = GetWindowRect(hwnd, &mut rect);
                    START_WIN_X.store(rect.left, Ordering::SeqCst);
                    START_WIN_Y.store(rect.top, Ordering::SeqCst);

                    let _ = SetTimer(hwnd, DRAG_TIMER_ID, 16, None);
                    LRESULT(0)
                }
                WM_TIMER => {
                    if wparam.0 == DRAG_TIMER_ID && MOUSE_DOWN.load(Ordering::SeqCst) {
                        let key_state = GetAsyncKeyState(0x01);
                        if (key_state & 0x8000u16 as i16) == 0 {
                            MOUSE_DOWN.store(false, Ordering::SeqCst);
                            let _ = KillTimer(hwnd, DRAG_TIMER_ID);

                            let mut pt = POINT::default();
                            let _ = GetCursorPos(&mut pt);
                            let dx = (pt.x - START_CURSOR_X.load(Ordering::SeqCst)).abs();
                            let dy = (pt.y - START_CURSOR_Y.load(Ordering::SeqCst)).abs();

                            if dx < 5 && dy < 5 {
                                EVENT_SENDER.with(|s| {
                                    if let Some(ref tx) = *s.borrow() {
                                        let _ = tx.send(FloatingButtonEvent::ToggleRecording);
                                    }
                                });
                            }
                        } else {
                            let mut pt = POINT::default();
                            let _ = GetCursorPos(&mut pt);
                            let dx = pt.x - START_CURSOR_X.load(Ordering::SeqCst);
                            let dy = pt.y - START_CURSOR_Y.load(Ordering::SeqCst);
                            let new_x = START_WIN_X.load(Ordering::SeqCst) + dx;
                            let new_y = START_WIN_Y.load(Ordering::SeqCst) + dy;
                            let _ = SetWindowPos(hwnd, HWND_TOPMOST, new_x, new_y, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
                        }
                    }
                    LRESULT(0)
                }
                WM_LBUTTONUP => {
                    if MOUSE_DOWN.load(Ordering::SeqCst) {
                        MOUSE_DOWN.store(false, Ordering::SeqCst);
                        let _ = KillTimer(hwnd, DRAG_TIMER_ID);

                        let mut pt = POINT::default();
                        let _ = GetCursorPos(&mut pt);
                        let dx = (pt.x - START_CURSOR_X.load(Ordering::SeqCst)).abs();
                        let dy = (pt.y - START_CURSOR_Y.load(Ordering::SeqCst)).abs();

                        if dx < 5 && dy < 5 {
                            EVENT_SENDER.with(|s| {
                                if let Some(ref tx) = *s.borrow() {
                                    let _ = tx.send(FloatingButtonEvent::ToggleRecording);
                                }
                            });
                        }
                    }
                    LRESULT(0)
                }
                WM_RBUTTONUP => {
                    // Right-click to show exit confirmation
                    use windows::core::w;
                    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_YESNO, MB_ICONQUESTION, IDYES};
                    let result = MessageBoxW(
                        hwnd,
                        w!("确定要退出豆包语音输入吗？"),
                        w!("退出确认"),
                        MB_YESNO | MB_ICONQUESTION,
                    );
                    if result == IDYES {
                        EVENT_SENDER.with(|s| {
                            if let Some(ref tx) = *s.borrow() {
                                let _ = tx.send(FloatingButtonEvent::Exit);
                            }
                        });
                        let _ = DestroyWindow(hwnd);
                    }
                    LRESULT(0)
                }
                WM_DESTROY => {
                    let _ = KillTimer(hwnd, DRAG_TIMER_ID);
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }

        unsafe {
            let inst = match GetModuleHandleW(None) {
                Ok(h) => h,
                Err(e) => {
                    tracing::error!("GetModuleHandleW failed: {:?}", e);
                    return;
                }
            };

            let cls = w!("DoubaoFloatingButton");
            let cursor = LoadCursorW(None, IDC_HAND).unwrap_or_else(|_| {
                LoadCursorW(None, IDC_ARROW).unwrap_or_default()
            });

            let wc = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wnd_proc),
                hInstance: inst.into(),
                hCursor: cursor,
                lpszClassName: cls,
                ..Default::default()
            };
            RegisterClassExW(&wc);

            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                cls,
                w!("豆包语音"),
                WS_POPUP | WS_VISIBLE,
                config.initial_x,
                config.initial_y,
                window_size,
                window_size,
                HWND::default(),
                HMENU::default(),
                inst,
                None,
            );

            if hwnd.0 == 0 {
                tracing::error!("CreateWindowExW failed");
                return;
            }

            hwnd_store.store(hwnd.0 as i32, Ordering::SeqCst);
            tracing::info!("Floating button window created");

            let _ = ShowWindow(hwnd, SW_SHOW);

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            tracing::info!("Floating button window closed");
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn run(self, _config: FloatingButtonConfig) {
        tracing::warn!("Floating button not supported on this platform");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}
