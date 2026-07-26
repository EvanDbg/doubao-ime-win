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
            // Rewrite the file when its on-disk shape differs from the
            // current schema so legacy hotkey fields are migrated once.
            if let Ok(current) = toml::to_string_pretty(&config) {
                if current != content {
                    if let Err(error) = config.save() {
                        tracing::warn!("Failed to rewrite migrated config: {error:#}");
                    }
                }
            }
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

/// Action invoked by the configured hotkey.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyAction {
    /// This application's voice input pipeline.
    #[default]
    VoiceInput,
    /// Forward to the official Doubao push-to-talk shortcut (right Alt).
    OfficialHold,
    /// Forward to the official Doubao hands-free shortcut (left Ctrl+Win).
    OfficialHandsFree,
}

impl HotkeyAction {
    /// Whether the action forwards to the official Doubao input method.
    pub fn is_official(self) -> bool {
        matches!(self, Self::OfficialHold | Self::OfficialHandsFree)
    }
}

/// Source of the configured binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyBinding {
    /// A named key or combo handled by `global-hotkey` (or the keyboard hook
    /// for pure modifier keys).
    #[default]
    Standard,
    /// A captured physical key matched by virtual-key/scan code.
    Raw,
}

/// How the configured key triggers its action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TriggerMode {
    #[default]
    SingleTap,
    DoubleTap,
    Hold,
}

/// Physical key captured for a raw binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawKeyConfig {
    /// Windows virtual-key code; always non-zero for a stored binding.
    #[serde(default)]
    pub vk_code: u32,
    /// `None` matches any scan code (mouse side buttons have none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_code: Option<u32>,
    /// Whether the key carries the extended-key flag.
    #[serde(default)]
    pub extended: bool,
}

pub const MIN_DOUBLE_TAP_INTERVAL_MS: u64 = 100;
pub const MAX_DOUBLE_TAP_INTERVAL_MS: u64 = 1000;
pub const DEFAULT_DOUBLE_TAP_INTERVAL_MS: u64 = 300;
const DEFAULT_STANDARD_KEY: &str = "Ctrl+Shift+V";

/// Hotkey configuration.
///
/// Deserialization goes through [`HotkeyConfigCompat`] so files and IPC
/// payloads written before the schema cleanup keep loading: legacy fields
/// (`combo_key`, `double_tap_key`, `double_tap_interval`, `raw_vk_code`,
/// `raw_scan_code`, `raw_extended`, `mode = "combo"`) are migrated and
/// unknown values fall back to defaults instead of failing startup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "HotkeyConfigCompat")]
pub struct HotkeyConfig {
    pub action: HotkeyAction,
    pub binding: HotkeyBinding,
    /// Requested trigger mode; official actions override it, see
    /// [`HotkeyConfig::effective_mode`].
    pub mode: TriggerMode,
    /// Trigger key for the standard binding: a single key name ("F13",
    /// "Ctrl") or a combo ("Ctrl+Shift+V").
    pub standard_key: String,
    /// Double-tap window in milliseconds, clamped on load.
    pub double_tap_interval_ms: u64,
    /// Captured key for the raw binding; required when `binding` is `Raw`.
    /// Must stay the last field: TOML serializes tables after scalars.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_key: Option<RawKeyConfig>,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            action: HotkeyAction::VoiceInput,
            binding: HotkeyBinding::Standard,
            mode: TriggerMode::SingleTap,
            standard_key: DEFAULT_STANDARD_KEY.to_string(),
            double_tap_interval_ms: DEFAULT_DOUBLE_TAP_INTERVAL_MS,
            raw_key: None,
        }
    }
}

impl HotkeyConfig {
    /// The trigger mode that is actually in effect. Official actions have a
    /// fixed interaction shape regardless of the stored `mode`.
    pub fn effective_mode(&self) -> TriggerMode {
        match self.action {
            HotkeyAction::OfficialHold => TriggerMode::Hold,
            HotkeyAction::OfficialHandsFree => TriggerMode::SingleTap,
            HotkeyAction::VoiceInput => self.mode,
        }
    }
}

