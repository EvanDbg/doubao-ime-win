//! Shared runtime settings with immediate save/apply support.

use anyhow::Result;
use std::sync::{Arc, RwLock};

use super::AppConfig;

#[derive(Clone)]
pub struct RuntimeSettings {
    config: Arc<RwLock<AppConfig>>,
}

impl RuntimeSettings {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
        }
    }

    pub fn get(&self) -> AppConfig {
        self.config.read().expect("settings lock poisoned").clone()
    }

    pub fn update(&self, config: AppConfig) -> Result<()> {
        config.save()?;
        *self.config.write().expect("settings lock poisoned") = config;
        Ok(())
    }
}
