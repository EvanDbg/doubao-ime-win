//! System Tray Demo
//!
//! A simple demo to test system tray functionality with menu items.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIconBuilder,
};

fn main() {
    println!("=== 系统托盘 Demo ===");
    println!("右键点击托盘图标查看菜单");
    println!();

    // Create icon first
    let icon = create_icon();

    // Create tray menu
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

    menu.append(&start_item).unwrap();
    menu.append(&stop_item).unwrap();
    menu.append(&separator1).unwrap();
    menu.append(&settings_item).unwrap();
    menu.append(&separator2).unwrap();
    menu.append(&quit_item).unwrap();

    // Build tray icon - must be on main thread
    let _tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("豆包语音输入 Demo")
        .with_icon(icon)
        .build()
        .expect("Failed to create tray icon");

    println!("托盘图标已创建！");

    // Get menu event receiver
    let menu_rx = MenuEvent::receiver();
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    // Spawn menu event handler thread
    std::thread::spawn(move || {
        while running_clone.load(Ordering::SeqCst) {
            if let Ok(event) = menu_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                if event.id == start_id {
                    println!("✅ 菜单: 开始语音输入");
                } else if event.id == stop_id {
                    println!("⏹️ 菜单: 停止语音输入");
                } else if event.id == settings_id {
                    println!("⚙️ 菜单: 打开设置");
                    // Note: MessageBox should be called from main thread
                } else if event.id == quit_id {
                    println!("👋 菜单: 退出");
                    running_clone.store(false, Ordering::SeqCst);
                    // Post quit message to main thread
                    #[cfg(target_os = "windows")]
                    unsafe {
                        windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
                    }
                }
            }
        }
    });

    // Run Win32 message loop on main thread (required for tray icon)
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, TranslateMessage, MSG,
        };

        println!("运行消息循环...");
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
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    println!("Demo 结束");
}

/// Create a simple tray icon (purple circle with microphone)
fn create_icon() -> tray_icon::Icon {
    let width = 32u32;
    let height = 32u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;
    let outer_radius = (width.min(height) as f32 / 2.0) - 1.0;
    let inner_radius = outer_radius - 3.0;

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= outer_radius {
                if dist <= inner_radius {
                    // Purple gradient
                    let t = dist / inner_radius;
                    let r = (102.0 + (200.0 - 102.0) * t * 0.5) as u8;
                    let g = (126.0 - 50.0 * t) as u8;
                    let b = (234.0 - 30.0 * t) as u8;
                    rgba.push(r);
                    rgba.push(g);
                    rgba.push(b);
                    rgba.push(255);
                } else {
                    // White border
                    rgba.push(255);
                    rgba.push(255);
                    rgba.push(255);
                    rgba.push(200);
                }
            } else {
                rgba.push(0);
                rgba.push(0);
                rgba.push(0);
                rgba.push(0);
            }
        }
    }

    // Draw microphone
    let mic = (255u8, 255u8, 255u8, 255u8);
    for y in 10..18 {
        for x in 13..19 {
            let idx = (y * width + x) as usize * 4;
            if idx + 3 < rgba.len() {
                rgba[idx] = mic.0;
                rgba[idx + 1] = mic.1;
                rgba[idx + 2] = mic.2;
                rgba[idx + 3] = mic.3;
            }
        }
    }
    for y in 18..22 {
        for x in 15..17 {
            let idx = (y * width + x) as usize * 4;
            if idx + 3 < rgba.len() {
                rgba[idx] = mic.0;
                rgba[idx + 1] = mic.1;
                rgba[idx + 2] = mic.2;
                rgba[idx + 3] = mic.3;
            }
        }
    }
    for x in 12..20 {
        let y = 22u32;
        let idx = (y * width + x) as usize * 4;
        if idx + 3 < rgba.len() {
            rgba[idx] = mic.0;
            rgba[idx + 1] = mic.1;
            rgba[idx + 2] = mic.2;
            rgba[idx + 3] = mic.3;
        }
    }

    tray_icon::Icon::from_rgba(rgba, width, height).expect("Failed to create icon")
}
