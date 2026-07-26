//! Doubao Voice Input - Main Entry Point
//!
//! Supports two modes:
//! - CLI mode: For quick testing (run with --cli flag)
//! - UI mode: Full application with system tray and hotkeys (default)

// Hide console window in release builds on Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Result;
use chrono::Local;
use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt::MakeWriter, layer::SubscriberExt, util::SubscriberInitExt};

use doubao_voice_input::{
    init_crypto_provider, AppConfig, AsrClient, AudioCapture, CredentialStore, HotkeyConfig,
    HotkeyManager, NerClient, NerLexicon, RichChatClient, TextInserter, VoiceController,
    VoiceSessionStore,
};

#[tokio::main]
async fn main() {
    init_crypto_provider();

    // Check for CLI mode
    let args: Vec<String> = env::args().collect();
    let cli_mode = args.iter().any(|a| a == "--cli" || a == "-c");

    let result = if cli_mode {
        run_cli_mode().await
    } else {
        run_ui_mode().await
    };

    if let Err(error) = result {
        eprintln!("Application failed: {error:#}");
        process::exit(1);
    }
}

/// Run in full UI mode with system tray and hotkeys
async fn run_ui_mode() -> Result<()> {
    let startup_logs = init_logging(false);
    let result = run_ui_mode_inner().await;

    if let Err(error) = &result {
        report_ui_mode_error(error, &startup_logs);
    }

    result
}

async fn run_ui_mode_inner() -> Result<()> {
    info!(
        "Starting Doubao Voice Input v{} (UI Mode)",
        env!("CARGO_PKG_VERSION")
    );

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
    info!(
        "Device registered: {}",
        &credentials.device_id[..8.min(credentials.device_id.len())]
    );

    // Initialize components
    let audio_capture = Arc::new(AudioCapture::new()?);
    let text_inserter = Arc::new(TextInserter::new());
    let did = credentials.device_id.clone();
    let asr_client = Arc::new(AsrClient::new(credentials));
    let ner_client = Arc::new(NerClient::new(did.clone())?);
    let rich_chat_client = config
        .cloud
        .auto_polish_enabled
        .then(|| RichChatClient::new(did.clone(), &config.cloud))
        .transpose()?
        .map(Arc::new);
    let ner_lexicon = Arc::new(StdMutex::new(NerLexicon::new()));
    let voice_sessions = Arc::new(VoiceSessionStore::new());

    if config.cloud.ner_enabled {
        let client = ner_client.clone();
        tokio::spawn(async move {
            if let Err(error) = client.prefetch_token().await {
                tracing::debug!("Unable to prefetch the NER token: {}", error);
            }
        });
    }

    let voice_controller = Arc::new(Mutex::new(
        VoiceController::new(asr_client, audio_capture.clone(), text_inserter.clone()).with_cloud(
            ner_client,
            ner_lexicon,
            rich_chat_client,
            voice_sessions.clone(),
        ),
    ));

    // Initialize hotkey manager. A binding that fails to parse or register
    // (for example because another program grabbed the shortcut) must not
    // prevent startup: fall back to the default hotkey and tell the user.
    let hotkey_manager = match HotkeyManager::new(&config.hotkey) {
        Ok(manager) => manager,
        Err(error) => {
            warn!("Hotkey configuration is unusable, falling back to defaults: {error:#}");
            show_nonfatal_warning(format!(
                "热键配置无效：{error}\n\n已临时回退为默认热键 Ctrl+Shift+V，请在设置中重新配置。"
            ));
            HotkeyManager::new(&HotkeyConfig::default())?
        }
    };
    info!("Hotkey registered");
    info!("Startup initialization complete");

    // Run system tray (hotkey callback is set up inside run_app for state sync)
    info!("Starting system tray...");
    doubao_voice_input::ui::run_app(config, did, voice_controller, hotkey_manager, audio_capture)?;

    info!("Application exited");
    Ok(())
}

