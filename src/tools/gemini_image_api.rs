use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Request body for Gemini API generateContent
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateContentRequest {
    contents: Vec<RequestContent>,
    generation_config: GenerationConfig,
}

#[derive(Serialize)]
struct RequestContent {
    parts: Vec<RequestPart>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum RequestPart {
    Text { text: String },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    response_modalities: Vec<String>,
}

/// Response from Gemini API generateContent
#[derive(Deserialize)]
struct GenerateContentResponse {
    candidates: Option<Vec<Candidate>>,
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Option<CandidateContent>,
}

#[derive(Deserialize)]
struct CandidateContent {
    parts: Option<Vec<ResponsePart>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponsePart {
    text: Option<String>,
    inline_data: Option<InlineData>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InlineData {
    mime_type: String,
    data: String,
}

#[derive(Deserialize)]
struct ApiError {
    message: Option<String>,
    code: Option<serde_json::Value>,
}

/// Result of image generation
pub struct ImageGenerationResult {
    /// Text response from the model (if any)
    pub text: Option<String>,
    /// Generated images as (base64_data, mime_type) pairs
    pub images: Vec<(String, String)>,
}

/// Generate an image using the Gemini API directly (not via CLI).
///
/// # Arguments
/// * `api_url` - Base URL of the Gemini API (e.g. "https://api.ikuncode.cc")
/// * `api_key` - API key for authentication
/// * `model` - Model name (e.g. "gemini-3-pro-image-preview")
/// * `prompt` - Text prompt for image generation
pub async fn generate_image(
    api_url: &str,
    api_key: &str,
    model: &str,
    prompt: &str,
) -> Result<ImageGenerationResult> {
    let url = format!(
        "{}/v1beta/models/{}:generateContent",
        api_url.trim_end_matches('/'),
        model
    );

    let request_body = GenerateContentRequest {
        contents: vec![RequestContent {
            parts: vec![RequestPart::Text {
                text: prompt.to_string(),
            }],
        }],
        generation_config: GenerationConfig {
            response_modalities: vec!["IMAGE".to_string(), "TEXT".to_string()],
        },
    };

    let client = Client::new();
    let response = client
        .post(&url)
        .header("x-goog-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .context("Failed to send request to Gemini API")?;

    let status = response.status();
    let body = response
        .text()
        .await
        .context("Failed to read response body")?;

    if !status.is_success() {
        return Err(anyhow::anyhow!(
            "Gemini API returned HTTP {}: {}",
            status,
            body
        ));
    }

    let parsed: GenerateContentResponse =
        serde_json::from_str(&body).context("Failed to parse Gemini API response")?;

    if let Some(error) = parsed.error {
        return Err(anyhow::anyhow!(
            "Gemini API error: {}",
            error.message.unwrap_or_else(|| "Unknown error".to_string())
        ));
    }

    let mut result = ImageGenerationResult {
        text: None,
        images: Vec::new(),
    };

    if let Some(candidates) = parsed.candidates {
        for candidate in candidates {
            if let Some(content) = candidate.content {
                if let Some(parts) = content.parts {
                    for part in parts {
                        if let Some(inline_data) = part.inline_data {
                            result
                                .images
                                .push((inline_data.data, inline_data.mime_type));
                        }
                        if let Some(text) = part.text {
                            if !text.is_empty() {
                                result.text = Some(match result.text {
                                    Some(existing) => format!("{}\n{}", existing, text),
                                    None => text,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    if result.images.is_empty() && result.text.is_none() {
        return Err(anyhow::anyhow!(
            "Gemini API returned no content (no images or text)"
        ));
    }

    Ok(result)
}
