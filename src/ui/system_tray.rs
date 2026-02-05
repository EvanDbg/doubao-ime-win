//! System Tray
//!
//! Implements the system tray icon and menu with proper Windows message loop.

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIconBuilder,
};

use crate::business::{HotkeyManager, VoiceController};
use crate::data::AppConfig;
use crate::ui::{ButtonState, FloatingButton, FloatingButtonConfig, FloatingButtonEvent};

/// Run the application with system tray and floating button
pub async fn run_app(
    config: AppConfig,
    voice_controller: Arc<Mutex<VoiceController>>,
    _hotkey_manager: HotkeyManager,
) -> Result<()> {
    // Create floating button
    let mut floating_button = FloatingButton::new();
    let button_state_setter = floating_button.state_setter();
    let floating_rx = floating_button.take_event_receiver();

    // Configure floating button position from config
    let fb_config = FloatingButtonConfig {
        initial_x: config.floating_button.position_x,
        initial_y: config.floating_button.position_y,
        size: 56,
    };

    // Spawn floating button thread if enabled
    if config.floating_button.enabled {
        std::thread::spawn(move || {
            floating_button.run(fb_config);
        });
    }

    // Create tray icon on main thread
    let icon = load_icon()?;
    let menu = Menu::new();

    let start_item = MenuItem::new("开始语音输入", true, None);
    let stop_item = MenuItem::new("停止语音输入", true, None);
    let separator1 = PredefinedMenuItem::separator();
    let settings_item = MenuItem::new("设置...", true, None);
    let separator2 = PredefinedMenuItem::separator();
    let quit_item = MenuItem::new("退出", true, None);

    let start_id = start_item.id().clone();
    let stop_id = stop_item.id().clone();
    let settings_id = settings_item.id().clone();
    let quit_id = quit_item.id().clone();

    menu.append(&start_item)?;
    menu.append(&stop_item)?;
    menu.append(&separator1)?;
    menu.append(&settings_item)?;
    menu.append(&separator2)?;
    menu.append(&quit_item)?;

    let _tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("豆包语音输入 - 双击Ctrl开始/停止")
        .with_icon(icon)
        .build()?;

    tracing::info!("System tray initialized");

    // Running flag
    let running = Arc::new(AtomicBool::new(true));

    // Get menu and floating button receivers
    let menu_rx = MenuEvent::receiver();

    // Get tokio runtime handle for async operations
    let runtime_handle = tokio::runtime::Handle::current();

    // Set up hotkey callback with state sync
    let vc_for_hotkey = voice_controller.clone();
    let state_for_hotkey = button_state_setter.clone();
    let handle_for_hotkey = runtime_handle.clone();
    _hotkey_manager.on_trigger(move || {
        let vc = vc_for_hotkey.clone();
        let setter = state_for_hotkey.clone();
        let handle = handle_for_hotkey.clone();
        handle.spawn(async move {
            let mut controller = vc.lock().await;
            if controller.is_recording() {
                tracing::info!("Hotkey: stopping voice input");
                setter.set_state(ButtonState::Processing);
                if let Err(e) = controller.stop().await {
                    tracing::error!("Failed to stop voice input: {}", e);
                }
                setter.set_state(ButtonState::Idle);
            } else {
                tracing::info!("Hotkey: starting voice input");
                if let Err(e) = controller.start().await {
                    tracing::error!("Failed to start voice input: {}", e);
                } else {
                    setter.set_state(ButtonState::Recording);
                }
            }
        });
    });

    // Spawn event handler thread for menu and floating button events
    let running_clone = running.clone();
    let vc_clone = voice_controller.clone();
    let state_setter_clone = button_state_setter.clone();

    std::thread::spawn(move || {
        while running_clone.load(Ordering::SeqCst) {
            // Check menu events
            if let Ok(event) = menu_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                if event.id == start_id {
                    let vc = vc_clone.clone();
                    let setter = state_setter_clone.clone();
                    runtime_handle.spawn(async move {
                        let mut controller = vc.lock().await;
                        if !controller.is_recording() {
                            tracing::info!("Starting from menu");
                            if let Err(e) = controller.start().await {
                                tracing::error!("Failed to start: {}", e);
                            } else {
                                setter.set_state(ButtonState::Recording);
                            }
                        }
                    });
                } else if event.id == stop_id {
                    let vc = vc_clone.clone();
                    let setter = state_setter_clone.clone();
                    runtime_handle.spawn(async move {
                        let mut controller = vc.lock().await;
                        if controller.is_recording() {
                            tracing::info!("Stopping from menu");
                            setter.set_state(ButtonState::Processing);
                            if let Err(e) = controller.stop().await {
                                tracing::error!("Failed to stop: {}", e);
                            }
                            setter.set_state(ButtonState::Idle);
                        }
                    });
                } else if event.id == settings_id {
                    tracing::info!("Settings from menu");
                    #[cfg(target_os = "windows")]
                    {
                        use windows::core::w;
                        use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_OK, MB_ICONINFORMATION};
                        unsafe {
                            MessageBoxW(
                                None,
                                w!("豆包语音输入 设置\n\n快捷键: 双击 Ctrl 开始/停止录音\n悬浮按钮: 点击切换录音状态\n\n配置文件: config.toml"),
                                w!("设置"),
                                MB_OK | MB_ICONINFORMATION,
                            );
                        }
                    }
                } else if event.id == quit_id {
                    tracing::info!("Quit from menu");
                    running_clone.store(false, Ordering::SeqCst);
                    #[cfg(target_os = "windows")]
                    unsafe {
                        windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
                    }
                }
            }

            // Check floating button events
            if let Some(ref rx) = floating_rx {
                if let Ok(event) = rx.try_recv() {
                    match event {
                        FloatingButtonEvent::ToggleRecording => {
                            let vc = vc_clone.clone();
                            let setter = state_setter_clone.clone();
                            runtime_handle.spawn(async move {
                                let mut controller = vc.lock().await;
                                if controller.is_recording() {
                                    tracing::info!("Toggle: stopping");
                                    setter.set_state(ButtonState::Processing);
                                    if let Err(e) = controller.stop().await {
                                        tracing::error!("Failed to stop: {}", e);
                                    }
                                    setter.set_state(ButtonState::Idle);
                                } else {
                                    tracing::info!("Toggle: starting");
                                    if let Err(e) = controller.start().await {
                                        tracing::error!("Failed to start: {}", e);
                                    } else {
                                        setter.set_state(ButtonState::Recording);
                                    }
                                }
                            });
                        }
                        FloatingButtonEvent::Exit => {
                            tracing::info!("Exit from floating button");
                            running_clone.store(false, Ordering::SeqCst);
                            #[cfg(target_os = "windows")]
                            unsafe {
                                windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
                            }
                        }
                    }
                }
            }
        }
    });

    // Run Win32 message loop on main thread (REQUIRED for tray icon to work)
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, TranslateMessage, MSG,
        };

        tracing::info!("Running Win32 message loop on main thread");
        let mut msg = MSG::default();
        unsafe {
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);

                if !running.load(Ordering::SeqCst) {
                    break;
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        while running.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    tracing::info!("Application exiting");
    Ok(())
}

