//! Voice Controller
//!
//! Coordinates voice input between audio capture, ASR, and text insertion.

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::asr::{AsrClient, ResponseType};
use crate::audio::AudioCapture;
use crate::business::TextInserter;

/// Voice input controller
pub struct VoiceController {
    asr_client: Arc<AsrClient>,
    audio_capture: Arc<AudioCapture>,
    text_inserter: Arc<TextInserter>,
    is_recording: Arc<AtomicBool>,
    stop_signal: Arc<AtomicBool>,
}

impl VoiceController {
    /// Create a new voice controller
    pub fn new(
        asr_client: Arc<AsrClient>,
        audio_capture: Arc<AudioCapture>,
        text_inserter: Arc<TextInserter>,
    ) -> Self {
        Self {
            asr_client,
            audio_capture,
            text_inserter,
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
        let audio_rx = self.audio_capture.start()?;
        tracing::info!("Audio capture started, frames will be sent to ASR");

        // Start ASR
        tracing::debug!("Connecting to ASR server...");
        let mut result_rx = self.asr_client.start_realtime(audio_rx).await?;
        tracing::info!("ASR connection established");

        // Clone for the task
        let text_inserter = self.text_inserter.clone();
        let is_recording = self.is_recording.clone();
        let stop_signal = self.stop_signal.clone();
        let audio_capture = self.audio_capture.clone();

        // Spawn result processing task
        tokio::spawn(async move {
            let mut last_text = String::new();
            let mut response_count = 0u32;

            tracing::info!("ASR result processing task started");

            loop {
                // Check stop signal
                if stop_signal.load(Ordering::SeqCst) {
                    tracing::info!("Voice input stopped by user (processed {} responses)", response_count);
                    break;
                }

                // Use timeout to periodically check stop signal
                match tokio::time::timeout(
                    std::time::Duration::from_millis(100),
                    result_rx.recv()
                ).await {
                    Ok(Some(response)) => {
                        response_count += 1;
                        match response.response_type {
                            ResponseType::InterimResult => {
                                tracing::debug!("[INTERIM #{}] {}", response_count, response.text);
                                println!("📝 [识别中] {}", response.text);
                                if !response.text.is_empty() {
                                    if let Err(e) = update_text(&text_inserter, &last_text, &response.text) {
                                        tracing::error!("Failed to update text: {}", e);
                                    }
                                    last_text = response.text.clone();
                                }
                            }
                            ResponseType::FinalResult => {
                                tracing::info!("[FINAL #{}] {}", response_count, response.text);
                                println!("✅ [确认] {}", response.text);
                                if !response.text.is_empty() {
                                    if let Err(e) = update_text(&text_inserter, &last_text, &response.text) {
                                        tracing::error!("Failed to update text: {}", e);
                                    }
                                    // 清空 last_text，这样新的语句不会删除已确认的文字
                                    last_text = String::new();
                                }
                            }
                            ResponseType::SessionFinished => {
                                tracing::info!("ASR session finished (total {} responses)", response_count);
                                println!("🏁 [会话结束]");
                                break;
                            }
                            ResponseType::Error => {
                                tracing::error!("ASR error: {}", response.error_msg);
                                println!("❌ [错误] {}", response.error_msg);
                                break;
                            }
                            _ => {
                                tracing::trace!("Other response type: {:?}", response.response_type);
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

/// Update text in the focused window
///
/// Deletes the old text and inserts the new text
fn update_text(text_inserter: &TextInserter, old_text: &str, new_text: &str) -> Result<()> {
    // Calculate characters to delete
    let chars_to_delete = old_text.chars().count();

    // Delete old text
    if chars_to_delete > 0 {
        text_inserter.delete_chars(chars_to_delete)?;
    }

    // Insert new text
    text_inserter.insert(new_text)?;

    tracing::debug!("Updated text: '{}' -> '{}'", old_text, new_text);
    Ok(())
}
