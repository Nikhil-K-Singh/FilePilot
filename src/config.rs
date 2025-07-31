use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub notification_endpoint: Option<String>,
    pub notification_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            notification_endpoint: None,
            notification_enabled: false,
        }
    }
}

impl Config {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: Config = serde_json::from_str(&content)?;
        Ok(config)
    }

    pub fn load_default() -> Self {
        // Try to load from src/config.json, fallback to default
        if let Ok(config) = Self::load_from_file("src/config.json") {
            config
        } else {
            Self::default()
        }
    }
}
