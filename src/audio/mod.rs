//! Audio capture and processing module

mod capture;
mod encoder;

pub use capture::AudioCapture;
pub use encoder::OpusEncoder;
