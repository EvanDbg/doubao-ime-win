//! Voice Controller
//!
//! Coordinates voice input between audio capture, ASR, and text insertion.

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::ai::AiPostProcessor;
use crate::asr::{AsrClient, ResponseType};
use crate::audio::AudioCapture;
use crate::business::TextInserter;
use crate::data::RuntimeSettings;

/// Voice input controller
pub struct VoiceController {
    asr_client: Arc<AsrClient>,
    audio_capture: Arc<AudioCapture>,
    text_inserter: Arc<TextInserter>,
    settings: RuntimeSettings,
    is_recording: Arc<AtomicBool>,
    stop_signal: Arc<AtomicBool>,
}

impl VoiceController {
    /// Create a new voice controller
    pub fn new(
        asr_client: Arc<AsrClient>,
        audio_capture: Arc<AudioCapture>,
        text_inserter: Arc<TextInserter>,
        settings: RuntimeSettings,
    ) -> Self {
        Self {
            asr_client,
            audio_capture,
            text_inserter,
            settings,
            is_recording: Arc::new(AtomicBool::new(false)),
            stop_signal: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Check if currently recording
    pub fn is_recording(&self) -> bool {
        self.is_recording.load(Ordering::SeqCst)
    }

    /// Toggle voice input on/off
    pub async fn toggle(&mut self) -> Result<()> {
        if self.is_recording() {
            self.stop().await
        } else {
            self.start().await
        }
    }

    /// Start voice input
    pub async fn start(&mut self) -> Result<()> {
        if self.is_recording() {
            return Ok(());
        }

        tracing::info!("Starting voice input...");
        self.is_recording.store(true, Ordering::SeqCst);
        self.stop_signal.store(false, Ordering::SeqCst);

        // Start audio capture
        tracing::debug!("Starting audio capture...");
        let audio_rx = self
            .audio_capture
            .start(self.settings.get().asr.vad_enabled)?;
        tracing::info!("Audio capture started, frames will be sent to ASR");

        // Clone for the task
        let asr_client = self.asr_client.clone();
        let text_inserter = self.text_inserter.clone();
        let is_recording = self.is_recording.clone();
        let stop_signal = self.stop_signal.clone();
        let audio_capture = self.audio_capture.clone();
        let settings = self.settings.clone();

        // Spawn result processing task
        tokio::spawn(async move {
            let mut last_text = String::new();
            let mut response_count = 0u32;

            tracing::info!("Persistent voice listener started; waiting for first speech");

            let mut result_rx = match asr_client.start_realtime(audio_rx).await {
                Ok(receiver) => {
                    tracing::info!("ASR connection established");
                    receiver
                }
                Err(error) => {
                    tracing::error!("Failed to establish ASR connection: {}", error);
                    println!("❌ [启动失败] {}", error);
                    audio_capture.stop();
                    is_recording.store(false, Ordering::SeqCst);
                    return;
                }
            };

            loop {
                // Check stop signal
                if stop_signal.load(Ordering::SeqCst) {
                    tracing::info!(
                        "Voice input stopped by user (processed {} responses)",
                        response_count
                    );
                    break;
                }

                // Use timeout to periodically check stop signal
                match tokio::time::timeout(std::time::Duration::from_millis(100), result_rx.recv())
                    .await
                {
                    Ok(Some(response)) => {
                        response_count += 1;
                        match response.response_type {
                            ResponseType::InterimResult => {
                                tracing::debug!("[INTERIM #{}] {}", response_count, response.text);
                                println!("📝 [识别中] {}", response.text);
                                if !response.text.is_empty() {
                                    if let Err(e) =
                                        update_text(&text_inserter, &last_text, &response.text)
                                    {
                                        tracing::error!("Failed to update text: {}", e);
                                    }
                                    last_text = response.text.clone();
                                }
                            }
                            ResponseType::FinalResult => {
                                tracing::info!("[FINAL #{}] {}", response_count, response.text);
                                println!("✅ [确认] {}", response.text);
                                if !response.text.is_empty() {
                                    let final_text = if settings.get().ai.enabled {
                                        match AiPostProcessor::process(
                                            &response.text,
                                            &settings.get().ai,
                                        )
                                        .await
                                        {
                                            Ok(processed) if !processed.trim().is_empty() => {
                                                println!("🤖 [AI] {}", processed);
                                                processed
                                            }
                                            Ok(_) => response.text.clone(),
                                            Err(e) => {
                                                tracing::error!("AI post-processing failed: {}", e);
                                                println!("⚠️ [AI 后处理失败] {}", e);
                                                response.text.clone()
                                            }
                                        }
                                    } else {
                                        response.text.clone()
                                    };
                                    if let Err(e) =
                                        update_text(&text_inserter, &last_text, &final_text)
                                    {
                                        tracing::error!("Failed to update text: {}", e);
                                    }
                                    // 清空 last_text，这样新的语句不会删除已确认的文字
                                    last_text = String::new();
                                }
                            }
                            ResponseType::SessionFinished => {
                                tracing::info!(
                                    "Speech segment finished (total {} responses)",
                                    response_count
                                );
                                println!("🏁 [语音段结束]");
                                break;
                            }
                            ResponseType::Error => {
                                tracing::error!("ASR error: {}", response.error_msg);
                                println!("❌ [错误] {}", response.error_msg);
                                break;
                            }
                            _ => {
                                tracing::trace!(
                                    "Other response type: {:?}",
                                    response.response_type
                                );
                            }
                        }
                    }
                    Ok(None) => {
                        // Channel closed
                        tracing::warn!("ASR result channel closed unexpectedly");
                        break;
                    }
                    Err(_) => {
                        // Timeout, continue loop to check stop signal
                        continue;
                    }
                }
            }

            // Cleanup
            audio_capture.stop();
            is_recording.store(false, Ordering::SeqCst);
            tracing::info!("Persistent voice listener stopped");
        });

        Ok(())
    }

    /// Stop voice input
    pub async fn stop(&mut self) -> Result<()> {
        if !self.is_recording() {
            return Ok(());
        }

        tracing::info!("Stopping voice input...");

        // Signal stop
        self.stop_signal.store(true, Ordering::SeqCst);
        self.audio_capture.stop();

        // Wait a bit for the task to finish
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        self.is_recording.store(false, Ordering::SeqCst);

        Ok(())
    }
}

/// Update text in the focused window using incremental updates
///
/// Uses prefix matching to minimize deletions and insertions:
/// 1. Find the common prefix between old and new text
/// 2. Only delete characters beyond the common prefix
/// 3. Only append the new suffix
///
/// This significantly reduces visual flickering compared to full replacement.
fn update_text(text_inserter: &TextInserter, old_text: &str, new_text: &str) -> Result<()> {
    // 找到公共前缀长度（无需删除和重新输入的部分）
    let common_prefix_len = old_text
        .chars()
        .zip(new_text.chars())
        .take_while(|(a, b)| a == b)
        .count();

    // 计算需要删除的字符数 = 旧文本超出公共前缀的部分
    let chars_to_delete = old_text.chars().count() - common_prefix_len;

    // 需要追加的文本 = 新文本超出公共前缀的部分
    let text_to_append: String = new_text.chars().skip(common_prefix_len).collect();

    // 执行增量更新
    if chars_to_delete > 0 {
        text_inserter.delete_chars(chars_to_delete)?;
    }
    if !text_to_append.is_empty() {
        text_inserter.insert(&text_to_append)?;
    }

    tracing::debug!(
        "Updated text incrementally: '{}' -> '{}' (kept {} chars, deleted {}, appended '{}')",
        old_text,
        new_text,
        common_prefix_len,
        chars_to_delete,
        text_to_append
    );
    Ok(())
}
