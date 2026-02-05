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
        
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        
        use windows::Win32::UI::WindowsAndMessaging::*;


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

        // Helper function to update layered window with PNG icon
        unsafe fn update_layered_icon(hwnd: HWND, state_val: u8) {
            use windows::Win32::Foundation::*;
            use windows::Win32::Graphics::Gdi::*;
            use windows::Win32::UI::WindowsAndMessaging::*;

            // Load embedded PNG icon based on state
            let icon_data: &[u8] = match state_val {
                1 => include_bytes!("../../assets/icon_recording.png"),
                2 => include_bytes!("../../assets/icon_processing.png"),
                _ => include_bytes!("../../assets/icon_idle.png"),
            };

            // Decode PNG
            if let Ok(img) = image::load_from_memory(icon_data) {
                let rgba = img.to_rgba8();
                let (img_w, img_h) = rgba.dimensions();

                // Get screen DC
                let hdc_screen = GetDC(HWND::default());
                let hdc_mem = CreateCompatibleDC(hdc_screen);

                // Create 32-bit bitmap
                let bmi = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: img_w as i32,
                        biHeight: -(img_h as i32), // Top-down
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: 0, // BI_RGB
                        biSizeImage: 0,
                        biXPelsPerMeter: 0,
                        biYPelsPerMeter: 0,
                        biClrUsed: 0,
                        biClrImportant: 0,
                    },
                    bmiColors: [RGBQUAD::default()],
                };

                let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
                if let Ok(hbmp) = CreateDIBSection(
                    hdc_mem,
                    &bmi,
                    DIB_RGB_COLORS,
                    &mut bits,
                    None,
                    0,
                ) {
                    if !bits.is_null() {
                        let old_bmp = SelectObject(hdc_mem, hbmp);

                        // Copy pixels with premultiplied alpha (required for UpdateLayeredWindow)
                        let pixel_data = bits as *mut u8;
                        let mut idx = 0usize;
                        for pixel in rgba.pixels() {
                            let r = pixel[0] as u32;
                            let g = pixel[1] as u32;
                            let b = pixel[2] as u32;
                            let a = pixel[3] as u32;

                            // Premultiply alpha
                            let pr = ((r * a) / 255) as u8;
                            let pg = ((g * a) / 255) as u8;
                            let pb = ((b * a) / 255) as u8;

                            *pixel_data.add(idx) = pb;     // B
                            *pixel_data.add(idx + 1) = pg; // G
                            *pixel_data.add(idx + 2) = pr; // R
                            *pixel_data.add(idx + 3) = pixel[3]; // A
                            idx += 4;
                        }

                        // Setup blend function for per-pixel alpha
                        let blend = BLENDFUNCTION {
                            BlendOp: 0, // AC_SRC_OVER
                            BlendFlags: 0,
                            SourceConstantAlpha: 255,
                            AlphaFormat: 1, // AC_SRC_ALPHA
                        };

                        let size = SIZE { cx: img_w as i32, cy: img_h as i32 };
                        let pt_src = POINT { x: 0, y: 0 };

                        // Update layered window
                        let _ = UpdateLayeredWindow(
                            hwnd,
                            hdc_screen,
                            None,
                            Some(&size),
                            hdc_mem,
                            Some(&pt_src),
                            COLORREF(0),
                            Some(&blend),
                            ULW_ALPHA,
                        );

                        SelectObject(hdc_mem, old_bmp);
                        let _ = DeleteObject(hbmp);
                    }
                }

                let _ = DeleteDC(hdc_mem);
                let _ = ReleaseDC(HWND::default(), hdc_screen);
            }
        }

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

            match msg {
                WM_CREATE => {
                    // Use UpdateLayeredWindow for per-pixel alpha, initial update
                    update_layered_icon(hwnd, 0);
                    LRESULT(0)
                }
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    let _ = BeginPaint(hwnd, &mut ps);
                    // Get current state and update layered window
                    let state_val = SHARED_STATE.with(|s| {
                        s.borrow().as_ref().map(|st| st.load(Ordering::SeqCst)).unwrap_or(0)
                    });
                    update_layered_icon(hwnd, state_val);
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