/// Load the tray icon with modern appearance
fn load_icon() -> Result<tray_icon::Icon> {
    let width = 32u32;
    let height = 32u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;
    let radius = (width.min(height) as f32 / 2.0) - 1.0;

    // Modern gradient colors (purple to blue)
    let color_start = (139u8, 92u8, 246u8);  // Purple
    let color_end = (59u8, 130u8, 246u8);    // Blue

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= radius {
                // Gradient based on position (top-left to bottom-right)
                let gradient_t = ((x as f32 / width as f32) + (y as f32 / height as f32)) / 2.0;
                let r = (color_start.0 as f32 * (1.0 - gradient_t) + color_end.0 as f32 * gradient_t) as u8;
                let g = (color_start.1 as f32 * (1.0 - gradient_t) + color_end.1 as f32 * gradient_t) as u8;
                let b = (color_start.2 as f32 * (1.0 - gradient_t) + color_end.2 as f32 * gradient_t) as u8;

                // Soft edge anti-aliasing
                let alpha = if dist > radius - 1.5 {
                    ((radius - dist + 1.5) / 1.5 * 255.0) as u8
                } else {
                    255
                };

                rgba.push(r);
                rgba.push(g);
                rgba.push(b);
                rgba.push(alpha);
            } else {
                rgba.push(0);
                rgba.push(0);
                rgba.push(0);
                rgba.push(0);
            }
        }
    }

    // Draw modern microphone icon (white, clean design)
    let mic_color = (255u8, 255u8, 255u8, 255u8);
    let cx = center_x as i32;
    let cy = center_y as i32;

    // Mic head (rounded rectangle)
    for dy in -5..=3 {
        for dx in -3..=3 {
            let in_corner = (dy == -5 || dy == 3) && (dx == -3 || dx == 3);
            if !in_corner {
                let idx = ((cy + dy) as u32 * width + (cx + dx) as u32) as usize * 4;
                if idx + 3 < rgba.len() {
                    rgba[idx] = mic_color.0;
                    rgba[idx + 1] = mic_color.1;
                    rgba[idx + 2] = mic_color.2;
                    rgba[idx + 3] = mic_color.3;
                }
            }
        }
    }

    // Mic holder arc (U shape)
    for dx in -5..=5 {
        let idx = ((cy + 6) as u32 * width + (cx + dx) as u32) as usize * 4;
        if idx + 3 < rgba.len() {
            rgba[idx] = mic_color.0;
            rgba[idx + 1] = mic_color.1;
            rgba[idx + 2] = mic_color.2;
            rgba[idx + 3] = mic_color.3;
        }
    }
    for dy in 3..=6 {
        for dx in [-5, 5] {
            let idx = ((cy + dy) as u32 * width + (cx + dx) as u32) as usize * 4;
            if idx + 3 < rgba.len() {
                rgba[idx] = mic_color.0;
                rgba[idx + 1] = mic_color.1;
                rgba[idx + 2] = mic_color.2;
                rgba[idx + 3] = mic_color.3;
            }
        }
    }

    // Mic stand
    for dy in 7..=10 {
        let idx = ((cy + dy) as u32 * width + cx as u32) as usize * 4;
        if idx + 3 < rgba.len() {
            rgba[idx] = mic_color.0;
            rgba[idx + 1] = mic_color.1;
            rgba[idx + 2] = mic_color.2;
            rgba[idx + 3] = mic_color.3;
        }
    }

    // Mic base
    for dx in -3..=3 {
        let idx = ((cy + 10) as u32 * width + (cx + dx) as u32) as usize * 4;
        if idx + 3 < rgba.len() {
            rgba[idx] = mic_color.0;
            rgba[idx + 1] = mic_color.1;
            rgba[idx + 2] = mic_color.2;
            rgba[idx + 3] = mic_color.3;
        }
    }

    let icon = tray_icon::Icon::from_rgba(rgba, width, height)?;
    Ok(icon)
}
