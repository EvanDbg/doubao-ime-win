//! Tao/Wry desktop shell for the tray, settings window and recording HUD.

use anyhow::{anyhow, Result};
use rust_embed::RustEmbed;
use serde::Serialize;
use std::{
    borrow::Cow,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy},
    platform::windows::{WindowBuilderExtWindows, WindowExtWindows},
    window::{Window, WindowBuilder},
};
use tokio::sync::Mutex;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIconBuilder,
};
use wry::{http::Response, PageLoadEvent, WebContext, WebView, WebViewBuilder};

use crate::{
    audio::AudioCapture,
    business::{
        invoke_official_doubao, HotkeyEvent, HotkeyManager, OfficialDoubaoAction, RawKeyBinding,
        VoiceController,
    },
    cloud::{test_custom_llm, RichChatClient},
    data::{AppConfig, CloudConfig},
};

#[derive(RustEmbed)]
#[folder = "frontend/dist/"]
struct FrontendAssets;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum VoiceState {
    Idle,
    Recording,
    Processing,
}

enum UserEvent {
    Ipc(Box<IpcCommand>),
    Hotkey(HotkeyEvent),
    OfficialDoubao(OfficialDoubaoAction),
    Start,
    Stop,
    SetState(VoiceState),
    CaptureResult(std::result::Result<RawKeyBinding, String>),
    LlmTestResult { success: bool, message: String },
    DragHud,
}

enum IpcCommand {
    GetConfig,
    SaveConfig(Box<AppConfig>),
    CaptureRawKey,
    OpenLogs,
    ShowSettings,
    GetVoiceState,
    StartRecording,
    StopRecording,
    GetSettingsWindowState,
    DragSettings,
    MinimizeSettings,
    ToggleSettingsMaximize,
    HideSettings,
    DragHud,
    TestCustomLlm(Box<CloudConfig>),
}

