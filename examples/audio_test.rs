//! Simple audio test - run with: cargo run --example audio_test

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn main() {
    println!("=== Audio Capture Test ===");
    println!();

    // Initialize COM on Windows
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        println!("[COM] Initialized");
    }

    // Get default host
    let host = cpal::default_host();
    println!("[Host] {:?}", host.id());

    // List ALL input devices
    println!();
    println!("[Devices] Enumerating ALL input devices:");
    let mut devices: Vec<_> = Vec::new();
    
    match host.input_devices() {
        Ok(device_iter) => {
            for (i, device) in device_iter.enumerate() {
                let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
                println!("  [{}] {}", i, name);
                
                // Show supported configs
                if let Ok(configs) = device.supported_input_configs() {
                    for config in configs.take(2) {
                        println!("      {:?}", config);
                    }
                }
                
                devices.push(device);
            }
        }
        Err(e) => {
            println!("  Error: {}", e);
        }
    }

    println!();
    println!("[Total] Found {} input device(s)", devices.len());

    // Check default device
    println!();
    match host.default_input_device() {
        Some(device) => {
            println!("[Default] {}", device.name().unwrap_or_default());
        }
        None => {
            println!("[Default] NONE - no default input device set!");
            println!();
            println!(">>> Please set a default recording device in Windows Sound Settings <<<");
            println!("    Right-click speaker icon -> Sound settings -> Input");
        }
    }

    if devices.is_empty() {
        println!();
        println!("[ERROR] No input devices found at all!");
        println!("Please check:");
        println!("  1. Microphone is physically connected");
        println!("  2. Microphone drivers are installed");
        println!("  3. Microphone is enabled in Device Manager");
        return;
    }

    // Try to use first available device
    println!();
    println!("[Test] Attempting to use first available device...");
    let device = &devices[0];
    println!("[Using] {}", device.name().unwrap_or_default());

    let config = match device.default_input_config() {
        Ok(c) => {
            println!("[Config] {:?}", c);
            c
        }
        Err(e) => {
            println!("[ERROR] Could not get config: {}", e);
            return;
        }
    };

    let sample_count = Arc::new(AtomicU64::new(0));
    let sample_count_clone = sample_count.clone();

    println!("[Stream] Building...");

    let stream = device.build_input_stream(
        &config.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            sample_count_clone.fetch_add(data.len() as u64, Ordering::Relaxed);
        },
        |err| {
            println!("[ERROR] Stream error: {}", err);
        },
        None,
    );

    let stream = match stream {
        Ok(s) => {
            println!("[Stream] Built OK");
            s
        }
        Err(e) => {
            println!("[ERROR] Build failed: {}", e);
            return;
        }
    };

    if let Err(e) = stream.play() {
        println!("[ERROR] Play failed: {}", e);
        return;
    }

    println!();
    println!("[Recording] 3 seconds...");
    println!();

    for i in 0..6 {
        std::thread::sleep(Duration::from_millis(500));
        let count = sample_count.load(Ordering::Relaxed);
        println!("  [{:.1}s] Samples: {}", (i + 1) as f32 * 0.5, count);
    }

    println!();
    let final_count = sample_count.load(Ordering::Relaxed);
    
    if final_count > 0 {
        println!("[SUCCESS] Captured {} samples!", final_count);
        println!();
        println!("Audio capture is WORKING!");
    } else {
        println!("[FAILURE] No samples received!");
        println!();
        println!("Possible issues:");
        println!("  1. Microphone is muted");
        println!("  2. Microphone volume is zero");
        println!("  3. Another app has exclusive access");
    }
}
