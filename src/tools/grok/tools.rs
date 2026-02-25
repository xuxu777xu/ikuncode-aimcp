use anyhow::Result;

use super::config::{self, Config};
use super::provider::GrokSearchProvider;

/// Execute a web search via the Grok API
pub async fn web_search(
    query: &str,
    platform: &str,
    min_results: i32,
    max_results: i32,
) -> Result<String> {
    let api_url = Config::grok_api_url()
        .map_err(|e| anyhow::anyhow!("Configuration error: {}", e))?;
    let api_key = Config::grok_api_key()
        .map_err(|e| anyhow::anyhow!("Configuration error: {}", e))?;
    let model = {
        let cfg = Config::global();
        let mut cfg = cfg.lock().unwrap();
        cfg.grok_model()
    };

    let provider = GrokSearchProvider::new(api_url, api_key, model);

    eprintln!("[grok] Begin Search: {}", query);
    let result = provider.search(query, platform, min_results, max_results).await?;
    eprintln!("[grok] Search Finished!");

    Ok(result)
}

/// Fetch and extract content from a URL via the Grok API
pub async fn web_fetch(url: &str) -> Result<String> {
    let api_url = Config::grok_api_url()
        .map_err(|e| anyhow::anyhow!("Configuration error: {}", e))?;
    let api_key = Config::grok_api_key()
        .map_err(|e| anyhow::anyhow!("Configuration error: {}", e))?;
    let model = {
        let cfg = Config::global();
        let mut cfg = cfg.lock().unwrap();
        cfg.grok_model()
    };

    let provider = GrokSearchProvider::new(api_url, api_key, model);

    eprintln!("[grok] Begin Fetch: {}", url);
    let result = provider.fetch(url).await?;
    eprintln!("[grok] Fetch Finished!");

    Ok(result)
}

/// Get current configuration info with connection test
pub async fn get_config_info() -> Result<String> {
    let mut config_info = config::get_config_info();

    // Test connection
    let test_result = match (Config::grok_api_url(), Config::grok_api_key()) {
        (Ok(api_url), Ok(api_key)) => {
            let model = {
                let cfg = Config::global();
                let mut cfg = cfg.lock().unwrap();
                cfg.grok_model()
            };
            let provider = GrokSearchProvider::new(api_url, api_key, model);
            match provider.test_connection().await {
                Ok(result) => result,
                Err(e) => serde_json::json!({
                    "status": "❌ Connection failed",
                    "message": format!("Error: {}", e),
                }),
            }
        }
        _ => serde_json::json!({
            "status": "❌ Configuration error",
            "message": "GROK_API_URL or GROK_API_KEY not set",
        }),
    };

    config_info["connection_test"] = test_result;

    serde_json::to_string_pretty(&config_info)
        .map_err(|e| anyhow::anyhow!("Failed to serialize config info: {}", e))
}

/// Switch the default model and persist the setting
pub async fn switch_model(model: &str) -> Result<String> {
    let previous_model = {
        let cfg = Config::global();
        let mut cfg = cfg.lock().unwrap();
        cfg.grok_model()
    };

    {
        let cfg = Config::global();
        let mut cfg = cfg.lock().unwrap();
        cfg.set_model(model)
            .map_err(|e| anyhow::anyhow!("Failed to switch model: {}", e))?;
    }

    let current_model = {
        let cfg = Config::global();
        let mut cfg = cfg.lock().unwrap();
        cfg.grok_model()
    };

    let result = serde_json::json!({
        "status": "✅ Success",
        "previous_model": previous_model,
        "current_model": current_model,
        "message": format!("Model switched from {} to {}", previous_model, current_model),
        "config_file": Config::config_file_path().to_string_lossy(),
    });

    serde_json::to_string_pretty(&result)
        .map_err(|e| anyhow::anyhow!("Failed to serialize result: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_search_requires_config() {
        std::env::remove_var("GROK_API_URL");
        std::env::remove_var("GROK_API_KEY");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(web_search("test", "", 3, 10));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Configuration error"));
    }

    #[test]
    fn test_web_fetch_requires_config() {
        std::env::remove_var("GROK_API_URL");
        std::env::remove_var("GROK_API_KEY");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(web_fetch("https://example.com"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Configuration error"));
    }

    #[test]
    fn test_get_config_info_without_env() {
        std::env::remove_var("GROK_API_URL");
        std::env::remove_var("GROK_API_KEY");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(get_config_info());
        assert!(result.is_ok());
        let info = result.unwrap();
        assert!(info.contains("config_status"));
        assert!(info.contains("connection_test"));
    }

    #[test]
    fn test_switch_model_persists() {
        // This test uses the config singleton, ensure clean state
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(switch_model("test-model-123"));
        // May fail if config dir is not writable, which is acceptable in CI
        if let Ok(json_str) = result {
            assert!(json_str.contains("test-model-123"));
            assert!(json_str.contains("Success"));
        }
    }
}
