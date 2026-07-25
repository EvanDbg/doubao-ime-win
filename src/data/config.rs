//! Application Configuration
//!
//! Handles loading and saving application configuration.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub hotkey: HotkeyConfig,
    #[serde(default)]
    pub floating_button: FloatingButtonConfig,
    #[serde(default)]
    pub asr: AsrConfig,
    #[serde(default)]
    pub cloud: CloudConfig,
}

impl AppConfig {
    /// Get the config file path
    pub fn config_path() -> PathBuf {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        exe_dir.join("config.toml")
    }

    /// Get the credentials file path
    pub fn credentials_path() -> PathBuf {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        exe_dir.join("credentials.json")
    }

    /// Load configuration from file or create default
    pub fn load_or_default() -> Result<Self> {
        let path = Self::config_path();

        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let config: AppConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            let config = AppConfig::default();
            config.save()?;
            Ok(config)
        }
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }
}

/// General configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default = "default_language")]
    pub language: String,
}

fn default_language() -> String {
    "zh-CN".to_string()
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            auto_start: false,
            language: default_language(),
        }
    }
}

/// Hotkey configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    /// Action invoked by the configured key: this application's voice input
    /// or the official Doubao input method shortcut.
    #[serde(default = "default_hotkey_action")]
    pub action: String,
    /// Active binding source: `standard` uses the existing global-hotkey
    /// implementation, while `raw` listens to a Windows keyboard event.
    #[serde(default = "default_hotkey_binding")]
    pub binding: String,
    /// Trigger behavior: `single_tap`, `double_tap`, or `hold`.
    /// The legacy `combo` value remains supported as a single-tap shortcut.
    #[serde(default = "default_hotkey_mode")]
    pub mode: String,
    #[serde(default = "default_combo_key")]
    pub combo_key: String,
    #[serde(default = "default_double_tap_key")]
    pub double_tap_key: String,
    #[serde(default = "default_double_tap_interval")]
    pub double_tap_interval: u64,
    /// Windows virtual-key code captured for a raw binding.
    #[serde(default)]
    pub raw_vk_code: u32,
    /// Windows scan code captured for a raw binding.
    #[serde(default)]
    pub raw_scan_code: u32,
    /// Whether the captured raw key has the extended-key flag.
    #[serde(default)]
    pub raw_extended: bool,
}

fn default_hotkey_binding() -> String {
    "standard".to_string()
}

fn default_hotkey_action() -> String {
    "voice_input".to_string()
}

fn default_hotkey_mode() -> String {
    "combo".to_string()
}

fn default_combo_key() -> String {
    "Ctrl+Shift+V".to_string()
}

fn default_double_tap_key() -> String {
    "Ctrl".to_string()
}

fn default_double_tap_interval() -> u64 {
    300
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            action: default_hotkey_action(),
            binding: default_hotkey_binding(),
            mode: default_hotkey_mode(),
            combo_key: default_combo_key(),
            double_tap_key: default_double_tap_key(),
            double_tap_interval: default_double_tap_interval(),
            raw_vk_code: 0,
            raw_scan_code: 0,
            raw_extended: false,
        }
    }
}

/// Floating button configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloatingButtonConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_position")]
    pub position_x: i32,
    #[serde(default = "default_position")]
    pub position_y: i32,
}

fn default_true() -> bool {
    true
}

fn default_position() -> i32 {
    100
}

impl Default for FloatingButtonConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            position_x: 100,
            position_y: 100,
        }
    }
}

/// ASR configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrConfig {
    #[serde(default = "default_true")]
    pub vad_enabled: bool,
    #[serde(default)]
    pub aec_enabled: bool,
    #[serde(default)]
    pub audio_quality: AudioQuality,
    #[serde(default)]
    pub punctuation_mode: PunctuationMode,
    #[serde(default = "default_end_smooth_window_ms")]
    pub end_smooth_window_ms: u32,
    #[serde(default = "default_post_ratio_gain")]
    pub post_ratio_gain: f32,
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self {
            vad_enabled: true,
            aec_enabled: false,
            audio_quality: AudioQuality::default(),
            punctuation_mode: PunctuationMode::default(),
            end_smooth_window_ms: default_end_smooth_window_ms(),
            post_ratio_gain: default_post_ratio_gain(),
        }
    }
}

fn default_end_smooth_window_ms() -> u32 {
    800
}

