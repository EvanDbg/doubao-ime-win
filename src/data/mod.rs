//! Data module for configuration and credential management

mod config;
mod credential;

pub use config::{
    AppConfig, AsrConfig, AudioProcessingConfig, AudioQuality, CloudConfig, FloatingButtonConfig,
    GeneralConfig, HotkeyAction, HotkeyBinding, HotkeyConfig, PunctuationMode, RawKeyConfig,
    TriggerMode, MAX_DOUBLE_TAP_INTERVAL_MS, MIN_DOUBLE_TAP_INTERVAL_MS,
};
pub use credential::CredentialStore;
