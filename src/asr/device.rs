//! Device Registration and Token Management
//!
//! Implements the device registration flow to obtain device_id and ASR token.

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use super::constants::*;

/// Device credentials for ASR authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCredentials {
    pub device_id: String,
    pub install_id: String,
    pub cdid: String,
    pub openudid: String,
    pub clientudid: String,
    pub token: String,
}

impl DeviceCredentials {
    /// Create new credentials with generated IDs
    pub fn new_generated() -> Self {
        Self {
            device_id: String::new(),
            install_id: String::new(),
            cdid: Uuid::new_v4().to_string(),
            openudid: generate_openudid(),
            clientudid: Uuid::new_v4().to_string(),
            token: String::new(),
        }
    }

    /// Check if credentials are complete
    pub fn is_complete(&self) -> bool {
        !self.device_id.is_empty() && !self.token.is_empty()
    }

    /// Save credentials to file
    pub fn save(&self, path: &PathBuf) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load credentials from file
    pub fn load(path: &PathBuf) -> Result<Self> {
        let json = std::fs::read_to_string(path)?;
        let creds: DeviceCredentials = serde_json::from_str(&json)?;
        Ok(creds)
    }
}

/// Generate a random openudid (16 hex characters)
fn generate_openudid() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 8] = rng.gen();
    hex::encode(bytes)
}

/// Get current timestamp in milliseconds
fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Device register request header
#[derive(Debug, Serialize)]
struct DeviceRegisterHeader {
    device_id: u64,
    install_id: u64,
    aid: u32,
    app_name: String,
    version_code: u32,
    version_name: String,
    manifest_version_code: u32,
    update_version_code: u32,
    channel: String,
    package: String,
    device_platform: String,
    os: String,
    os_api: String,
    os_version: String,
    device_type: String,
    device_brand: String,
    device_model: String,
    resolution: String,
    dpi: String,
    language: String,
    timezone: i32,
    access: String,
    rom: String,
    rom_version: String,
    openudid: String,
    clientudid: String,
    cdid: String,
    region: String,
    tz_name: String,
    tz_offset: i32,
    sim_region: String,
    carrier_region: String,
    cpu_abi: String,
    build_serial: String,
    not_request_sender: i32,
    sig_hash: String,
    google_aid: String,
    mc: String,
    serial_number: String,
}