/// Runs the UI shell on the main thread. Audio and network work remain on Tokio.
pub fn run_app(
    config: AppConfig,
    device_id: String,
    voice_controller: Arc<Mutex<VoiceController>>,
    hotkey_manager: HotkeyManager,
    audio_capture: Arc<AudioCapture>,
) -> Result<()> {
    let event_loop: EventLoop<UserEvent> = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let settings_window = WindowBuilder::new()
        .with_title("豆包语音输入 - 设置")
        .with_decorations(false)
        .with_inner_size(LogicalSize::new(820.0, 420.0))
        .with_min_inner_size(LogicalSize::new(640.0, 420.0))
        .build(&event_loop)?;
    set_settings_immersive_theme(&settings_window);
    let hud_window = WindowBuilder::new()
        .with_title("豆包语音输入")
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top(true)
        .with_skip_taskbar(true)
        .with_inner_size(LogicalSize::new(280.0, 110.0))
        .with_position(tao::dpi::LogicalPosition::new(
            config.floating_button.position_x as f64,
            config.floating_button.position_y as f64,
        ))
        .build(&event_loop)?;
    settings_window.set_visible(false);
    hud_window.set_visible(false);
    set_hud_no_activate(&hud_window);

    let webview_data_dir = webview_data_directory();
    std::fs::create_dir_all(&webview_data_dir).map_err(|error| {
        anyhow!(
            "Unable to create WebView2 data directory {}: {error}",
            webview_data_dir.display()
        )
    })?;
    tracing::info!(
        "Using WebView2 data directory: {}",
        webview_data_dir.display()
    );
    let mut web_context = WebContext::new(Some(webview_data_dir));
    let settings_webview = build_webview(&settings_window, false, proxy.clone(), &mut web_context)?;
    let hud_webview = build_webview(&hud_window, true, proxy.clone(), &mut web_context)?;

    let menu = Menu::new();
    let start_item = MenuItem::new("开始语音输入", true, None);
    let stop_item = MenuItem::new("停止语音输入", true, None);
    let settings_item = MenuItem::new("设置...", true, None);
    let logs_item = MenuItem::new("打开日志文件夹", true, None);
    let quit_item = MenuItem::new("退出", true, None);
    let start_id = start_item.id().clone();
    let stop_id = stop_item.id().clone();
    let settings_id = settings_item.id().clone();
    let logs_id = logs_item.id().clone();
    let quit_id = quit_item.id().clone();
    menu.append(&start_item)?;
    menu.append(&stop_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&settings_item)?;
    menu.append(&logs_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&quit_item)?;
    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("豆包语音输入")
        .with_icon(load_icon()?)
        .build()?;

    let hotkey_proxy = proxy.clone();
    let hotkey_action = hotkey_manager.clone();
    hotkey_manager.on_event(move |event| {
        if hotkey_action.invokes_official_doubao() {
            if let Some(action) = hotkey_action.official_doubao_action(event) {
                let _ = hotkey_proxy.send_event(UserEvent::OfficialDoubao(action));
            }
        } else {
            let _ = hotkey_proxy.send_event(UserEvent::Hotkey(event));
        }
    });

    let settings_id_window = settings_window.id();
    let hud_id_window = hud_window.id();
    let mut state = VoiceState::Idle;
    let mut last_meter_at = Instant::now();
    let runtime = tokio::runtime::Handle::current();
    let menu_rx = MenuEvent::receiver();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(16));
        match event {
            Event::UserEvent(command) => match command {
                UserEvent::Ipc(command) => handle_ipc(*command, &settings_webview, &hud_webview, &settings_window, &proxy, &hotkey_manager, &runtime, voice_controller.clone(), &device_id, &mut state),
                UserEvent::Hotkey(HotkeyEvent::Toggle) => {
                    if matches!(state, VoiceState::Recording) { let _ = proxy.send_event(UserEvent::Stop); } else { let _ = proxy.send_event(UserEvent::Start); }
                }
                UserEvent::Hotkey(HotkeyEvent::Start) => {
                    let _ = proxy.send_event(UserEvent::Start);
                }
                UserEvent::Hotkey(HotkeyEvent::Stop) => {
                    let _ = proxy.send_event(UserEvent::Stop);
                }
                UserEvent::OfficialDoubao(action) => {
                    if let Err(error) = invoke_official_doubao(action) {
                        tracing::error!("Unable to invoke the official Doubao input method: {error:#}");
                    }
                }
                UserEvent::Start => start_recording(voice_controller.clone(), runtime.clone(), proxy.clone()),
                UserEvent::Stop => stop_recording(voice_controller.clone(), runtime.clone(), proxy.clone()),
                UserEvent::SetState(next) => {
                    state = next;
                    send_event(&hud_webview, &serde_json::json!({"type":"voice_state", "state": state}));
                    let hud_enabled = AppConfig::load_or_default().map(|config| config.floating_button.enabled).unwrap_or(false);
                    hud_window.set_visible(!matches!(state, VoiceState::Idle) && hud_enabled);
                }
                UserEvent::CaptureResult(result) => match result { Ok(binding) => send_event(&settings_webview, &serde_json::json!({"type":"capture_result", "message":format!("已录入：VK {} / 扫描码 {}", binding.vk_code, binding.scan_code), "binding":{"vk_code":binding.vk_code,"scan_code":binding.scan_code,"extended":binding.extended}})), Err(error) => send_event(&settings_webview, &serde_json::json!({"type":"capture_result", "message":format!("录入失败：{error}")})) },
                UserEvent::LlmTestResult { success, message } => send_event(&settings_webview, &serde_json::json!({"type":"llm_test_result", "success":success, "message":message})),
                UserEvent::DragHud => { let _ = hud_window.drag_window(); }
            },
            Event::MainEventsCleared => {
                while let Ok(menu_event) = menu_rx.try_recv() {
                    if menu_event.id == start_id { let _ = proxy.send_event(UserEvent::Start); }
                    else if menu_event.id == stop_id { let _ = proxy.send_event(UserEvent::Stop); }
                    else if menu_event.id == settings_id {
                        settings_window.set_minimized(false);
                        settings_window.set_visible(true);
                    }
                    else if menu_event.id == logs_id {
                        if let Err(error) = open_logs_directory() {
                            tracing::error!("Unable to open log directory: {error:#}");
                        }
                    }
                    else if menu_event.id == quit_id { *control_flow = ControlFlow::Exit; }
                }
                if matches!(state, VoiceState::Recording) && last_meter_at.elapsed() >= Duration::from_millis(33) {
                    last_meter_at = Instant::now();
                    send_event(&hud_webview, &serde_json::json!({"type":"meter", "value":audio_capture.meter_level()}));
                }
            }
            Event::WindowEvent { window_id, event: WindowEvent::Moved(position), .. } if window_id == hud_id_window => {
                if let Ok(mut current) = AppConfig::load_or_default() { current.floating_button.position_x = position.x; current.floating_button.position_y = position.y; let _ = current.save(); }
            }
            Event::WindowEvent { window_id, event: WindowEvent::Resized(_), .. } if window_id == settings_id_window => {
                send_settings_window_state(&settings_webview, &settings_window);
            }
            Event::WindowEvent { window_id, event: WindowEvent::CloseRequested, .. } if window_id == settings_id_window => settings_window.set_visible(false),
            Event::WindowEvent { window_id, event: WindowEvent::CloseRequested, .. } if window_id == hud_id_window => { let _ = proxy.send_event(UserEvent::Stop); },
            _ => {}
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn handle_ipc(
    command: IpcCommand,
    settings: &WebView,
    hud: &WebView,
    settings_window: &Window,
    proxy: &EventLoopProxy<UserEvent>,
    hotkeys: &HotkeyManager,
    runtime: &tokio::runtime::Handle,
    controller: Arc<Mutex<VoiceController>>,
    device_id: &str,
    state: &mut VoiceState,
) {
    match command {
        IpcCommand::GetConfig => match AppConfig::load_or_default().and_then(|mut config| {
            config.general.auto_start = crate::startup::is_enabled()?;
            Ok(config)
        }) {
            Ok(config) => send_event(
                settings,
                &serde_json::json!({"type":"config", "config":config}),
            ),
            Err(error) => send_error(settings, error),
        },
        IpcCommand::SaveConfig(config) => {
            let rich_chat_client = if config.cloud.auto_polish_enabled {
                match RichChatClient::new(device_id.to_owned(), &config.cloud) {
                    Ok(client) => Some(Arc::new(client)),
                    Err(error) => {
                        send_error(settings, error);
                        return;
                    }
                }
            } else {
                None
            };
            if let Err(error) = hotkeys
                .reconfigure(&config.hotkey)
                .and_then(|_| crate::startup::set_enabled(config.general.auto_start))
                .and_then(|_| config.save())
            {
                send_error(settings, error);
            } else {
                let controller = controller.clone();
                runtime.spawn(async move {
                    controller
                        .lock()
                        .await
                        .reconfigure_rich_chat(rich_chat_client);
                });
                send_event(
                    settings,
                    &serde_json::json!({"type":"config", "config":config}),
                );
            }
        }
        IpcCommand::CaptureRawKey => {
            let proxy = proxy.clone();
            let hotkeys = hotkeys.clone();
            std::thread::spawn(move || {
                let result = hotkeys
                    .capture_raw_key(Duration::from_secs(10))
                    .map_err(|error| error.to_string());
                let _ = proxy.send_event(UserEvent::CaptureResult(result));
            });
        }
        IpcCommand::OpenLogs => {
            if let Err(error) = open_logs_directory() {
                send_error(settings, error);
            }
        }
        IpcCommand::ShowSettings => {
            settings_window.set_minimized(false);
            settings_window.set_visible(true);
        }
        IpcCommand::GetVoiceState => send_event(
            hud,
            &serde_json::json!({"type":"voice_state", "state":state}),
        ),
        IpcCommand::StartRecording => start_recording(controller, runtime.clone(), proxy.clone()),
        IpcCommand::StopRecording => stop_recording(controller, runtime.clone(), proxy.clone()),
        IpcCommand::GetSettingsWindowState => {
            send_settings_window_state(settings, settings_window);
        }
        IpcCommand::DragSettings => {
            let _ = settings_window.drag_window();
        }
        IpcCommand::MinimizeSettings => {
            settings_window.set_minimized(true);
        }
        IpcCommand::ToggleSettingsMaximize => {
            settings_window.set_maximized(!settings_window.is_maximized());
            send_settings_window_state(settings, settings_window);
        }
        IpcCommand::HideSettings => {
            settings_window.set_visible(false);
        }
        IpcCommand::DragHud => {
            let _ = proxy.send_event(UserEvent::DragHud);
        }
        IpcCommand::TestCustomLlm(config) => {
            let proxy = proxy.clone();
            runtime.spawn(async move {
                let result = test_custom_llm(&config).await;
                let _ = proxy.send_event(UserEvent::LlmTestResult {
                    success: result.is_success(),
                    message: result.message(),
                });
            });
        }
    }
}

fn start_recording(
    controller: Arc<Mutex<VoiceController>>,
    runtime: tokio::runtime::Handle,
    proxy: EventLoopProxy<UserEvent>,
) {
    runtime.spawn(async move {
        let mut controller = controller.lock().await;
        match controller.start().await {
            Ok(()) => {
                let _ = proxy.send_event(UserEvent::SetState(VoiceState::Recording));
            }
            Err(error) => tracing::error!("Unable to start voice input: {error:#}"),
        }
    });
}

fn stop_recording(
    controller: Arc<Mutex<VoiceController>>,
    runtime: tokio::runtime::Handle,
    proxy: EventLoopProxy<UserEvent>,
) {
    let _ = proxy.send_event(UserEvent::SetState(VoiceState::Processing));
    runtime.spawn(async move {
        let mut controller = controller.lock().await;
        if let Err(error) = controller.stop().await {
            tracing::error!("Unable to stop voice input: {error:#}");
        }
        let _ = proxy.send_event(UserEvent::SetState(VoiceState::Idle));
    });
}

fn webview_data_directory() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("DoubaoVoiceInput")
        .join("WebView2")
}

