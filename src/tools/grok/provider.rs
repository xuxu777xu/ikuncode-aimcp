use anyhow::{Context, Result};
use chrono::{Datelike, Local};
use rand::Rng;
use reqwest::Client;
use std::time::Duration;

use super::config::Config;
use super::prompts::{FETCH_PROMPT, SEARCH_PROMPT};

/// Chinese time-related keywords
const CN_TIME_KEYWORDS: &[&str] = &[
    "当前", "现在", "今天", "明天", "昨天",
    "本周", "上周", "下周", "这周",
    "本月", "上月", "下月", "这个月",
    "今年", "去年", "明年",
    "最新", "最近", "近期", "刚刚", "刚才",
    "实时", "即时", "目前",
];

/// English time-related keywords
const EN_TIME_KEYWORDS: &[&str] = &[
    "current", "now", "today", "tomorrow", "yesterday",
    "this week", "last week", "next week",
    "this month", "last month", "next month",
    "this year", "last year", "next year",
    "latest", "recent", "recently", "just now",
    "real-time", "realtime", "up-to-date",
];

/// Retryable HTTP status codes
const RETRYABLE_STATUS_CODES: &[u16] = &[408, 429, 500, 502, 503, 504];

/// Check if a query needs time context injection
pub fn needs_time_context(query: &str) -> bool {
    let query_lower = query.to_lowercase();

    for keyword in CN_TIME_KEYWORDS {
        if query.contains(keyword) {
            return true;
        }
    }

    for keyword in EN_TIME_KEYWORDS {
        if query_lower.contains(keyword) {
            return true;
        }
    }

    false
}

/// Get local time info string for injection into queries
fn get_local_time_info() -> String {
    let now = Local::now();
    let weekdays_cn = ["星期一", "星期二", "星期三", "星期四", "星期五", "星期六", "星期日"];
    let weekday = weekdays_cn[now.weekday().num_days_from_monday() as usize];

    format!(
        "[Current Time Context]\n- Date: {} ({})\n- Time: {}\n- Timezone: {}\n",
        now.format("%Y-%m-%d"),
        weekday,
        now.format("%H:%M:%S"),
        now.format("%Z"),
    )
}

/// Check if an HTTP status code is retryable
fn is_retryable_status(status: u16) -> bool {
    RETRYABLE_STATUS_CODES.contains(&status)
}

/// Parse Retry-After header value (seconds or HTTP date)
fn parse_retry_after(header_value: &str) -> Option<f64> {
    let header = header_value.trim();

    // Try parsing as integer seconds
    if let Ok(secs) = header.parse::<u64>() {
        return Some(secs as f64);
    }

    // Try parsing as HTTP date (RFC 2822)
    if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(header) {
        let now = chrono::Utc::now();
        let delay = (dt.signed_duration_since(now)).num_milliseconds() as f64 / 1000.0;
        return Some(delay.max(0.0));
    }

    None
}

/// Calculate exponential backoff with jitter
fn exponential_backoff_with_jitter(attempt: u32, multiplier: f64, max_wait: u64) -> f64 {
    let mut rng = rand::thread_rng();
    let base = multiplier * (2.0_f64.powi(attempt as i32));
    let jitter = rng.gen_range(0.0..base);
    let wait = base + jitter;
    wait.min(max_wait as f64)
}

pub struct GrokSearchProvider {
    client: Client,
    api_url: String,
    api_key: String,
    model: String,
}

