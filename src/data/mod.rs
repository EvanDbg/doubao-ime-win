//! Data module for configuration and credential management

mod config;
mod credential;

pub use config::{AppConfig, GeneralConfig, HotkeyConfig, FloatingButtonConfig, AsrConfig};
pub use credential::CredentialStore;
