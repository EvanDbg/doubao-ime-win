//! Native Win32 settings window for voice input options.

#[cfg(target_os = "windows")]
mod windows_impl {
    use anyhow::{anyhow, Result};
    use std::mem::size_of;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, CreateSolidBrush, EndPaint, GetStockObject, DEFAULT_GUI_FONT, HBRUSH,
        PAINTSTRUCT,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows::Win32::UI::WindowsAndMessaging::*;

    use crate::data::RuntimeSettings;

    const SETTINGS_CLASS: PCWSTR = w!("DoubaoVoiceSettings");
    const SETTINGS_TITLE: PCWSTR = w!("豆包语音输入设置");

    const ID_CAPTION_PUNCTUATION: isize = 101;
    const ID_SENTENCE_PUNCTUATION: isize = 102;
    const ID_AI_ENABLED: isize = 103;
    const ID_AI_ENDPOINT: isize = 104;
    const ID_AI_MODEL: isize = 105;
    const ID_AI_API_KEY: isize = 106;
    const ID_ACCEPT_INVALID_CERTS: isize = 107;
    const ID_SAVE: isize = 108;
    const ID_EXIT: isize = 109;
    const ID_STATUS: isize = 110;

    struct Ui {
        settings: Arc<RuntimeSettings>,
        caption_punctuation: HWND,
        sentence_punctuation: HWND,
        ai_enabled: HWND,
        ai_endpoint: HWND,
        ai_model: HWND,
        ai_api_key: HWND,
        accept_invalid_certs: HWND,
        status: HWND,
    }

    thread_local! {
        static CURRENT_UI: std::cell::RefCell<Option<Ui>> = const { std::cell::RefCell::new(None) };
    }

