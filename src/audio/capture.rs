//! Audio Capture using cpal

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::sync::mpsc as std_mpsc;
use tokio::sync::mpsc as tokio_mpsc;

use super::encoder::OpusEncoder;

// Opus encoder always uses 16kHz mono
const OPUS_SAMPLE_RATE: u32 = 16000;
const OPUS_CHANNELS: u16 = 1;
const FRAME_DURATION_MS: u32 = 20;

pub struct AudioCapture {
    is_recording: Arc<AtomicBool>,
}

impl AudioCapture {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        match host.default_input_device() {
            Some(device) => {
                println!("[AudioCapture] Default device: {}", device.name().unwrap_or_default());
            }
            None => {
                println!("[AudioCapture] WARNING: No default input device found.");
            }
        }

        Ok(Self {
            is_recording: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn is_recording(&self) -> bool {
        self.is_recording.load(Ordering::SeqCst)
    }

    pub fn start(&self) -> Result<tokio_mpsc::Receiver<Vec<u8>>> {
        if self.is_recording.swap(true, Ordering::SeqCst) {
            return Err(anyhow!("Already recording"));
        }

        let (tokio_tx, tokio_rx) = tokio_mpsc::channel::<Vec<u8>>(100);
        let is_recording = self.is_recording.clone();

        thread::spawn(move || {
            #[cfg(target_os = "windows")]
            {
                use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                }
                println!("[AudioCapture] COM initialized");
            }

            println!("[AudioCapture] >>> Thread spawned <<<");
            use std::io::Write;
            let _ = std::io::stdout().flush();
            
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_audio_capture(tokio_tx, is_recording.clone())
            }));
            
            match result {
                Ok(Ok(_)) => {
                    println!("[AudioCapture] Completed normally");
                }
                Ok(Err(e)) => {
                    println!("[AudioCapture] ERROR: {}", e);
                }
                Err(panic_info) => {
                    println!("[AudioCapture] PANIC: {:?}", panic_info);
                }
            }
            
            is_recording.store(false, Ordering::SeqCst);
            println!("[AudioCapture] Thread exiting");
            let _ = std::io::stdout().flush();
        });

        tracing::info!("Audio capture started");
        Ok(tokio_rx)
    }

    pub fn stop(&self) {
        self.is_recording.store(false, Ordering::SeqCst);
        tracing::info!("Audio capture stopped");
    }
}