impl DeviceRegisterHeader {
    fn new(cdid: &str, openudid: &str, clientudid: &str) -> Self {
        Self {
            device_id: 0,
            install_id: 0,
            aid: AID,
            app_name: APP_NAME.to_string(),
            version_code: VERSION_CODE,
            version_name: VERSION_NAME.to_string(),
            manifest_version_code: VERSION_CODE,
            update_version_code: VERSION_CODE,
            channel: CHANNEL.to_string(),
            package: PACKAGE.to_string(),
            device_platform: DEVICE_PLATFORM.to_string(),
            os: OS.to_string(),
            os_api: OS_API.to_string(),
            os_version: OS_VERSION.to_string(),
            device_type: DEVICE_TYPE.to_string(),
            device_brand: DEVICE_BRAND.to_string(),
            device_model: DEVICE_MODEL.to_string(),
            resolution: RESOLUTION.to_string(),
            dpi: DPI.to_string(),
            language: LANGUAGE.to_string(),
            timezone: TIMEZONE,
            access: ACCESS.to_string(),
            rom: ROM.to_string(),
            rom_version: ROM_VERSION.to_string(),
            openudid: openudid.to_string(),
            clientudid: clientudid.to_string(),
            cdid: cdid.to_string(),
            region: "CN".to_string(),
            tz_name: "Asia/Shanghai".to_string(),
            tz_offset: 28800,
            sim_region: "cn".to_string(),
            carrier_region: "cn".to_string(),
            cpu_abi: "arm64-v8a".to_string(),
            build_serial: "unknown".to_string(),
            not_request_sender: 0,
            sig_hash: String::new(),
            google_aid: String::new(),
            mc: String::new(),
            serial_number: String::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct DeviceRegisterBody {
    magic_tag: String,
    header: DeviceRegisterHeader,
    #[serde(rename = "_gen_time")]
    gen_time: u64,
}

#[derive(Debug, Deserialize)]
struct DeviceRegisterResponse {
    device_id: u64,
    install_id: u64,
    #[allow(dead_code)]
    device_id_str: Option<String>,
    #[allow(dead_code)]
    install_id_str: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SettingsResponse {
    data: SettingsData,
    #[allow(dead_code)]
    message: String,
}

#[derive(Debug, Deserialize)]
struct SettingsData {
    settings: Settings,
}

#[derive(Debug, Deserialize)]
struct Settings {
    asr_config: AsrConfig,
}

#[derive(Debug, Deserialize)]
struct AsrConfig {
    app_key: String,
}

/// Register a new device and get device_id
pub async fn register_device(creds: &mut DeviceCredentials) -> Result<()> {
    let client = Client::new();

    let header = DeviceRegisterHeader::new(&creds.cdid, &creds.openudid, &creds.clientudid);
    let body = DeviceRegisterBody {
        magic_tag: "ss_app_log".to_string(),
        header,
        gen_time: current_time_ms(),
    };

    // Build query params
    let mut params: HashMap<&str, String> = HashMap::new();
    params.insert("device_platform", DEVICE_PLATFORM.to_string());
    params.insert("os", OS.to_string());
    params.insert("ssmix", "a".to_string());
    params.insert("_rticket", current_time_ms().to_string());
    params.insert("cdid", creds.cdid.clone());
    params.insert("channel", CHANNEL.to_string());
    params.insert("aid", AID.to_string());
    params.insert("app_name", APP_NAME.to_string());
    params.insert("version_code", VERSION_CODE.to_string());
    params.insert("version_name", VERSION_NAME.to_string());
    params.insert("manifest_version_code", VERSION_CODE.to_string());
    params.insert("update_version_code", VERSION_CODE.to_string());
    params.insert("resolution", RESOLUTION.to_string());
    params.insert("dpi", DPI.to_string());
    params.insert("device_type", DEVICE_TYPE.to_string());
    params.insert("device_brand", DEVICE_BRAND.to_string());
    params.insert("language", LANGUAGE.to_string());
    params.insert("os_api", OS_API.to_string());
    params.insert("os_version", OS_VERSION.to_string());
    params.insert("ac", "wifi".to_string());

    let response = client
        .post(REGISTER_URL)
        .header("User-Agent", USER_AGENT)
        .query(&params)
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Device registration failed: {}",
            response.status()
        ));
    }

    let result: DeviceRegisterResponse = response.json().await?;

    if result.device_id == 0 {
        return Err(anyhow!("Device registration returned invalid device_id"));
    }

    creds.device_id = result.device_id.to_string();
    creds.install_id = result.install_id.to_string();

    tracing::info!("Device registered: device_id={}", creds.device_id);
    Ok(())
}

/// Get ASR token using device_id
pub async fn get_asr_token(creds: &mut DeviceCredentials) -> Result<()> {
    let client = Client::new();

    let mut params: HashMap<&str, String> = HashMap::new();
    params.insert("device_platform", DEVICE_PLATFORM.to_string());
    params.insert("os", OS.to_string());
    params.insert("ssmix", "a".to_string());
    params.insert("_rticket", current_time_ms().to_string());
    params.insert("cdid", creds.cdid.clone());
    params.insert("channel", CHANNEL.to_string());
    params.insert("aid", AID.to_string());
    params.insert("app_name", APP_NAME.to_string());
    params.insert("version_code", VERSION_CODE.to_string());
    params.insert("version_name", VERSION_NAME.to_string());
    params.insert("device_id", creds.device_id.clone());

    // Body is "body=null"
    let body_str = "body=null";
    let x_ss_stub = format!("{:X}", md5::compute(body_str.as_bytes()));

    let response = client
        .post(SETTINGS_URL)
        .header("User-Agent", USER_AGENT)
        .header("x-ss-stub", x_ss_stub)
        .query(&params)
        .body(body_str)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!("Failed to get ASR token: {}", response.status()));
    }

    let result: SettingsResponse = response.json().await?;
    creds.token = result.data.settings.asr_config.app_key;

    tracing::info!("ASR token obtained successfully");
    Ok(())
}
