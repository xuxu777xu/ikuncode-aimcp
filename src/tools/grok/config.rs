use std::sync::Mutex;
use std::sync::OnceLock;

const DEFAULT_MODEL: &str = "grok-4-fast";

static CONFIG: OnceLock<Mutex<Config>> = OnceLock::new();

pub struct Config {
    cached_model: Option<String>,
}

impl Config {
    fn new() -> Self {
        Self { cached_model: None }
    }

    /// Get the global config singleton
    pub fn global() -> &'static Mutex<Config> {
        CONFIG.get_or_init(|| Mutex::new(Config::new()))
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

    /// Get the current model: env > cached > default
    pub fn grok_model(&mut self) -> String {
        // Check env override first (always takes priority)
        if let Ok(env_model) = std::env::var("GROK_MODEL") {
            let env_model = env_model.trim().to_string();
            if !env_model.is_empty() {
                self.cached_model = Some(env_model.clone());
                return env_model;
            }
        }

        if let Some(ref cached) = self.cached_model {
            return cached.clone();
        }

        self.cached_model = Some(DEFAULT_MODEL.to_string());
        DEFAULT_MODEL.to_string()
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

/// Get config info as a JSON value (API key is NOT included)
pub fn get_config_info() -> serde_json::Value {
    let (api_url, config_status) = match Config::grok_api_url() {
        Ok(url) => {
            if Config::grok_api_key().is_ok() {
                (url, "✅ Configuration complete".to_string())
            } else {
                (url, "❌ GROK_API_KEY not set".to_string())
            }
        }
        Err(e) => (
            "Not configured".to_string(),
            format!("❌ Configuration error: {e}"),
        ),
    };

    let model = {
        let config = Config::global();
        let mut cfg = config.lock().unwrap();
        cfg.grok_model()
    };

    serde_json::json!({
        "GROK_API_URL": api_url,
        "GROK_MODEL": model,
        "GROK_DEBUG": Config::debug_enabled(),
        "config_status": config_status
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
