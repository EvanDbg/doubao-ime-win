//! Business logic module
//!
//! Contains the core business logic for voice input control.

mod context_capture;
mod hotkey_manager;
mod punctuation;
mod text_inserter;
mod trigger;
mod voice_controller;
mod voice_session;

pub use context_capture::{capture_context, ContextSnapshot, TargetWindow};
pub use hotkey_manager::{
    invoke_official_doubao, HotkeyEvent, HotkeyManager, OfficialDoubaoAction, RawKeyBinding,
};
pub use text_inserter::TextInserter;
pub use voice_controller::VoiceController;
pub use voice_session::{VoiceSessionRecord, VoiceSessionStore};
