//! Credential Store
//!
//! Manages device credentials with optional encryption.

use anyhow::Result;
use std::path::PathBuf;

use crate::asr::{get_asr_token, register_device, DeviceCredentials};
use crate::data::AppConfig;

/// Credential store for managing device credentials
pub struct CredentialStore {
    credentials_path: PathBuf,
    credentials: Option<DeviceCredentials>,
}

impl CredentialStore {
    /// Create a new credential store
    pub fn new(_config: &AppConfig) -> Result<Self> {
        let credentials_path = AppConfig::credentials_path();

        // Try to load existing credentials
        let credentials = if credentials_path.exists() {
            DeviceCredentials::load(&credentials_path).ok()
        } else {
            None
        };

        Ok(Self {
            credentials_path,
            credentials,
        })
    }

    /// Ensure we have valid credentials
    pub async fn ensure_credentials(&self) -> Result<DeviceCredentials> {
        // Check if we have existing complete credentials
        if let Some(ref creds) = self.credentials {
            if creds.is_complete() {
                tracing::info!("Using cached credentials");
                return Ok(creds.clone());
            }
        }

        // Need to register device
        tracing::info!("Registering new device...");
        let mut creds = DeviceCredentials::new_generated();

        // Register device to get device_id
        register_device(&mut creds).await?;

        // Get ASR token
        get_asr_token(&mut creds).await?;

        // Save credentials
        creds.save(&self.credentials_path)?;
        tracing::info!("Credentials saved to {:?}", self.credentials_path);

        Ok(creds)
    }
}
