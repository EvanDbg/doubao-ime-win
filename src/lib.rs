//! Doubao Voice Input - Windows voice input tool
//!
//! A lightweight voice input tool that uses Doubao ASR for real-time
//! speech recognition and inserts text into the focused window.

pub mod asr;
pub mod audio;
pub mod business;
pub mod cloud;
pub mod data;
mod startup;
pub mod ui;

/// Install the process-wide Rustls crypto provider used by all network clients.
///
/// Calling this more than once is harmless. If an embedding application already
/// installed a provider, Rustls keeps that provider instead.
pub fn init_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

pub use asr::AsrClient;
pub use audio::AudioCapture;
pub use business::{HotkeyManager, TextInserter, VoiceController, VoiceSessionStore};
pub use cloud::{NerClient, NerLexicon, RichChatClient};
pub use data::{AppConfig, CredentialStore, HotkeyConfig};
