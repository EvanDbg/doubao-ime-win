//! Opus Audio Encoder
//!
//! Encodes PCM audio data to Opus format.

use anyhow::{anyhow, Result};
use opus::{Application, Channels, Encoder};

/// Opus encoder wrapper
pub struct OpusEncoder {
    encoder: Encoder,
    sample_rate: u32,
    channels: u16,
    frame_size: usize,
}

impl OpusEncoder {
    /// Create a new Opus encoder
    pub fn new(sample_rate: u32, channels: u16) -> Result<Self> {
        let channels_enum = match channels {
            1 => Channels::Mono,
            2 => Channels::Stereo,
            _ => return Err(anyhow!("Invalid channel count: {}", channels)),
        };

        let encoder = Encoder::new(sample_rate, channels_enum, Application::Audio)
            .map_err(|e| anyhow!("Failed to create Opus encoder: {:?}", e))?;
        
        // Frame size for 20ms at the given sample rate
        let frame_size = (sample_rate * 20 / 1000) as usize;

        Ok(Self {
            encoder,
            sample_rate,
            channels,
            frame_size,
        })
    }

    /// Encode PCM data to Opus
    ///
    /// Input: PCM data as bytes (16-bit samples, little-endian)
    /// Output: Opus-encoded frame
    pub fn encode(&mut self, pcm_data: &[u8]) -> Result<Vec<u8>> {
        // Convert bytes to i16 samples
        let samples: Vec<i16> = pcm_data
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();

        // Ensure we have the right number of samples
        let expected_samples = self.frame_size * self.channels as usize;
        if samples.len() < expected_samples {
            return Err(anyhow!(
                "Not enough samples: got {}, expected {}",
                samples.len(),
                expected_samples
            ));
        }

        // Encode to Opus
        let mut output = vec![0u8; 4000]; // Max Opus frame size
        let encoded_len = self.encoder
            .encode(&samples[..expected_samples], &mut output)
            .map_err(|e| anyhow!("Opus encode error: {:?}", e))?;
        
        output.truncate(encoded_len);
        Ok(output)
    }

    /// Get the frame size in samples
    pub fn frame_size(&self) -> usize {
        self.frame_size
    }

    /// Get the sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get the number of channels
    pub fn channels(&self) -> u16 {
        self.channels
    }
}
