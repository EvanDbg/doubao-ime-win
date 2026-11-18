//! Data module for configuration and credential management

mod config;
mod credential;

pub use config::{
    AiConfig, AppConfig, AsrConfig, FloatingButtonConfig, GeneralConfig, HotkeyConfig,
};
pub use settings::RuntimeSettings;

mod settings;
pub use credential::CredentialStore;