fn run_audio_capture(
    tokio_tx: tokio_mpsc::Sender<Vec<u8>>,
    is_recording: Arc<AtomicBool>,
) -> Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("No input device available"))?;

    println!("[AudioCapture] Device: {}", device.name().unwrap_or_default());

    // Get the device's default config - USE THIS EXACTLY
    let supported_config = device.default_input_config()?;
    println!("[AudioCapture] Device config: {:?}", supported_config);

    let native_sample_rate = supported_config.sample_rate().0;
    let native_channels = supported_config.channels();
    let sample_format = supported_config.sample_format();
    
    println!("[AudioCapture] Native: {}Hz, {} channels, {:?}", 
        native_sample_rate, native_channels, sample_format);

    // Use the device's EXACT config (don't override channels!)
    let config = supported_config.config();
    println!("[AudioCapture] Using config: {:?}", config);

    // Create Opus encoder (16kHz mono)
    let mut encoder = match OpusEncoder::new(OPUS_SAMPLE_RATE, OPUS_CHANNELS) {
        Ok(enc) => {
            println!("[AudioCapture] Opus encoder created (16kHz mono)");
            enc
        }
        Err(e) => {
            println!("[AudioCapture] Opus encoder FAILED: {}", e);
            return Err(e);
        }
    };

    // Calculate frame sizes
    let samples_per_frame_native = (native_sample_rate * FRAME_DURATION_MS / 1000) as usize * native_channels as usize;
    let samples_per_frame_opus = (OPUS_SAMPLE_RATE * FRAME_DURATION_MS / 1000) as usize; // mono

    println!("[AudioCapture] Samples/frame: native={} ({}ch), opus={} (mono)", 
        samples_per_frame_native, native_channels, samples_per_frame_opus);

    let (std_tx, std_rx) = std_mpsc::channel::<Vec<i16>>();

    let is_recording_clone = is_recording.clone();
    let frame_counter = Arc::new(AtomicU64::new(0));
    let frame_counter_clone = frame_counter.clone();
    let native_channels_clone = native_channels;

    let err_fn = |err| {
        println!("[AudioCapture] Stream error: {}", err);
    };

    let stream = match sample_format {
        SampleFormat::I16 => {
            println!("[AudioCapture] Building I16 stream");
            let mut buffer = Vec::<i16>::with_capacity(samples_per_frame_native * 2);
            
            device.build_input_stream(
                &config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    if !is_recording_clone.load(Ordering::SeqCst) {
                        return;
                    }
                    
                    buffer.extend_from_slice(data);
                    
                    while buffer.len() >= samples_per_frame_native {
                        let frame: Vec<i16> = buffer.drain(..samples_per_frame_native).collect();
                        let _ = std_tx.send(frame);
                    }
                },
                err_fn,
                None,
            )?
        }
        SampleFormat::F32 => {
            println!("[AudioCapture] Building F32 stream");
            let mut buffer = Vec::<i16>::with_capacity(samples_per_frame_native * 2);
            
            device.build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if !is_recording_clone.load(Ordering::SeqCst) {
                        return;
                    }
                    
                    // Convert f32 to i16
                    let samples: Vec<i16> = data.iter().map(|s| (*s * 32767.0) as i16).collect();
                    buffer.extend_from_slice(&samples);
                    
                    while buffer.len() >= samples_per_frame_native {
                        let frame: Vec<i16> = buffer.drain(..samples_per_frame_native).collect();
                        let _ = std_tx.send(frame);
                    }
                },
                err_fn,
                None,
            )?
        }
        format => {
            return Err(anyhow!("Unsupported format: {:?}", format));
        }
    };

    stream.play()?;
    println!("[AudioCapture] Stream playing!");
    println!("[Mic] Recording started...");

    // Process frames: convert to mono 16kHz and encode
    while is_recording.load(Ordering::SeqCst) {
        match std_rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(frame) => {
                // Step 1: Convert stereo to mono (if needed)
                let mono_frame: Vec<i16> = if native_channels_clone > 1 {
                    // Average channels
                    frame.chunks(native_channels_clone as usize)
                        .map(|chunk| {
                            let sum: i32 = chunk.iter().map(|&s| s as i32).sum();
                            (sum / native_channels_clone as i32) as i16
                        })
                        .collect()
                } else {
                    frame
                };
                
                // Step 2: Resample to 16kHz (if needed)
                let mono_samples_per_native_frame = samples_per_frame_native / native_channels_clone as usize;
                let resampled: Vec<i16> = if mono_samples_per_native_frame != samples_per_frame_opus {
                    let ratio = mono_samples_per_native_frame as f32 / samples_per_frame_opus as f32;
                    (0..samples_per_frame_opus)
                        .map(|i| {
                            let src_idx = ((i as f32 * ratio) as usize).min(mono_frame.len() - 1);
                            mono_frame[src_idx]
                        })
                        .collect()
                } else {
                    mono_frame
                };
                
                // Step 3: Convert to bytes
                let pcm_bytes: Vec<u8> = resampled.iter().flat_map(|s| s.to_le_bytes()).collect();
                
                // Step 4: Encode to Opus
                match encoder.encode(&pcm_bytes) {
                    Ok(opus_frame) => {
                        let count = frame_counter_clone.fetch_add(1, Ordering::SeqCst);
                        if count == 0 {
                            println!("[Audio] First frame captured and encoded!");
                        }
                        if count > 0 && count % 50 == 0 {
                            println!("[AudioCapture] Frames: {} ({:.1}s)", count, count as f32 * 0.02);
                        }
                        
                        if tokio_tx.try_send(opus_frame).is_err() {
                            println!("[AudioCapture] Channel full, dropping frame");
                        }
                    }
                    Err(e) => {
                        if frame_counter_clone.load(Ordering::SeqCst) == 0 {
                            println!("[AudioCapture] First encode error: {}", e);
                        }
                    }
                }
            }
            Err(std_mpsc::RecvTimeoutError::Timeout) => {
                // Normal timeout
            }
            Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                println!("[AudioCapture] Channel disconnected");
                break;
            }
        }
    }

    let total = frame_counter.load(Ordering::SeqCst);
    println!("[AudioCapture] Total frames: {}", total);
    println!("[Mic] Stopped. {} frames ({:.1}s)", total, total as f32 * 0.02);

    Ok(())
}