fn default_post_ratio_gain() -> f32 {
    1.0
}

#[derive(Debug, Clone, Copy)]
pub struct AudioProcessingConfig {
    pub vad_enabled: bool,
    pub aec_enabled: bool,
    pub end_smooth_window_ms: u32,
    pub post_ratio_gain: f32,
}

impl From<&AsrConfig> for AudioProcessingConfig {
    fn from(config: &AsrConfig) -> Self {
        let post_ratio_gain = if config.post_ratio_gain.is_finite() {
            config.post_ratio_gain.clamp(0.25, 4.0)
        } else {
            default_post_ratio_gain()
        };
        Self {
            vad_enabled: config.vad_enabled,
            aec_enabled: config.aec_enabled,
            end_smooth_window_ms: config.end_smooth_window_ms.min(3_000),
            post_ratio_gain,
        }
    }
}

/// Optional cloud processing applied around voice input sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudConfig {
    /// Send final ASR text to NER for future context and candidate improvement.
    #[serde(default)]
    pub ner_enabled: bool,
    /// Remove filler speech after a voice session and auto-replace on success.
    #[serde(default)]
    pub auto_polish_enabled: bool,
    /// Include text surrounding the target caret as LLM correction context.
    #[serde(default)]
    pub llm_context_enabled: bool,
    /// Explicitly select the custom OpenAI-compatible backend. `None` keeps
    /// compatibility with older configs, where a non-empty URL enabled it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_custom_api_enabled: Option<bool>,
    /// OpenAI-compatible Chat Completions URL. Empty keeps the built-in service.
    #[serde(default)]
    pub llm_base_url: String,
    /// Bearer token for the custom OpenAI-compatible service.
    #[serde(default)]
    pub llm_api_key: String,
    /// Model name used with the custom OpenAI-compatible service.
    #[serde(default)]
    pub llm_model: String,
    /// Optional replacement prompt for voice correction. Empty uses the built-in prompt.
    #[serde(default)]
    pub llm_prompt: String,
    /// Optional provider extension: `omit`, `disabled`, or `enabled`.
    #[serde(default = "default_llm_thinking_mode")]
    pub llm_thinking_mode: String,
    /// Optional OpenAI-compatible reasoning effort, such as `low`, `medium`, or `high`.
    #[serde(default)]
    pub llm_reasoning_effort: String,
}

impl Default for CloudConfig {
    fn default() -> Self {
        Self {
            ner_enabled: false,
            auto_polish_enabled: false,
            llm_context_enabled: false,
            llm_custom_api_enabled: Some(false),
            llm_base_url: String::new(),
            llm_api_key: String::new(),
            llm_model: String::new(),
            llm_prompt: String::new(),
            llm_thinking_mode: default_llm_thinking_mode(),
            llm_reasoning_effort: String::new(),
        }
    }
}

impl CloudConfig {
    /// Resolve the backend while preserving configs written before the
    /// explicit custom-API switch was introduced.
    pub fn custom_api_enabled(&self) -> bool {
        self.llm_custom_api_enabled
            .unwrap_or_else(|| !self.llm_base_url.trim().is_empty())
    }
}

fn default_llm_thinking_mode() -> String {
    "omit".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_hotkey_config_defaults_to_local_voice_input() {
        let config: HotkeyConfig = toml::from_str("").expect("legacy hotkey config should load");

        assert_eq!(config.action, "voice_input");
    }

    #[test]
    fn hotkey_action_is_serialized() {
        let mut config = HotkeyConfig::default();
        config.action = "official_hands_free".to_string();

        let serialized = toml::to_string(&config).expect("hotkey config should serialize");

        assert!(serialized.contains("action = \"official_hands_free\""));
    }
}

/// Audio format sent to the ASR service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AudioQuality {
    /// Official-compatible 16kHz mono Opus.
    #[default]
    Standard,
    /// Experimental 24kHz mono Opus; some ASR routes are less accurate.
    HighQuality,
}

impl AudioQuality {
    pub const fn sample_rate(self) -> u32 {
        match self {
            Self::Standard => 16_000,
            Self::HighQuality => 24_000,
        }
    }

    pub const fn channels(self) -> u16 {
        1
    }
}

/// Client-side punctuation display behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PunctuationMode {
    #[default]
    Smart,
    Spaces,
    NoSentenceFinal,
    Preserve,
}
