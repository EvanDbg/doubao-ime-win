//! Register a temporary Doubao IME device, fetch its ASR token, and verify
//! whether the token can create an ASR session.
//!
//! The temporary credentials stay in memory and the complete token is never
//! printed. Pass an existing credentials.json path to compare token identity:
//!
//! cargo run --example token_probe -- "D:\path\to\credentials.json"

use anyhow::{Context, Result};
use doubao_voice_input::asr::{
    get_asr_token, register_device, AsrClient, DeviceCredentials, ResponseType,
};
use doubao_voice_input::data::AudioQuality;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    doubao_voice_input::init_crypto_provider();

    println!("1/3 Registering a temporary device...");
    let mut fresh = DeviceCredentials::new_generated();
    register_device(&mut fresh)
        .await
        .context("temporary device registration failed")?;
    println!("    device_id suffix: {}", suffix(&fresh.device_id, 6));

    println!("2/3 Fetching a fresh ASR token...");
    get_asr_token(&mut fresh)
        .await
        .context("fresh ASR token request failed")?;
    println!("    token fingerprint: {}", fingerprint(&fresh.token));
    println!("    token length: {}", fresh.token.len());

    if let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) {
        let existing = DeviceCredentials::load(&path)
            .with_context(|| format!("could not load existing credentials from {path:?}"))?;
        println!(
            "    matches existing token: {}",
            if fresh.token == existing.token {
                "YES"
            } else {
                "NO"
            }
        );
    }

    println!("3/3 Creating an empty ASR session with the fresh token...");
    let client = AsrClient::new(fresh);
    let (audio_tx, audio_rx) = mpsc::channel(1);
    drop(audio_tx);

    let mut responses = match client
        .start_realtime(audio_rx, AudioQuality::Standard, 800)
        .await
    {
        Ok(responses) => responses,
        Err(error) => {
            println!("    RESULT: token was accepted, but session creation failed");
            println!("    server response: {error:#}");
            return Ok(());
        }
    };

    let outcome = tokio::time::timeout(Duration::from_secs(15), async {
        while let Some(response) = responses.recv().await {
            match response.response_type {
                ResponseType::SessionFinished => return "session started and finished normally",
                ResponseType::Error => return "session started, then the server returned an error",
                _ => {}
            }
        }
        "session started, then the response stream closed"
    })
    .await
    .unwrap_or("session started, but did not finish within 15 seconds");

    println!("    RESULT: {outcome}");
    Ok(())
}

fn fingerprint(token: &str) -> String {
    format!("{:x}", md5::compute(token.as_bytes()))[..12].to_string()
}

fn suffix(value: &str, count: usize) -> &str {
    value
        .get(value.len().saturating_sub(count)..)
        .unwrap_or(value)
}
