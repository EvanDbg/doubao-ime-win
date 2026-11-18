//! Audio capture and processing module

mod capture;
mod encoder;

pub use capture::{AudioCapture, AudioFrameEvent};
pub use encoder::OpusEncoder;
