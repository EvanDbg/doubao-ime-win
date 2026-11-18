//! Data module for configuration and credential management

mod config;
mod credential;

pub use config::{
    AiConfig, AppConfig, AsrConfig, FloatingButtonConfig, GeneralConfig, HotkeyConfig,
    PunctuationMode,
};
pub use settings::RuntimeSettings;

mod settings;
pub use credential::CredentialStore;