fn build_webview(
    window: &Window,
    hud: bool,
    proxy: EventLoopProxy<UserEvent>,
    web_context: &mut WebContext,
) -> Result<WebView> {
    let version = env!("CARGO_PKG_VERSION");
    let url = if hud {
        format!("doubao://localhost/index.html?view=hud&v={version}")
    } else {
        format!("doubao://localhost/index.html?v={version}")
    };
    let view_name = if hud { "HUD" } else { "settings" };
    let builder = WebViewBuilder::new_with_web_context(web_context)
        .with_transparent(hud)
        .with_custom_protocol("doubao".into(), |_, request| {
            asset_response(request.uri().path())
        })
        .with_on_page_load_handler(move |event, url| match event {
            PageLoadEvent::Started => tracing::debug!("{view_name} page loading: {url}"),
            PageLoadEvent::Finished => tracing::info!("{view_name} page loaded: {url}"),
        })
        .with_ipc_handler(move |request| match parse_ipc(request.body()) {
            Ok(command) => {
                let _ = proxy.send_event(UserEvent::Ipc(Box::new(command)));
            }
            Err(error) => tracing::warn!("Ignoring invalid UI IPC request: {error:#}"),
        })
        .with_url(url);
    builder.build(window).map_err(Into::into)
}

fn parse_ipc(message: &str) -> Result<IpcCommand> {
    let value: serde_json::Value = serde_json::from_str(message)?;
    match value.get("command").and_then(serde_json::Value::as_str) {
        Some("get_config") => Ok(IpcCommand::GetConfig),
        Some("capture_raw_key") => Ok(IpcCommand::CaptureRawKey),
        Some("open_logs") => Ok(IpcCommand::OpenLogs),
        Some("show_settings") => Ok(IpcCommand::ShowSettings),
        Some("get_voice_state") => Ok(IpcCommand::GetVoiceState),
        Some("start_recording") => Ok(IpcCommand::StartRecording),
        Some("stop_recording") => Ok(IpcCommand::StopRecording),
        Some("get_settings_window_state") => Ok(IpcCommand::GetSettingsWindowState),
        Some("drag_settings") => Ok(IpcCommand::DragSettings),
        Some("minimize_settings") => Ok(IpcCommand::MinimizeSettings),
        Some("toggle_settings_maximize") => Ok(IpcCommand::ToggleSettingsMaximize),
        Some("hide_settings") => Ok(IpcCommand::HideSettings),
        Some("drag_hud") => Ok(IpcCommand::DragHud),
        Some("test_custom_llm") => Ok(IpcCommand::TestCustomLlm(Box::new(serde_json::from_value(
            value
                .get("params")
                .and_then(|params| params.get("config"))
                .cloned()
                .ok_or_else(|| anyhow!("test_custom_llm requires params.config"))?,
        )?))),
        Some("save_config") => Ok(IpcCommand::SaveConfig(Box::new(serde_json::from_value(
            value
                .get("params")
                .and_then(|params| params.get("config"))
                .cloned()
                .ok_or_else(|| anyhow!("save_config requires params.config"))?,
        )?))),
        _ => Err(anyhow!("unknown IPC command")),
    }
}