/// Accepts both the current and the legacy on-disk/IPC shape of `[hotkey]`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct HotkeyConfigCompat {
    action: Option<String>,
    binding: Option<String>,
    mode: Option<String>,
    standard_key: Option<String>,
    double_tap_interval_ms: Option<u64>,
    raw_key: Option<RawKeyConfig>,
    // Legacy fields, consumed by migration and never written back.
    combo_key: Option<String>,
    double_tap_key: Option<String>,
    double_tap_interval: Option<u64>,
    raw_vk_code: Option<u32>,
    raw_scan_code: Option<u32>,
    raw_extended: Option<bool>,
}

fn normalized(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

fn trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

impl From<HotkeyConfigCompat> for HotkeyConfig {
    fn from(compat: HotkeyConfigCompat) -> Self {
        let action = match normalized(&compat.action).as_deref() {
            None | Some("voice_input") => HotkeyAction::VoiceInput,
            Some("official_hold") => HotkeyAction::OfficialHold,
            Some("official_hands_free") => HotkeyAction::OfficialHandsFree,
            Some(other) => {
                tracing::warn!("Unknown hotkey action \"{other}\", using voice_input");
                HotkeyAction::VoiceInput
            }
        };

        let requested_binding = match normalized(&compat.binding).as_deref() {
            None | Some("standard") => HotkeyBinding::Standard,
            Some("raw") => HotkeyBinding::Raw,
            Some(other) => {
                tracing::warn!("Unknown hotkey binding \"{other}\", using standard");
                HotkeyBinding::Standard
            }
        };

        // The legacy default mode was "combo": a single-tap trigger whose
        // key lived in `combo_key` instead of `double_tap_key`.
        let legacy_mode = normalized(&compat.mode);
        let legacy_combo = matches!(legacy_mode.as_deref(), None | Some("combo"));
        let mode = match legacy_mode.as_deref() {
            None | Some("combo") | Some("single_tap") => TriggerMode::SingleTap,
            Some("double_tap") => TriggerMode::DoubleTap,
            Some("hold") => TriggerMode::Hold,
            Some(other) => {
                tracing::warn!("Unknown hotkey trigger mode \"{other}\", using single_tap");
                TriggerMode::SingleTap
            }
        };
        // Normalize contradictory stored combinations up front so the file
        // and `effective_mode()` agree.
        let mode = match action {
            HotkeyAction::OfficialHold => TriggerMode::Hold,
            HotkeyAction::OfficialHandsFree => TriggerMode::SingleTap,
            HotkeyAction::VoiceInput => mode,
        };

        let raw_key = compat.raw_key.filter(|raw| raw.vk_code != 0).or_else(|| {
            compat.raw_vk_code.filter(|&vk| vk != 0).map(|vk_code| {
                RawKeyConfig {
                    vk_code,
                    // A legacy scan code of 0 meant "match any".
                    scan_code: compat.raw_scan_code.filter(|&scan| scan != 0),
                    extended: compat.raw_extended.unwrap_or(false),
                }
            })
        });

        let binding = if requested_binding == HotkeyBinding::Raw && raw_key.is_none() {
            tracing::warn!("Raw hotkey binding has no captured key, using the standard binding");
            HotkeyBinding::Standard
        } else {
            requested_binding
        };

        let standard_key = trimmed(compat.standard_key)
            .or_else(|| match requested_binding {
                // The legacy UI wrote the hidden text field into
                // `double_tap_key` while a raw binding was active; discard it.
                HotkeyBinding::Raw => None,
                HotkeyBinding::Standard if legacy_combo => trimmed(compat.combo_key),
                // Non-combo legacy configs kept their trigger key in
                // `double_tap_key` for every mode; its default was "Ctrl".
                HotkeyBinding::Standard => {
                    Some(trimmed(compat.double_tap_key).unwrap_or_else(|| "Ctrl".to_string()))
                }
            })
            .unwrap_or_else(|| DEFAULT_STANDARD_KEY.to_string());

        let double_tap_interval_ms = compat
            .double_tap_interval_ms
            .or(compat.double_tap_interval)
            .unwrap_or(DEFAULT_DOUBLE_TAP_INTERVAL_MS)
            .clamp(MIN_DOUBLE_TAP_INTERVAL_MS, MAX_DOUBLE_TAP_INTERVAL_MS);

        Self {
            action,
            binding,
            mode,
            standard_key,
            double_tap_interval_ms,
            raw_key,
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
    fn empty_hotkey_config_loads_defaults() {
        let config: HotkeyConfig = toml::from_str("").expect("empty hotkey config should load");

        assert_eq!(config, HotkeyConfig::default());
    }

    #[test]
    fn hotkey_action_is_serialized_as_snake_case() {
        let mut config = HotkeyConfig::default();
        config.action = HotkeyAction::OfficialHandsFree;

        let serialized = toml::to_string(&config).expect("hotkey config should serialize");

        assert!(serialized.contains("action = \"official_hands_free\""));
    }

    #[test]
    fn legacy_combo_config_migrates_to_single_tap() {
        let config: HotkeyConfig = toml::from_str(
            r#"
            action = "voice_input"
            binding = "standard"
            mode = "combo"
            combo_key = "Ctrl+Alt+Space"
            double_tap_key = "Ctrl"
            double_tap_interval = 300
            "#,
        )
        .expect("legacy combo config should load");

        assert_eq!(config.mode, TriggerMode::SingleTap);
        assert_eq!(config.standard_key, "Ctrl+Alt+Space");
        assert_eq!(config.double_tap_interval_ms, 300);
        assert_eq!(config.raw_key, None);
    }

    #[test]
    fn legacy_non_combo_config_takes_key_from_double_tap_key() {
        // Real-world dirty data: a combo string stored in the field that was
        // named after double-tap even though the mode is single_tap.
        let config: HotkeyConfig = toml::from_str(
            r#"
            mode = "single_tap"
            combo_key = "Ctrl+Shift+V"
            double_tap_key = "Ctrl+Shift+V"
            "#,
        )
        .expect("legacy single_tap config should load");

        assert_eq!(config.mode, TriggerMode::SingleTap);
        assert_eq!(config.standard_key, "Ctrl+Shift+V");
    }

    #[test]
    fn legacy_double_tap_without_key_defaults_to_ctrl() {
        let config: HotkeyConfig =
            toml::from_str("mode = \"double_tap\"").expect("legacy double_tap config should load");

        assert_eq!(config.mode, TriggerMode::DoubleTap);
        assert_eq!(config.standard_key, "Ctrl");
    }

    #[test]
    fn legacy_raw_fields_migrate_and_discard_garbage_standard_key() {
        let config: HotkeyConfig = toml::from_str(
            r#"
            action = "official_hands_free"
            binding = "raw"
            mode = "hold"
            double_tap_key = "not a key"
            raw_vk_code = 255
            raw_scan_code = 114
            raw_extended = false
            "#,
        )
        .expect("legacy raw config should load");

        assert_eq!(config.action, HotkeyAction::OfficialHandsFree);
        assert_eq!(config.binding, HotkeyBinding::Raw);
        // Contradictory stored mode is normalized by the action.
        assert_eq!(config.mode, TriggerMode::SingleTap);
        assert_eq!(config.effective_mode(), TriggerMode::SingleTap);
        // The hidden-field garbage from the legacy UI is discarded.
        assert_eq!(config.standard_key, DEFAULT_STANDARD_KEY);
        assert_eq!(
            config.raw_key,
            Some(RawKeyConfig {
                vk_code: 255,
                scan_code: Some(114),
                extended: false,
            })
        );
    }

    #[test]
    fn legacy_zero_scan_code_becomes_wildcard() {
        let config: HotkeyConfig = toml::from_str(
            r#"
            binding = "raw"
            raw_vk_code = 5
            raw_scan_code = 0
            "#,
        )
        .expect("legacy mouse-button config should load");

        assert_eq!(
            config.raw_key,
            Some(RawKeyConfig {
                vk_code: 5,
                scan_code: None,
                extended: false,
            })
        );
    }

    #[test]
    fn raw_binding_without_captured_key_falls_back_to_standard() {
        let config: HotkeyConfig =
            toml::from_str("binding = \"raw\"").expect("raw config without key should load");

        assert_eq!(config.binding, HotkeyBinding::Standard);
        assert_eq!(config.standard_key, DEFAULT_STANDARD_KEY);
    }

    #[test]
    fn unknown_enum_values_fall_back_to_defaults() {
        let config: HotkeyConfig = toml::from_str(
            r#"
            action = "surprise"
            binding = "telepathy"
            mode = "triple_tap"
            "#,
        )
        .expect("unknown enum values should not fail deserialization");

        assert_eq!(config.action, HotkeyAction::VoiceInput);
        assert_eq!(config.binding, HotkeyBinding::Standard);
        assert_eq!(config.mode, TriggerMode::SingleTap);
    }

    #[test]
    fn double_tap_interval_is_clamped() {
        let low: HotkeyConfig = toml::from_str("double_tap_interval_ms = 0").unwrap();
        let high: HotkeyConfig = toml::from_str("double_tap_interval_ms = 100000").unwrap();
        let legacy: HotkeyConfig = toml::from_str("double_tap_interval = 250").unwrap();

        assert_eq!(low.double_tap_interval_ms, MIN_DOUBLE_TAP_INTERVAL_MS);
        assert_eq!(high.double_tap_interval_ms, MAX_DOUBLE_TAP_INTERVAL_MS);
        assert_eq!(legacy.double_tap_interval_ms, 250);
    }

    #[test]
    fn official_action_forces_effective_mode() {
        let mut config = HotkeyConfig::default();
        config.mode = TriggerMode::DoubleTap;

        config.action = HotkeyAction::OfficialHold;
        assert_eq!(config.effective_mode(), TriggerMode::Hold);

        config.action = HotkeyAction::OfficialHandsFree;
        assert_eq!(config.effective_mode(), TriggerMode::SingleTap);

        config.action = HotkeyAction::VoiceInput;
        assert_eq!(config.effective_mode(), TriggerMode::DoubleTap);
    }

    #[test]
    fn app_config_round_trips_through_toml_with_raw_key_table() {
        let mut config = AppConfig::default();
        config.hotkey.binding = HotkeyBinding::Raw;
        config.hotkey.raw_key = Some(RawKeyConfig {
            vk_code: 255,
            scan_code: Some(114),
            extended: true,
        });

        let serialized =
            toml::to_string_pretty(&config).expect("config with raw key should serialize");
        let restored: AppConfig =
            toml::from_str(&serialized).expect("serialized config should load");

        assert_eq!(restored.hotkey, config.hotkey);
    }

    #[test]
    fn app_config_without_raw_key_serializes_without_subtable() {
        let serialized =
            toml::to_string_pretty(&AppConfig::default()).expect("default config should serialize");

        assert!(!serialized.contains("raw_key"));
        let restored: AppConfig =
            toml::from_str(&serialized).expect("serialized config should load");
        assert_eq!(restored.hotkey, HotkeyConfig::default());
    }

    #[test]
    fn new_shape_json_round_trips() {
        let json = serde_json::json!({
            "action": "voice_input",
            "binding": "standard",
            "mode": "double_tap",
            "standard_key": "F13",
            "double_tap_interval_ms": 400,
            "raw_key": null,
        });

        let config: HotkeyConfig =
            serde_json::from_value(json).expect("new-shape JSON should load");

        assert_eq!(config.mode, TriggerMode::DoubleTap);
        assert_eq!(config.standard_key, "F13");
        assert_eq!(config.double_tap_interval_ms, 400);
        assert_eq!(config.raw_key, None);
    }

    #[test]
    fn legacy_shape_json_from_old_frontend_still_loads() {
        let json = serde_json::json!({
            "action": "voice_input",
            "binding": "standard",
            "mode": "double_tap",
            "combo_key": "Ctrl+Shift+V",
            "double_tap_key": "Ctrl",
            "double_tap_interval": 300,
            "raw_vk_code": 0,
            "raw_scan_code": 0,
            "raw_extended": false,
        });

        let config: HotkeyConfig =
            serde_json::from_value(json).expect("legacy-shape JSON should load");

        assert_eq!(config.mode, TriggerMode::DoubleTap);
        assert_eq!(config.standard_key, "Ctrl");
        assert_eq!(config.raw_key, None);
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
