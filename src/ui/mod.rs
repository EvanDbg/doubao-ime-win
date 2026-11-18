//! UI Module
//!
//! Handles system tray and floating button UI.

mod floating_button;
mod settings_window;
mod system_tray;

pub use floating_button::{
    ButtonState, FloatingButton, FloatingButtonConfig, FloatingButtonEvent,
    FloatingButtonStateSetter,
};
pub use system_tray::run_app;

#[cfg(target_os = "windows")]
pub use settings_window::{exit_requested, open};