fn logs_directory() -> PathBuf {
    AppConfig::config_path()
        .parent()
        .map(|parent| parent.join("logs"))
        .unwrap_or_else(|| PathBuf::from("logs"))
}

#[cfg(target_os = "windows")]
fn open_logs_directory() -> Result<()> {
    let directory = logs_directory();
    std::fs::create_dir_all(&directory)?;
    std::process::Command::new("explorer.exe")
        .arg(&directory)
        .spawn()
        .map_err(|error| anyhow!("无法打开日志文件夹 {}：{error}", directory.display()))?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn open_logs_directory() -> Result<()> {
    let directory = logs_directory();
    std::fs::create_dir_all(&directory)?;
    std::process::Command::new("xdg-open")
        .arg(&directory)
        .spawn()
        .map_err(|error| {
            anyhow!(
                "Unable to open log directory {}: {error}",
                directory.display()
            )
        })?;
    Ok(())
}

fn asset_response(path: &str) -> Response<Cow<'static, [u8]>> {
    let path = match path.trim_start_matches('/') {
        "" => "index.html",
        path => path,
    };
    match FrontendAssets::get(path) {
        Some(asset) => Response::builder()
            .header(
                "content-type",
                mime_guess::from_path(path).first_or_octet_stream().as_ref(),
            )
            .header("cache-control", "no-store")
            .body(Cow::Owned(asset.data.into_owned()))
            .unwrap(),
        None => Response::builder()
            .status(404)
            .body(Cow::Owned(Vec::new()))
            .unwrap(),
    }
}

