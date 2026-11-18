//! ASR (Automatic Speech Recognition) module
//!
//! This module implements the Doubao ASR protocol for real-time speech recognition.

mod client;
mod constants;
mod device;
mod protocol;

pub use client::AsrClient;
pub use constants::*;
pub use device::{get_asr_token, register_device, DeviceCredentials};
pub use protocol::{AsrResponse, ResponseType};

// Include the generated protobuf code
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/asr.rs"));
}
