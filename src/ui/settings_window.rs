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
        BeginPaint, CreateSolidBrush, EndPaint, GetStockObject, SetBkColor, SetTextColor,
        DEFAULT_GUI_FONT, HBRUSH, HDC, PAINTSTRUCT,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows::Win32::UI::WindowsAndMessaging::*;

    use crate::data::RuntimeSettings;

    const SETTINGS_CLASS: PCWSTR = w!("DoubaoVoiceSettings");
    const SETTINGS_TITLE: PCWSTR = w!("标点设置");

    const ID_PUNCTUATION_SMART: isize = 111;
    const ID_PUNCTUATION_SPACE: isize = 112;
    const ID_PUNCTUATION_FULL: isize = 113;
    const ID_SAVE: isize = 108;
    const ID_EXIT: isize = 109;
    const ID_STATUS: isize = 110;

    const COLOR_BACKGROUND: COLORREF = COLORREF(0x00000000);
    const COLOR_CARD: COLORREF = COLORREF(0x00222222);
    const COLOR_TEXT: COLORREF = COLORREF(0x00ffffff);
    #[derive(Clone, Copy)]
    struct Theme {
        background: COLORREF,
        card: COLORREF,
        text: COLORREF,
    }

    impl Theme {
        const DARK: Self = Self {
            background: COLOR_BACKGROUND,
            card: COLOR_CARD,
            text: COLOR_TEXT,
        };

        const LIGHT: Self = Self {
            background: COLORREF(0x00f9fafb),
            card: COLORREF(0x00ffffff),
            text: COLORREF(0x00171717),
        };
    }

    fn current_theme() -> Theme {
        unsafe {
            let mut value = [0u16; 32];
            let mut size = u32::try_from(value.len() * 2).unwrap_or(0);
            let status = RegGetValueW(
                HKEY_CURRENT_USER,
                w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
                w!("AppsUseLightTheme"),
                RRF_RT_REG_SZ,
                None,
                Some(value.as_mut_ptr().cast()),
                Some(&mut size),
            );
            if status.is_ok() {
                let text = String::from_utf16_lossy(&value);
                if text.trim_end_matches(char::from(0)).trim() == "0" {
                    return Theme::DARK;
                }
            }
        }
        Theme::LIGHT
    }

    #[derive(Clone)]
    struct Ui {
        settings: Arc<RuntimeSettings>,
        punctuation_smart: HWND,
        punctuation_space: HWND,
        punctuation_full: HWND,
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
                let existing_hwnd = HWND(existing as isize);
                if IsWindow(existing_hwnd).as_bool() {
                    let _ = SetForegroundWindow(existing_hwnd);
                    return;
                }
            }
            SETTINGS_OPEN.store(0, Ordering::SeqCst);
        }

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
        let mode = if config.asr.caption_punctuation && config.asr.sentence_punctuation {
            config.asr.punctuation_mode
        } else {
            crate::data::PunctuationMode::from_legacy(
                config.asr.caption_punctuation,
                config.asr.sentence_punctuation,
            )
        };
        set_checked(
            ui.punctuation_smart,
            mode == crate::data::PunctuationMode::Smart,
        );
        set_checked(
            ui.punctuation_space,
            mode == crate::data::PunctuationMode::Space,
        );
        set_checked(
            ui.punctuation_full,
            mode == crate::data::PunctuationMode::Full,
        );
        set_text(ui.status, "选择后立即保存");
    }

    unsafe fn save_controls(ui: &Ui) -> Result<()> {
        let mut config = ui.settings.get();
        let mode = if is_checked(ui.punctuation_smart) {
            crate::data::PunctuationMode::Smart
        } else if is_checked(ui.punctuation_space) {
            crate::data::PunctuationMode::Space
        } else {
            crate::data::PunctuationMode::Full
        };
        config.asr.punctuation_mode = mode;
        config.asr.caption_punctuation = mode != crate::data::PunctuationMode::Full;
        config.asr.sentence_punctuation = mode != crate::data::PunctuationMode::Space;
        ui.settings.update(config)?;
        set_text(ui.status, "已保存");
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
        const WM_CTLCOLOREDIT: u32 = 0x0133;
        const WM_CTLCOLORSTATIC: u32 = 0x0138;

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
            WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC => {
                let hdc = HDC(wparam.0 as isize);
                let theme = current_theme();
                SetTextColor(hdc, theme.text);
                SetBkColor(hdc, theme.card);
                let brush = HBRUSH(CreateSolidBrush(theme.card).0);
                LRESULT(brush.0 as isize)
            }
            WM_COMMAND => {
                let id = wparam.0 & 0xffff;
                let ui = CURRENT_UI.with(|cell| {
                    let ui = cell.borrow();
                    ui.as_ref().cloned()
                });
                if let Some(ui) = ui {
                    match id as isize {
                        ID_PUNCTUATION_SMART | ID_PUNCTUATION_SPACE | ID_PUNCTUATION_FULL => unsafe {
                            let id = id as isize;
                            set_checked(ui.punctuation_smart, id == ID_PUNCTUATION_SMART);
                            set_checked(ui.punctuation_space, id == ID_PUNCTUATION_SPACE);
                            set_checked(ui.punctuation_full, id == ID_PUNCTUATION_FULL);
                        },
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
                hbrBackground: HBRUSH(CreateSolidBrush(current_theme().background).0),
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
            let label = w!("STATIC");

            let punctuation_smart = create_control(
                hwnd,
                button,
                "智能添加标点",
                BS_AUTORADIOBUTTON,
                24,
                20,
                472,
                34,
                ID_PUNCTUATION_SMART,
                true,
            );
            let punctuation_space = create_control(
                hwnd,
                button,
                "空格代替标点",
                BS_AUTORADIOBUTTON,
                24,
                64,
                472,
                34,
                ID_PUNCTUATION_SPACE,
                true,
            );
            let punctuation_full = create_control(
                hwnd,
                button,
                "添加完整标点",
                BS_AUTORADIOBUTTON,
                24,
                108,
                472,
                34,
                ID_PUNCTUATION_FULL,
                true,
            );

            let status = create_control(hwnd, label, "", 0, 24, 154, 472, 24, ID_STATUS, false);

            let ui = Ui {
                settings,
                punctuation_smart,
                punctuation_space,
                punctuation_full,
                status,
            };
            CURRENT_UI.with(|cell| *cell.borrow_mut() = Some(ui));
            SETTINGS_OPEN.store(hwnd.0 as usize, Ordering::SeqCst);

            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetFocus(punctuation_smart);

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