fn send_event(webview: &WebView, value: &serde_json::Value) {
    let payload = value.to_string().replace('<', "\\u003c");
    let _ = webview.evaluate_script(&format!(
        "window.__doubaoEvent && window.__doubaoEvent({payload});"
    ));
}
fn send_error(webview: &WebView, error: impl std::fmt::Display) {
    send_event(
        webview,
        &serde_json::json!({"type":"error", "message":error.to_string()}),
    );
}

fn send_settings_window_state(webview: &WebView, window: &Window) {
    send_event(
        webview,
        &serde_json::json!({"type":"window_state", "maximized":window.is_maximized()}),
    );
}

#[cfg(target_os = "windows")]
fn set_hud_no_activate(window: &Window) {
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{
            GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        },
    };
    unsafe {
        let hwnd = HWND(window.hwnd() as *mut _);
        let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(
            hwnd,
            GWL_EXSTYLE,
            style | WS_EX_NOACTIVATE.0 as i32 | WS_EX_TOOLWINDOW.0 as i32,
        );
    }
}

#[cfg(target_os = "windows")]
fn set_settings_immersive_theme(window: &Window) {
    use std::{ffi::c_void, mem::size_of};
    use windows::Win32::{
        Foundation::HWND,
        Graphics::Dwm::{
            DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
        },
    };

    unsafe fn set_attribute<T: Copy>(
        hwnd: HWND,
        attribute: windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE,
        value: &T,
    ) {
        let _ = DwmSetWindowAttribute(
            hwnd,
            attribute,
            value as *const T as *const c_void,
            size_of::<T>() as u32,
        );
    }

    unsafe {
        let hwnd = HWND(window.hwnd() as *mut _);
        let dark_mode: i32 = 1;
        // COLORREF values use 0x00BBGGRR byte order.
        let caption_color: u32 = 0x001A_1617; // #17161a
        let border_color: u32 = 0x003E_3539; // #39353e
        let text_color: u32 = 0x00ED_E5E9; // #e9e5ed
        set_attribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark_mode);
        set_attribute(hwnd, DWMWA_CAPTION_COLOR, &caption_color);
        set_attribute(hwnd, DWMWA_BORDER_COLOR, &border_color);
        set_attribute(hwnd, DWMWA_TEXT_COLOR, &text_color);
    }
}

#[cfg(not(target_os = "windows"))]
fn set_settings_immersive_theme(_window: &Window) {}

#[cfg(not(target_os = "windows"))]
fn set_hud_no_activate(_window: &Window) {}

fn load_icon() -> Result<tray_icon::Icon> {
    let mut rgba = vec![0; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let dx = x as i32 - 16;
            let dy = y as i32 - 16;
            if dx * dx + dy * dy < 220 {
                let index = (y * 32 + x) * 4;
                rgba[index..index + 4].copy_from_slice(&[124, 58, 237, 255]);
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, 32, 32).map_err(Into::into)
}
