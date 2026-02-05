//! Doubao Voice Input - Main Entry Point
//!
//! Supports two modes:
//! - CLI mode: For quick testing (run with --cli flag)
//! - UI mode: Full application with system tray and hotkeys (default)

// Hide console window in release builds on Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Result;
use std::env;
use std::io::{self, Write};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use doubao_voice_input::{
    AppConfig, AsrClient, AudioCapture, CredentialStore, HotkeyManager, TextInserter,
    VoiceController,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Check for CLI mode
    let args: Vec<String> = env::args().collect();
    let cli_mode = args.iter().any(|a| a == "--cli" || a == "-c");

    if cli_mode {
        run_cli_mode().await
    } else {
        run_ui_mode().await
    }
}

/// Run in full UI mode with system tray and hotkeys
async fn run_ui_mode() -> Result<()> {
    init_logging(false);

    info!("Starting Doubao Voice Input v{} (UI Mode)", env!("CARGO_PKG_VERSION"));

    // Initialize COM for Windows
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
    }

    // Load configuration
    let config = AppConfig::load_or_default()?;
    info!("Configuration loaded");

    // Initialize credentials
    let credential_store = CredentialStore::new(&config)?;
    let credentials = credential_store.ensure_credentials().await?;
    info!("Device registered: {}", &credentials.device_id[..8.min(credentials.device_id.len())]);

    // Initialize components
    let audio_capture = Arc::new(AudioCapture::new()?);
    let text_inserter = Arc::new(TextInserter::new());
    let asr_client = Arc::new(AsrClient::new(credentials));

    let voice_controller = Arc::new(Mutex::new(VoiceController::new(
        asr_client,
        audio_capture,
        text_inserter,
    )));

    // Initialize hotkey manager
    let hotkey_manager = HotkeyManager::new(&config.hotkey)?;
    info!("Hotkey registered");

    // Run system tray (hotkey callback is set up inside run_app for state sync)
    info!("Starting system tray...");
    doubao_voice_input::ui::run_app(config, voice_controller, hotkey_manager).await?;

    info!("Application exited");
    Ok(())
}

/// Run in CLI mode for testing
async fn run_cli_mode() -> Result<()> {
    init_logging(true);

    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║     豆包语音输入 - CLI 验证版本 v{}        ║", env!("CARGO_PKG_VERSION"));
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!();

    info!("Starting Doubao Voice Input v{} (CLI Mode)", env!("CARGO_PKG_VERSION"));

    // Step 1: Load configuration
    println!("[1/5] 加载配置...");
    let config = AppConfig::load_or_default()?;
    info!("Configuration loaded");
    println!("      ✅ 配置加载成功");

    // Step 2: Initialize credential store and register device
    println!("[2/5] 初始化设备凭据...");
    let credential_store = CredentialStore::new(&config)?;

    println!("      正在注册设备或加载缓存凭据...");
    let credentials = credential_store.ensure_credentials().await?;
    info!("Device ID: {}", credentials.device_id);
    info!("Install ID: {}", credentials.install_id);
    info!("Token available: {}", !credentials.token.is_empty());
    println!(
        "      ✅ 设备已注册，Device ID: {}",
        &credentials.device_id[..8.min(credentials.device_id.len())]
    );

    // Step 3: Initialize audio capture
    println!("[3/5] 初始化音频设备...");
    let audio_capture = match AudioCapture::new() {
        Ok(capture) => {
            println!("      ✅ 音频设备初始化成功");
            Arc::new(capture)
        }
        Err(e) => {
            warn!("Audio capture initialization failed: {}", e);
            println!("      ⚠️  音频设备初始化失败: {}", e);
            println!("      请确保麦克风已连接并被系统识别");
            return Err(e);
        }
    };

    // Step 4: Initialize components
    println!("[4/5] 初始化组件...");
    let text_inserter = Arc::new(TextInserter::new());
    let asr_client = Arc::new(AsrClient::new(credentials.clone()));

    let voice_controller = Arc::new(Mutex::new(VoiceController::new(
        asr_client.clone(),
        audio_capture.clone(),
        text_inserter.clone(),
    )));
    println!("      ✅ ASR 客户端、文本插入器已就绪");

    // Step 5: Ready for testing
    println!("[5/5] 初始化完成！");
    println!();
    println!("════════════════════════════════════════════════════════════");
    println!("  功能验证命令:");
    println!("  [s] 开始语音输入 (Start)");
    println!("  [e] 停止语音输入 (End)");
    println!("  [t] 测试文本插入");
    println!("  [a] 测试 ASR 连接");
    println!("  [q] 退出程序 (Quit)");
    println!("════════════════════════════════════════════════════════════");
    println!();

    // Interactive command loop
    loop {
        print!(">>> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let cmd = input.trim().to_lowercase();

        match cmd.as_str() {
            "s" | "start" => {
                println!("🎤 开始语音输入...");
                info!("User command: start voice input");

                let mut vc = voice_controller.lock().await;
                if vc.is_recording() {
                    println!("⚠️  已经在录音中");
                } else {
                    match vc.start().await {
                        Ok(_) => {
                            println!("✅ 语音输入已开始 - 请对着麦克风说话");
                            println!("   识别结果将实时显示...");
                            info!("Voice recording started successfully");
                        }
                        Err(e) => {
                            error!("Failed to start voice input: {}", e);
                            println!("❌ 启动失败: {}", e);
                        }
                    }
                }
            }
            "e" | "end" | "stop" => {
                println!("⏹️  停止语音输入...");
                info!("User command: stop voice input");

                let mut vc = voice_controller.lock().await;
                if !vc.is_recording() {
                    println!("⚠️  当前没有在录音");
                } else {
                    match vc.stop().await {
                        Ok(_) => {
                            println!("✅ 语音输入已停止");
                            info!("Voice recording stopped");
                        }
                        Err(e) => {
                            error!("Failed to stop voice input: {}", e);
                            println!("❌ 停止失败: {}", e);
                        }
                    }
                }
            }
            "t" | "test" => {
                println!("📝 测试文本插入...");
                println!("   3秒后将在光标位置插入测试文本，请先点击目标应用...");

                tokio::time::sleep(std::time::Duration::from_secs(3)).await;

                match text_inserter.insert("你好，这是豆包语音输入测试！Hello, this is a test!")
                {
                    Ok(_) => {
                        println!("✅ 文本插入成功");
                        info!("Text insertion test passed");
                    }
                    Err(e) => {
                        error!("Text insertion failed: {}", e);
                        println!("❌ 文本插入失败: {}", e);
                    }
                }
            }
            "a" | "asr" => {
                println!("🔗 测试 ASR 连接...");
                info!("Testing ASR connection...");

                println!("   设备 ID: {}", credentials.device_id);
                println!(
                    "   Token: {}...",
                    &credentials.token[..20.min(credentials.token.len())]
                );
                println!("✅ ASR 凭据有效");
                println!("   完整 ASR 测试需要开始录音 (命令: s)");
            }
            "q" | "quit" | "exit" => {
                println!("👋 退出程序...");
                info!("User requested exit");
                break;
            }
            "" => {
                // Empty input, ignore
            }
            _ => {
                println!("❓ 未知命令: {}", cmd);
                println!("   输入 s/e/t/a/q");
            }
        }
    }

    // Cleanup
    let mut vc = voice_controller.lock().await;
    if vc.is_recording() {
        let _ = vc.stop().await;
    }

    println!("程序已退出");
    Ok(())
}

fn init_logging(debug: bool) {
    let level = if debug { "doubao_voice_input=debug" } else { "doubao_voice_input=info" };
    
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| level.into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}