impl GrokSearchProvider {
    pub fn new(api_url: String, api_key: String, model: String) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(6))
            .read_timeout(Duration::from_secs(120))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .unwrap_or_default();

        Self {
            client,
            api_url,
            api_key,
            model,
        }
    }

    /// Perform a web search via the Grok API
    pub async fn search(
        &self,
        query: &str,
        platform: &str,
        min_results: i32,
        max_results: i32,
    ) -> Result<String> {
        let mut platform_prompt = String::new();
        let mut return_prompt = String::new();

        if !platform.is_empty() {
            platform_prompt = format!(
                "\n\nYou should search the web for the information you need, and focus on these platform: {}",
                platform
            );
        }

        if max_results > 0 {
            return_prompt = format!(
                "\n\nYou should return the results in a JSON format, and the results should at least be {} and at most be {} results.",
                min_results, max_results
            );
        }

        // Inject time context only when query contains time-related keywords
        let time_context = if needs_time_context(query) {
            get_local_time_info() + "\n"
        } else {
            String::new()
        };

        let user_content = format!("{}{}{}{}", time_context, query, platform_prompt, return_prompt);

        let payload = serde_json::json!({
            "model": self.model,
            "messages": [
                {
                    "role": "system",
                    "content": SEARCH_PROMPT,
                },
                {
                    "role": "user",
                    "content": user_content,
                },
            ],
            "stream": true,
        });

        if Config::debug_enabled() {
            eprintln!("[grok] search payload user: {}", user_content);
        }

        self.execute_stream_with_retry(&payload).await
    }

    /// Fetch a URL's content via the Grok API
    pub async fn fetch(&self, url: &str) -> Result<String> {
        let user_content = format!("{}\n获取该网页内容并返回其结构化Markdown格式", url);

        let payload = serde_json::json!({
            "model": self.model,
            "messages": [
                {
                    "role": "system",
                    "content": FETCH_PROMPT,
                },
                {
                    "role": "user",
                    "content": user_content,
                },
            ],
            "stream": true,
        });

        self.execute_stream_with_retry(&payload).await
    }

    /// Test API connection by calling /models endpoint
    pub async fn test_connection(&self) -> Result<serde_json::Value> {
        let models_url = format!("{}/models", self.api_url.trim_end_matches('/'));
        let start = std::time::Instant::now();

        let response = self
            .client
            .get(&models_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .context("Failed to connect to API")?;

        let response_time = start.elapsed().as_millis();
        let status = response.status();

        if status.is_success() {
            let body: serde_json::Value = response.json().await.unwrap_or_default();
            let mut result = serde_json::json!({
                "status": "✅ Connected",
                "message": format!("Successfully retrieved model list (HTTP {})", status.as_u16()),
                "response_time_ms": response_time,
            });

            if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
                let model_count = data.len();
                result["message"] = serde_json::json!(format!(
                    "Successfully retrieved model list (HTTP {}), {} models",
                    status.as_u16(),
                    model_count
                ));
                let model_names: Vec<&str> = data
                    .iter()
                    .filter_map(|m| m.get("id").and_then(|id| id.as_str()))
                    .collect();
                if !model_names.is_empty() {
                    result["available_models"] = serde_json::json!(model_names);
                }
            }

            Ok(result)
        } else {
            let body = response.text().await.unwrap_or_default();
            Ok(serde_json::json!({
                "status": "⚠️ Connection error",
                "message": format!("HTTP {}: {}", status.as_u16(), &body[..body.len().min(100)]),
                "response_time_ms": response_time,
            }))
        }
    }

    /// Parse SSE streaming response, extracting content from delta chunks.
    /// Uses `response.chunk()` to read incrementally, avoiding hangs on keep-alive connections.
    /// Terminates on `data: [DONE]`, `finish_reason` != null, or connection close.
    async fn parse_streaming_response(&self, response: reqwest::Response) -> Result<String> {
        let mut content = String::new();
        let mut line_buf = String::new();
        let mut full_body_lines: Vec<String> = Vec::new();
        let mut finished = false;

        if Config::debug_enabled() {
            eprintln!("[grok] entering parse_streaming_response, reading chunks...");
        }

        let mut response = response;
        while let Some(chunk) = response.chunk().await.context("Failed to read SSE chunk")? {
            let chunk_str = String::from_utf8_lossy(&chunk);
            line_buf.push_str(&chunk_str);

            // Process complete lines from the buffer
            while let Some(newline_pos) = line_buf.find('\n') {
                let line = line_buf[..newline_pos].trim().to_string();
                line_buf = line_buf[newline_pos + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                full_body_lines.push(line.clone());

                // Handle SSE "data: {...}" and "data:{...}" formats
                if let Some(data_str) = line.strip_prefix("data:") {
                    let data_str = data_str.trim();
                    if data_str == "[DONE]" {
                        finished = true;
                        break;
                    }

                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(data_str) {
                        if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
                            if let Some(first) = choices.first() {
                                if let Some(delta_content) =
                                    first.get("delta").and_then(|d| d.get("content")).and_then(|c| c.as_str())
                                {
                                    content.push_str(delta_content);
                                }
                                // Check for finish_reason (some proxies don't send [DONE])
                                if first.get("finish_reason").and_then(|v| v.as_str()).is_some() {
                                    finished = true;
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            if finished {
                break;
            }
        }

        // Fallback: try parsing the entire body as non-streaming JSON
        if content.is_empty() && !full_body_lines.is_empty() {
            let full_text: String = full_body_lines.join("");
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(&full_text) {
                if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
                    if let Some(first) = choices.first() {
                        if let Some(msg_content) = first
                            .get("message")
                            .and_then(|m| m.get("content"))
                            .and_then(|c| c.as_str())
                        {
                            content = msg_content.to_string();
                        }
                    }
                }
            }
        }

        if Config::debug_enabled() {
            eprintln!("[grok] stream ended (finished={}), lines: {}, content length: {}",
                finished, full_body_lines.len(), content.len());
            if content.is_empty() && !full_body_lines.is_empty() {
                for (i, l) in full_body_lines.iter().take(5).enumerate() {
                    eprintln!("[grok] body line {}: {}", i, &l[..l.len().min(200)]);
                }
            }
        }

        Ok(content)
    }

    /// Execute a streaming HTTP request with retry logic
    async fn execute_stream_with_retry(&self, payload: &serde_json::Value) -> Result<String> {
        let max_attempts = Config::retry_max_attempts();
        let multiplier = Config::retry_multiplier();
        let max_wait = Config::retry_max_wait();
        let url = format!("{}/chat/completions", self.api_url.trim_end_matches('/'));

        let mut last_error: Option<anyhow::Error> = None;

        for attempt in 0..=max_attempts {
            if attempt > 0 {
                eprintln!("[grok] Retry attempt {}/{}", attempt, max_attempts);
            }

            match self
                .client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(payload)
                .send()
                .await
            {
                Ok(response) => {
                    let status = response.status();
                    if Config::debug_enabled() {
                        eprintln!("[grok] HTTP {} from {}", status.as_u16(), &url);
                    }
                    if status.is_success() {
                        return self.parse_streaming_response(response).await;
                    }

                    let status_code = status.as_u16();
                    if !is_retryable_status(status_code) || attempt == max_attempts {
                        let body = response.text().await.unwrap_or_default();
                        anyhow::bail!("API request failed with HTTP {}: {}", status_code, body);
                    }

                    // Check for Retry-After header on 429
                    let wait_secs = if status_code == 429 {
                        response
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(parse_retry_after)
                            .unwrap_or_else(|| exponential_backoff_with_jitter(attempt, multiplier, max_wait))
                    } else {
                        exponential_backoff_with_jitter(attempt, multiplier, max_wait)
                    };

                    eprintln!("[grok] Retryable error (HTTP {}), waiting {:.1}s", status_code, wait_secs);
                    tokio::time::sleep(Duration::from_secs_f64(wait_secs)).await;
                    last_error = Some(anyhow::anyhow!("HTTP {}", status_code));
                }
                Err(e) => {
                    // Network/timeout errors are retryable
                    if attempt == max_attempts {
                        return Err(e).context("API request failed after all retries");
                    }

                    let wait_secs = exponential_backoff_with_jitter(attempt, multiplier, max_wait);
                    eprintln!("[grok] Network error: {}, waiting {:.1}s", e, wait_secs);
                    tokio::time::sleep(Duration::from_secs_f64(wait_secs)).await;
                    last_error = Some(e.into());
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("All retry attempts exhausted")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_time_context_chinese() {
        assert!(needs_time_context("今天天气怎么样"));
        assert!(needs_time_context("最新的Rust版本"));
        assert!(needs_time_context("目前市场行情"));
        assert!(!needs_time_context("Rust语言教程"));
        assert!(!needs_time_context("如何写代码"));
    }

    #[test]
    fn test_needs_time_context_english() {
        assert!(needs_time_context("latest rust release"));
        assert!(needs_time_context("what happened today"));
        assert!(needs_time_context("Current weather"));
        assert!(needs_time_context("recent news"));
        assert!(!needs_time_context("how to write rust code"));
        assert!(!needs_time_context("rust programming tutorial"));
    }

    #[test]
    fn test_needs_time_context_mixed() {
        assert!(needs_time_context("最新 Rust release"));
        assert!(needs_time_context("latest Rust版本"));
        assert!(!needs_time_context("Rust programming 教程"));
    }

    #[test]
    fn test_get_local_time_info() {
        let info = get_local_time_info();
        assert!(info.contains("[Current Time Context]"));
        assert!(info.contains("Date:"));
        assert!(info.contains("Time:"));
        assert!(info.contains("Timezone:"));
        assert!(info.contains("星期"));
    }

    #[test]
    fn test_is_retryable_status() {
        assert!(is_retryable_status(429));
        assert!(is_retryable_status(500));
        assert!(is_retryable_status(502));
        assert!(is_retryable_status(503));
        assert!(is_retryable_status(504));
        assert!(is_retryable_status(408));
        assert!(!is_retryable_status(200));
        assert!(!is_retryable_status(400));
        assert!(!is_retryable_status(401));
        assert!(!is_retryable_status(403));
        assert!(!is_retryable_status(404));
    }

    #[test]
    fn test_parse_retry_after_seconds() {
        assert_eq!(parse_retry_after("5"), Some(5.0));
        assert_eq!(parse_retry_after("30"), Some(30.0));
        assert_eq!(parse_retry_after("  10  "), Some(10.0));
    }

    #[test]
    fn test_parse_retry_after_invalid() {
        assert_eq!(parse_retry_after("not-a-number"), None);
        assert_eq!(parse_retry_after(""), None);
    }

    #[test]
    fn test_exponential_backoff_with_jitter() {
        let wait = exponential_backoff_with_jitter(0, 1.0, 10);
        assert!(wait >= 0.0);
        assert!(wait <= 10.0);

        let wait = exponential_backoff_with_jitter(3, 1.0, 10);
        assert!(wait >= 0.0);
        assert!(wait <= 10.0);
    }

    #[test]
    fn test_exponential_backoff_respects_max() {
        for attempt in 0..10 {
            let wait = exponential_backoff_with_jitter(attempt, 1.0, 5);
            assert!(wait <= 5.0, "wait {} exceeded max 5 for attempt {}", wait, attempt);
        }
    }

    #[test]
    fn test_grok_provider_new() {
        let provider = GrokSearchProvider::new(
            "https://api.x.ai/v1".to_string(),
            "test-key".to_string(),
            "grok-4-fast".to_string(),
        );
        assert_eq!(provider.api_url, "https://api.x.ai/v1");
        assert_eq!(provider.api_key, "test-key");
        assert_eq!(provider.model, "grok-4-fast");
    }
}