/// Run in CLI mode for testing
async fn run_cli_mode() -> Result<()> {
    let _ = init_logging(true);

    println!("╔═══════════════════════════════════════════════════════════╗");
    println!(
        "║     豆包语音输入 - CLI 验证版本 v{}        ║",
        env!("CARGO_PKG_VERSION")
    );
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!();

    info!(
        "Starting Doubao Voice Input v{} (CLI Mode)",
        env!("CARGO_PKG_VERSION")
    );

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
    info!("Startup initialization complete");
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

#[derive(Clone)]
struct StartupLogWriter {
    buffer: Arc<StdMutex<String>>,
}

struct StartupLogWriterGuard {
    buffer: Arc<StdMutex<String>>,
}

#[derive(Clone)]
struct DailyLogWriter {
    directory: Arc<PathBuf>,
    state: Arc<StdMutex<DailyLogState>>,
}

#[derive(Default)]
struct DailyLogState {
    date: String,
    file: Option<File>,
}

struct DailyLogWriterGuard {
    writer: DailyLogWriter,
}

impl DailyLogWriter {
    fn new(directory: PathBuf) -> Self {
        Self {
            directory: Arc::new(directory),
            state: Arc::new(StdMutex::new(DailyLogState::default())),
        }
    }

    fn with_current_file<T>(
        &self,
        operation: impl FnOnce(&mut File) -> io::Result<T>,
    ) -> io::Result<T> {
        let date = Local::now().format("%Y-%m-%d").to_string();
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("daily log writer lock poisoned"))?;

        if state.file.is_none() || state.date != date {
            std::fs::create_dir_all(self.directory.as_ref())?;
            let path = self
                .directory
                .join(format!("doubao-voice-input-{date}.log"));
            state.file = Some(OpenOptions::new().create(true).append(true).open(path)?);
            state.date = date;
        }

        operation(state.file.as_mut().expect("daily log file is open"))
    }
}

impl<'a> MakeWriter<'a> for DailyLogWriter {
    type Writer = DailyLogWriterGuard;

    fn make_writer(&'a self) -> Self::Writer {
        DailyLogWriterGuard {
            writer: self.clone(),
        }
    }
}

impl Write for DailyLogWriterGuard {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.writer.with_current_file(|file| file.write(bytes))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.with_current_file(File::flush)
    }
}

impl<'a> MakeWriter<'a> for StartupLogWriter {
    type Writer = StartupLogWriterGuard;

    fn make_writer(&'a self) -> Self::Writer {
        StartupLogWriterGuard {
            buffer: self.buffer.clone(),
        }
    }
}

impl Write for StartupLogWriterGuard {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Ok(mut buffer) = self.buffer.lock() {
            buffer.push_str(&String::from_utf8_lossy(bytes));
        }
        io::stderr().write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stderr().flush()
    }
}

fn init_logging(debug: bool) -> Arc<StdMutex<String>> {
    let level = if debug {
        "doubao_voice_input=debug"
    } else {
        "doubao_voice_input=info"
    };
    let startup_logs = Arc::new(StdMutex::new(String::new()));
    let log_directory = env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("logs")))
        .unwrap_or_else(|| PathBuf::from("logs"));

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| level.into()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_timer(tracing_subscriber::fmt::time::ChronoLocal::rfc_3339())
                .with_writer(StartupLogWriter {
                    buffer: startup_logs.clone(),
                }),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_timer(tracing_subscriber::fmt::time::ChronoLocal::rfc_3339())
                .with_writer(DailyLogWriter::new(log_directory)),
        )
        .init();

    startup_logs
}

fn report_ui_mode_error(error: &anyhow::Error, logs: &Arc<StdMutex<String>>) {
    let startup_logs = logs
        .lock()
        .map(|logs| logs.clone())
        .unwrap_or_else(|_| String::from("日志缓冲读取失败"));
    let details = if startup_logs.trim().is_empty() {
        format!("启动失败：{error:#}")
    } else {
        format!("启动失败：{error:#}\n\n已捕获的启动日志：\n{startup_logs}")
    };

    #[cfg(target_os = "windows")]
    {
        if let Err(copy_error) = clipboard_win::set_clipboard_string(&details) {
            eprintln!("Failed to copy UI startup error to clipboard: {copy_error:?}");
        }

        show_windows_error_message(&details);
    }

    #[cfg(not(target_os = "windows"))]
    {
        eprintln!("UI mode failed:\n{details}");
    }
}

#[cfg(target_os = "windows")]
fn show_windows_error_message(message: &str) {
    use windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR;

    show_windows_message_box("豆包语音输入启动失败", message, MB_ICONERROR.0);
}

/// Show a warning without blocking startup; the message box runs on its own
/// thread so the event loop keeps initializing behind it.
fn show_nonfatal_warning(message: String) {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::WindowsAndMessaging::MB_ICONWARNING;

        std::thread::spawn(move || {
            show_windows_message_box("豆包语音输入", &message, MB_ICONWARNING.0);
        });
    }

    #[cfg(not(target_os = "windows"))]
    eprintln!("Warning: {message}");
}

#[cfg(target_os = "windows")]
fn show_windows_message_box(title: &str, message: &str, style: u32) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_OK, MESSAGEBOX_STYLE};

    let message = message
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let title = title
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();

    unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR(message.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MESSAGEBOX_STYLE(style),
        );
    }
}