    static SETTINGS_OPEN: AtomicUsize = AtomicUsize::new(0);
    static EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);

    pub fn exit_requested() -> bool {
        EXIT_REQUESTED.load(Ordering::SeqCst)
    }

    pub fn open(settings: Arc<RuntimeSettings>) {
        EXIT_REQUESTED.store(false, Ordering::SeqCst);
        let existing = SETTINGS_OPEN.load(Ordering::SeqCst);
        if existing != 0 {
            unsafe {
                let _ = SetForegroundWindow(HWND(existing as isize));
            }
            return;
        }

        SETTINGS_OPEN.store(unsafe { GetCurrentThreadId() } as usize, Ordering::SeqCst);
        std::thread::spawn(move || {
            if let Err(e) = run_window(settings) {
                tracing::error!("Failed to open settings window: {:?}", e);
            }
        });
    }

    unsafe fn set_control_font(hwnd: HWND) {
        let font = GetStockObject(DEFAULT_GUI_FONT);
        SendMessageW(hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
    }

    unsafe fn is_checked(hwnd: HWND) -> bool {
        SendMessageW(hwnd, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 as u32
            == windows::Win32::UI::Controls::BST_CHECKED.0
    }

    unsafe fn set_checked(hwnd: HWND, checked: bool) {
        SendMessageW(
            hwnd,
            BM_SETCHECK,
            WPARAM(if checked {
                windows::Win32::UI::Controls::BST_CHECKED.0 as usize
            } else {
                windows::Win32::UI::Controls::BST_UNCHECKED.0 as usize
            }),
            LPARAM(0),
        );
    }

    unsafe fn read_text(hwnd: HWND) -> String {
        let length = SendMessageW(hwnd, WM_GETTEXTLENGTH, WPARAM(0), LPARAM(0)).0 as usize;
        if length == 0 {
            return String::new();
        }
        let mut buffer = vec![0u16; length + 1];
        SendMessageW(
            hwnd,
            WM_GETTEXT,
            WPARAM(length + 1),
            LPARAM(buffer.as_mut_ptr() as isize),
        );
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..end])
    }

    unsafe fn set_text(hwnd: HWND, text: &str) {
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        wide.push(0);
        let _ = SetWindowTextW(hwnd, PCWSTR::from_raw(wide.as_ptr()));
    }

    unsafe fn create_control(
        parent: HWND,
        class: PCWSTR,
        text: &str,
        style: i32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        id: isize,
        tab_stop: bool,
    ) -> HWND {
        let mut text_wide: Vec<u16> = text.encode_utf16().collect();
        text_wide.push(0);
        let tab_stop_style = if tab_stop { WS_TABSTOP.0 as i32 } else { 0 };
        let style_value = WINDOW_STYLE(
            ((WS_CHILD.0 as i32) | (WS_VISIBLE.0 as i32) | style | tab_stop_style) as u32,
        );
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            PCWSTR::from_raw(text_wide.as_ptr()),
            style_value,
            x,
            y,
            width,
            height,
            parent,
            HMENU(id),
            GetModuleHandleW(None).unwrap_or_default(),
            None,
        );
        if hwnd == HWND::default() {
            panic!("CreateWindowExW failed");
        }
        set_control_font(hwnd);
        hwnd
    }

    unsafe fn load_controls(ui: &Ui) {
        let config = ui.settings.get();
        set_checked(ui.caption_punctuation, config.asr.caption_punctuation);
        set_checked(ui.sentence_punctuation, config.asr.sentence_punctuation);
        set_checked(ui.ai_enabled, config.ai.enabled);
        set_text(ui.ai_endpoint, &config.ai.endpoint);
        set_text(ui.ai_model, &config.ai.model);
        set_text(ui.ai_api_key, &config.ai.api_key);
        set_checked(ui.accept_invalid_certs, config.ai.accept_invalid_certs);
        set_text(ui.status, "保存后会立即应用到下一次识别");
    }

    unsafe fn save_controls(ui: &Ui) -> Result<()> {
        let mut config = ui.settings.get();
        config.asr.caption_punctuation = is_checked(ui.caption_punctuation);
        config.asr.sentence_punctuation = is_checked(ui.sentence_punctuation);
        config.ai.enabled = is_checked(ui.ai_enabled);
        config.ai.endpoint = read_text(ui.ai_endpoint).trim().to_string();
        config.ai.model = read_text(ui.ai_model).trim().to_string();
        config.ai.api_key = read_text(ui.ai_api_key).trim().to_string();
        config.ai.accept_invalid_certs = is_checked(ui.accept_invalid_certs);

        let needs_api_key = config.ai.enabled && config.ai.api_key.trim().is_empty();
        ui.settings.update(config)?;
        let status = if needs_api_key {
            "设置已保存；AI 后处理需要填写 API Key"
        } else {
            "设置已保存"
        };
        set_text(ui.status, status);
        Ok(())
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        const WM_CREATE: u32 = 0x0001;
        const WM_COMMAND: u32 = 0x0111;
        const WM_DESTROY: u32 = 0x0002;
        const WM_PAINT: u32 = 0x000F;

        match msg {
            WM_CREATE => {
                CURRENT_UI.with(|cell| {
                    if let Some(ui) = cell.borrow().as_ref() {
                        load_controls(ui);
                    }
                });
                LRESULT(0)
            }
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let _ = BeginPaint(hwnd, &mut paint);
                let _ = EndPaint(hwnd, &paint);
                LRESULT(0)
            }
            WM_COMMAND => {
                let id = wparam.0 & 0xffff;
                let ui = CURRENT_UI.with(|cell| {
                    let ui = cell.borrow();
                    ui.as_ref().map(|ui| Ui {
                        settings: ui.settings.clone(),
                        ..*ui
                    })
                });
                if let Some(ui) = ui {
                    match id as isize {
                        ID_SAVE => {
                            if let Err(e) = save_controls(&ui) {
                                unsafe { set_text(ui.status, &format!("保存失败：{e}")) };
                                tracing::error!("Failed to save settings: {:?}", e);
                            }
                        }
                        ID_EXIT => {
                            tracing::info!("Exit requested from settings");
                            EXIT_REQUESTED.store(true, Ordering::SeqCst);
                            unsafe {
                                let _ = DestroyWindow(hwnd);
                            }
                        }
                        _ => {}
                    }
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                SETTINGS_OPEN.store(0, Ordering::SeqCst);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    fn run_window(settings: Arc<RuntimeSettings>) -> Result<()> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let class_name = SETTINGS_CLASS;

            let class = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wnd_proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH(CreateSolidBrush(COLORREF(0xf9fafb)).0),
                lpszClassName: class_name,
                ..Default::default()
            };
            RegisterClassExW(&class);

            let width = 520;
            let height = 400;
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class_name,
                SETTINGS_TITLE,
                WS_OVERLAPPEDWINDOW & !WS_MAXIMIZEBOX,
                80,
                80,
                width,
                height,
                HWND::default(),
                HMENU(0),
                instance,
                None,
            );
            if hwnd == HWND::default() {
                return Err(anyhow!("CreateWindowExW failed"));
            }

            let button = w!("BUTTON");
            let edit = w!("EDIT");
            let label = w!("STATIC");

            let caption = create_control(
                hwnd,
                button,
                "字幕加标点（识别中文字）",
                BS_AUTOCHECKBOX,
                24,
                20,
                240,
                26,
                ID_CAPTION_PUNCTUATION,
                true,
            );
            let sentence = create_control(
                hwnd,
                button,
                "整句加标点（最终插入文字）",
                BS_AUTOCHECKBOX,
                280,
                20,
                220,
                26,
                ID_SENTENCE_PUNCTUATION,
                true,
            );
            let ai_enabled = create_control(
                hwnd,
                button,
                "启用 AI 后处理（兼容 OpenAI Chat API）",
                BS_AUTOCHECKBOX,
                24,
                64,
                460,
                26,
                ID_AI_ENABLED,
                true,
            );

            create_control(hwnd, label, "API 地址", 0, 24, 108, 80, 20, 0, false);
            let endpoint = create_control(
                hwnd,
                edit,
                "",
                WS_BORDER.0 as i32 | ES_AUTOHSCROLL,
                112,
                104,
                390,
                26,
                ID_AI_ENDPOINT,
                true,
            );

            create_control(hwnd, label, "模型名称", 0, 24, 148, 80, 20, 0, false);
            let model = create_control(
                hwnd,
                edit,
                "",
                WS_BORDER.0 as i32 | ES_AUTOHSCROLL,
                112,
                144,
                390,
                26,
                ID_AI_MODEL,
                true,
            );

            create_control(hwnd, label, "API Key", 0, 24, 188, 80, 20, 0, false);
            let api_key = create_control(
                hwnd,
                edit,
                "",
                WS_BORDER.0 as i32 | ES_AUTOHSCROLL | ES_PASSWORD,
                112,
                184,
                390,
                26,
                ID_AI_API_KEY,
                true,
            );

            let accept_invalid_certs = create_control(
                hwnd,
                button,
                "允许无效 HTTPS 证书（不推荐）",
                BS_AUTOCHECKBOX,
                24,
                224,
                460,
                26,
                ID_ACCEPT_INVALID_CERTS,
                true,
            );
            let _save = create_control(
                hwnd,
                button,
                "保存设置",
                BS_PUSHBUTTON,
                112,
                262,
                130,
                32,
                ID_SAVE,
                true,
            );
            let _exit = create_control(
                hwnd,
                button,
                "退出本输入法",
                BS_PUSHBUTTON,
                258,
                262,
                130,
                32,
                ID_EXIT,
                true,
            );
            let status = create_control(hwnd, label, "", 0, 112, 308, 390, 24, ID_STATUS, false);

            let ui = Ui {
                settings,
                caption_punctuation: caption,
                sentence_punctuation: sentence,
                ai_enabled,
                ai_endpoint: endpoint,
                ai_model: model,
                ai_api_key: api_key,
                accept_invalid_certs,
                status,
            };
            CURRENT_UI.with(|cell| *cell.borrow_mut() = Some(ui));

            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetFocus(endpoint);

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if IsDialogMessageW(hwnd, &mut msg).as_bool() {
                    continue;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
                if SETTINGS_OPEN.load(Ordering::SeqCst) == 0 {
                    break;
                }
            }

            CURRENT_UI.with(|cell| *cell.borrow_mut() = None);
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
pub use windows_impl::{exit_requested, open};

#[cfg(not(target_os = "windows"))]
pub fn open(_settings: Arc<RuntimeSettings>) {}

#[cfg(not(target_os = "windows"))]
pub fn exit_requested() -> bool {
    false
}
