use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;

const DEFAULT_MODEL: &str = "grok-4-fast";

static CONFIG: OnceLock<Mutex<Config>> = OnceLock::new();

#[derive(Debug, Serialize, Deserialize, Default)]
struct ConfigFile {
    #[serde(default)]
    model: Option<String>,
}

pub struct Config {
    cached_model: Option<String>,
}

impl Config {
    fn new() -> Self {
        Self {
            cached_model: None,
        }
    }

    /// Get the global config singleton
    pub fn global() -> &'static Mutex<Config> {
        CONFIG.get_or_init(|| Mutex::new(Config::new()))
    }

    /// Path to the config file: ~/.config/grok-search/config.json
    pub fn config_file_path() -> PathBuf {
        let config_dir = if cfg!(target_os = "windows") {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("grok-search")
        } else {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".config")
                .join("grok-search")
        };
        let _ = std::fs::create_dir_all(&config_dir);
        config_dir.join("config.json")
    }

    fn load_config_file() -> ConfigFile {
        let path = Self::config_file_path();
        if !path.exists() {
            return ConfigFile::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => ConfigFile::default(),
        }
    }

    fn save_config_file(config_data: &ConfigFile) -> Result<(), String> {
        let path = Self::config_file_path();
        let content = serde_json::to_string_pretty(config_data)
            .map_err(|e| format!("Failed to serialize config: {e}"))?;
        std::fs::write(&path, content)
            .map_err(|e| format!("Failed to save config file: {e}"))?;
        Ok(())
    }

    /// Read GROK_API_URL from environment
    pub fn grok_api_url() -> Result<String, String> {
        std::env::var("GROK_API_URL").map_err(|_| {
            "GROK_API_URL not set. Please configure the environment variable.".to_string()
        })
    }

    /// Read GROK_API_KEY from environment
    pub fn grok_api_key() -> Result<String, String> {
        std::env::var("GROK_API_KEY").map_err(|_| {
            "GROK_API_KEY not set. Please configure the environment variable.".to_string()
        })
    }

    /// Get the current model: env > config file > default
    pub fn grok_model(&mut self) -> String {
        if let Some(ref cached) = self.cached_model {
            return cached.clone();
        }

        // Check env override first
        if let Ok(env_model) = std::env::var("GROK_MODEL") {
            let env_model = env_model.trim().to_string();
            if !env_model.is_empty() {
                self.cached_model = Some(env_model.clone());
                return env_model;
            }
        }

        let config_data = Self::load_config_file();
        if let Some(file_model) = config_data.model {
            self.cached_model = Some(file_model.clone());
            return file_model;
        }

        self.cached_model = Some(DEFAULT_MODEL.to_string());
        DEFAULT_MODEL.to_string()
    }

    /// Set the model and persist to config file
    pub fn set_model(&mut self, model: &str) -> Result<(), String> {
        let mut config_data = Self::load_config_file();
        config_data.model = Some(model.to_string());
        Self::save_config_file(&config_data)?;
        self.cached_model = Some(model.to_string());
        Ok(())
    }

    /// Check if debug mode is enabled
    pub fn debug_enabled() -> bool {
        std::env::var("GROK_DEBUG")
            .map(|v| matches!(v.to_lowercase().as_str(), "true" | "1" | "yes"))
            .unwrap_or(false)
    }

    /// Maximum retry attempts
    pub fn retry_max_attempts() -> u32 {
        std::env::var("GROK_RETRY_MAX_ATTEMPTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3)
    }

    /// Retry backoff multiplier
    pub fn retry_multiplier() -> f64 {
        std::env::var("GROK_RETRY_MULTIPLIER")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.0)
    }

    /// Maximum retry wait in seconds
    pub fn retry_max_wait() -> u64 {
        std::env::var("GROK_RETRY_MAX_WAIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10)
    }
}

/// Mask API key for display: show first/last 4 chars
pub fn mask_api_key(key: &str) -> String {
    if key.len() <= 8 {
        return "***".to_string();
    }
    format!(
        "{}{}{}",
        &key[..4],
        "*".repeat(key.len() - 8),
        &key[key.len() - 4..]
    )
}

/// Get masked config info as a JSON value
pub fn get_config_info() -> serde_json::Value {
    let (api_url, api_key_masked, config_status) = match (Config::grok_api_url(), Config::grok_api_key()) {
        (Ok(url), Ok(key)) => {
            let masked = mask_api_key(&key);
            (url, masked, "✅ Configuration complete".to_string())
        }
        (Err(e), _) | (_, Err(e)) => {
            ("Not configured".to_string(), "Not configured".to_string(), format!("❌ Configuration error: {e}"))
        }
    };

    let model = {
        let config = Config::global();
        let mut cfg = config.lock().unwrap();
        cfg.grok_model()
    };

    serde_json::json!({
        "GROK_API_URL": api_url,
        "GROK_API_KEY": api_key_masked,
        "GROK_MODEL": model,
        "GROK_DEBUG": Config::debug_enabled(),
        "config_status": config_status
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_api_key_short() {
        assert_eq!(mask_api_key("1234"), "***");
        assert_eq!(mask_api_key(""), "***");
        assert_eq!(mask_api_key("12345678"), "***");
    }

    #[test]
    fn test_mask_api_key_normal() {
        assert_eq!(mask_api_key("abcd1234efgh5678"), "abcd********5678");
    }

    #[test]
    fn test_mask_api_key_nine_chars() {
        assert_eq!(mask_api_key("123456789"), "1234*6789");
    }

    #[test]
    fn test_default_model() {
        assert_eq!(DEFAULT_MODEL, "grok-4-fast");
    }

    #[test]
    fn test_debug_disabled_by_default() {
        // Remove env var if set
        std::env::remove_var("GROK_DEBUG");
        assert!(!Config::debug_enabled());
    }

    #[test]
    fn test_retry_defaults() {
        std::env::remove_var("GROK_RETRY_MAX_ATTEMPTS");
        std::env::remove_var("GROK_RETRY_MULTIPLIER");
        std::env::remove_var("GROK_RETRY_MAX_WAIT");
        assert_eq!(Config::retry_max_attempts(), 3);
        assert!((Config::retry_multiplier() - 1.0).abs() < f64::EPSILON);
        assert_eq!(Config::retry_max_wait(), 10);
    }

    #[test]
    fn test_config_file_path_is_valid() {
        let path = Config::config_file_path();
        assert!(path.to_string_lossy().contains("grok-search"));
        assert!(path.to_string_lossy().ends_with("config.json"));
    }
}
