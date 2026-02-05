//! Floating Window Demo - Fixed with Timer-based Drag

#[cfg(target_os = "windows")]
fn main() {
    use std::mem::size_of;
    use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering};
    use windows::core::w;
    use windows::Win32::Foundation::*;
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    use windows::Win32::UI::WindowsAndMessaging::*;

    println!("=== 悬浮窗 Demo ===");

    const WINDOW_SIZE: i32 = 64;
    const BUTTON_RADIUS: i32 = 24;
    const DRAG_TIMER_ID: usize = 1;

    static STATE: AtomicU8 = AtomicU8::new(0);
    static MOUSE_DOWN: AtomicBool = AtomicBool::new(false);
    static START_CURSOR_X: AtomicI32 = AtomicI32::new(0);
    static START_CURSOR_Y: AtomicI32 = AtomicI32::new(0);
    static START_WIN_X: AtomicI32 = AtomicI32::new(0);
    static START_WIN_Y: AtomicI32 = AtomicI32::new(0);

    unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        const WM_CREATE: u32 = 0x0001;
        const WM_DESTROY: u32 = 0x0002;
        const WM_PAINT: u32 = 0x000F;
        const WM_TIMER: u32 = 0x0113;
        const WM_LBUTTONDOWN: u32 = 0x0201;
        const WM_LBUTTONUP: u32 = 0x0202;
        const WM_KEYDOWN: u32 = 0x0100;
        const VK_ESCAPE: usize = 0x1B;

        match msg {
            WM_CREATE => {
                let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0x00FF00), 0, LWA_COLORKEY);
                LRESULT(0)
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                
                let bg = CreateSolidBrush(COLORREF(0x00FF00));
                let mut r = RECT::default();
                let _ = GetClientRect(hwnd, &mut r);
                FillRect(hdc, &r, bg);
                let _ = DeleteObject(bg);
                
                let state = STATE.load(Ordering::SeqCst);
                let color = match state {
                    0 => COLORREF(0xEA7E66),
                    1 => COLORREF(0x4040FF),
                    _ => COLORREF(0xFF8040),
                };
                
                let brush = CreateSolidBrush(color);
                let pen = CreatePen(PS_SOLID, 2, COLORREF(0xFFFFFF));
                let ob = SelectObject(hdc, brush);
                let op = SelectObject(hdc, pen);
                
                let c = WINDOW_SIZE / 2;
                let _ = Ellipse(hdc, c - BUTTON_RADIUS, c - BUTTON_RADIUS, c + BUTTON_RADIUS, c + BUTTON_RADIUS);
                
                SelectObject(hdc, ob);
                SelectObject(hdc, op);
                let _ = DeleteObject(brush);
                let _ = DeleteObject(pen);

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
                
                // Start timer to poll mouse position (16ms = ~60fps)
                let _ = SetTimer(hwnd, DRAG_TIMER_ID, 16, None);
                
                LRESULT(0)
            }
            WM_TIMER => {
                if wparam.0 == DRAG_TIMER_ID && MOUSE_DOWN.load(Ordering::SeqCst) {
                    // Check if left mouse button is still pressed
                    let key_state = GetAsyncKeyState(0x01); // VK_LBUTTON
                    if (key_state & 0x8000u16 as i16) == 0 {
                        // Mouse button released - treat as mouse up
                        MOUSE_DOWN.store(false, Ordering::SeqCst);
                        let _ = KillTimer(hwnd, DRAG_TIMER_ID);
                        
                        // Check for click
                        let mut pt = POINT::default();
                        let _ = GetCursorPos(&mut pt);
                        let dx = (pt.x - START_CURSOR_X.load(Ordering::SeqCst)).abs();
                        let dy = (pt.y - START_CURSOR_Y.load(Ordering::SeqCst)).abs();
                        
                        if dx < 5 && dy < 5 {
                            let cur = STATE.load(Ordering::SeqCst);
                            let nxt = (cur + 1) % 3;
                            STATE.store(nxt, Ordering::SeqCst);
                            println!("状态: {} -> {}", ["空闲","录音","处理"][cur as usize], ["空闲","录音","处理"][nxt as usize]);
                            let _ = InvalidateRect(hwnd, None, TRUE);
                        }
                    } else {
                        // Still dragging - update window position
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
                        let cur = STATE.load(Ordering::SeqCst);
                        let nxt = (cur + 1) % 3;
                        STATE.store(nxt, Ordering::SeqCst);
                        println!("状态: {} -> {}", ["空闲","录音","处理"][cur as usize], ["空闲","录音","处理"][nxt as usize]);
                        let _ = InvalidateRect(hwnd, None, TRUE);
                    }
                }
                LRESULT(0)
            }
            WM_KEYDOWN => {
                if wparam.0 == VK_ESCAPE { let _ = DestroyWindow(hwnd); }
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
            Err(e) => { eprintln!("GetModuleHandleW failed: {:?}", e); return; }
        };
        let cls = w!("FloatDemo2");
        
        let cursor = match LoadCursorW(None, IDC_HAND) {
            Ok(c) => c,
            Err(_) => match LoadCursorW(None, IDC_ARROW) {
                Ok(c) => c,
                Err(e) => { eprintln!("LoadCursorW failed: {:?}", e); return; }
            }
        };
        
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
            cls, w!("Voice"), WS_POPUP | WS_VISIBLE,
            100, 100, WINDOW_SIZE, WINDOW_SIZE,
            HWND::default(), HMENU::default(), inst, None,
        );
        
        if hwnd.0 == 0 {
            eprintln!("CreateWindowExW failed");
            return;
        }

        println!("悬浮窗已创建！");
        println!("- 拖动窗口移动位置");
        println!("- 点击切换状态（紫→红→蓝）");
        println!("- ESC 退出");
        let _ = ShowWindow(hwnd, SW_SHOW);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        println!("Demo结束");
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("This demo only works on Windows");
}
