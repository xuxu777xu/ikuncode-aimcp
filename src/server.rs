use crate::detection::Capabilities;
use crate::tools::codex::{self, SandboxPolicy};
use crate::tools::gemini;
use crate::tools::gemini_image_api;
use crate::tools::grok;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars,
    service::NotificationContext,
    tool, tool_handler, tool_router, ErrorData as McpError, RoleServer, ServerHandler,
};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::shared::{MAX_TIMEOUT_SECS, MIN_TIMEOUT_SECS};

// ---------------------------------------------------------------------------
// PathBuf serde helpers (from codex-mcp-rs)
// ---------------------------------------------------------------------------

mod serialize_as_os_string {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::path::{Path, PathBuf};

    #[allow(dead_code)]
    pub fn serialize<S>(path: &Path, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match path.to_str() {
            Some(s) => s.serialize(serializer),
            None => Err(serde::ser::Error::custom("path contains invalid UTF-8")),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <String as Deserialize>::deserialize(deserializer)?;
        Ok(PathBuf::from(s))
    }
}

mod serialize_as_os_string_vec {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::path::PathBuf;

    #[allow(dead_code)]
    pub fn serialize<S>(paths: &Vec<PathBuf>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(paths.len()))?;
        for path in paths {
            match path.to_str() {
                Some(s) => seq.serialize_element(s)?,
                None => return Err(serde::ser::Error::custom("path contains invalid UTF-8")),
            }
        }
        seq.end()
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<PathBuf>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let vec_strings = <Vec<String> as Deserialize>::deserialize(deserializer)?;
        Ok(vec_strings.into_iter().map(PathBuf::from).collect())
    }
}

// ---------------------------------------------------------------------------
// Tool argument structs
// ---------------------------------------------------------------------------

/// Input parameters for gemini tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GeminiArgs {
    /// Instruction for the task to send to gemini
    #[serde(rename = "PROMPT")]
    pub prompt: String,
    /// Run in sandbox mode. Defaults to `False`
    #[serde(default)]
    pub sandbox: bool,
    /// Resume the specified session of the gemini. If not provided or empty, starts a new session
    #[serde(rename = "SESSION_ID", default)]
    pub session_id: Option<String>,
    /// Return all messages (e.g. reasoning, tool calls, etc.) from the gemini session. Set to `False` by default, only the agent's final reply message is returned
    #[serde(default)]
    pub return_all_messages: bool,
    /// The model to use for the gemini session. If not specified, uses GEMINI_FORCE_MODEL
    /// environment variable or the Gemini CLI default
    #[serde(default)]
    pub model: Option<String>,
    /// Timeout in seconds for gemini execution (1-3600). If not specified, uses GEMINI_DEFAULT_TIMEOUT
    /// environment variable or falls back to 600 seconds (10 minutes).
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// Input parameters for gemini_image tool (image generation via Gemini CLI)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GeminiImageArgs {
    /// Instruction for the image generation task to send to gemini
    #[serde(rename = "PROMPT")]
    pub prompt: String,
    /// Run in sandbox mode. Defaults to `False`
    #[serde(default)]
    pub sandbox: bool,
    /// Resume the specified session of the gemini. If not provided or empty, starts a new session
    #[serde(rename = "SESSION_ID", default)]
    pub session_id: Option<String>,
    /// Return all messages (e.g. reasoning, tool calls, etc.) from the gemini session. Set to `False` by default, only the agent's final reply message is returned
    #[serde(default)]
    pub return_all_messages: bool,
    /// The model to use for image generation. If not specified, uses GEMINI_IMAGE_MODEL
    /// environment variable or the Gemini CLI default
    #[serde(default)]
    pub model: Option<String>,
    /// Timeout in seconds for gemini execution (1-3600). If not specified, uses GEMINI_DEFAULT_TIMEOUT
    /// environment variable or falls back to 600 seconds (10 minutes).
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// Directory to save the generated image. If not specified, uses the first MCP workspace root or current working directory.
    #[serde(default)]
    pub output_dir: Option<String>,
    /// Aspect ratio of the generated image. Supported values: "1:1", "2:3", "3:2", "3:4", "4:3", "4:5", "5:4", "9:16", "16:9", "21:9". If not specified, the model chooses automatically.
    #[serde(default)]
    pub aspect_ratio: Option<String>,
    /// Resolution/size of the generated image. Supported values: "1K", "2K", "4K". 4K is only available for certain models. If not specified, defaults to model default.
    #[serde(default)]
    pub image_size: Option<String>,
}

/// Input parameters for codex tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CodexArgs {
    /// Instruction for task to send to codex
    #[serde(rename = "PROMPT")]
    pub prompt: String,
    /// Set the workspace root for codex before executing the task
    #[serde(
        serialize_with = "serialize_as_os_string::serialize",
        deserialize_with = "serialize_as_os_string::deserialize"
    )]
    pub cd: PathBuf,
    /// Sandbox policy for model-generated commands. Defaults to 'read-only'
    #[serde(default)]
    pub sandbox: SandboxPolicy,
    /// Resume the specified session of the codex. Defaults to None, start a new session
    #[serde(rename = "SESSION_ID", default)]
    pub session_id: Option<String>,
    /// Allow codex running outside a Git repository (useful for one-off directories)
    #[serde(default)]
    pub skip_git_repo_check: bool,
    /// Return all messages (e.g. reasoning, tool calls, etc.) from the codex session
    #[serde(default)]
    pub return_all_messages: bool,
    /// Maximum number of messages to keep when return_all_messages is true (default: 10000)
    #[serde(default)]
    pub return_all_messages_limit: Option<usize>,
    /// Attach one or more image files to the initial prompt
    #[serde(
        serialize_with = "serialize_as_os_string_vec::serialize",
        deserialize_with = "serialize_as_os_string_vec::deserialize"
    )]
    pub image: Vec<PathBuf>,
    /// The model to use for the codex session
    #[serde(default)]
    pub model: Option<String>,
    /// Run every command without approvals or sandboxing
    #[serde(default)]
    pub yolo: bool,
    /// Configuration profile name to load from '~/.codex/config.toml'
    #[serde(default)]
    pub profile: Option<String>,
    /// Timeout in seconds for codex execution. If not specified, uses CODEX_DEFAULT_TIMEOUT
    /// environment variable or falls back to 600 seconds (10 minutes). Max: 3600 seconds.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// Force using stdin to pass the prompt to the codex process, bypassing the auto-detection.
    /// Default: false. When true, the prompt is always piped via stdin regardless of content.
    #[serde(default)]
    pub force_stdin: bool,
}

fn default_min_results() -> i32 {
    3
}
fn default_max_results() -> i32 {
    10
}

/// Input parameters for web_search tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WebSearchArgs {
    /// Clear, self-contained natural-language search query. When helpful, include constraints such as topic, time range, language, or domain.
    pub query: String,
    /// Platforms to focus on searching, such as "Twitter", "GitHub", "Reddit", etc.
    #[serde(default)]
    pub platform: Option<String>,
    /// Minimum number of results to return
    #[serde(default = "default_min_results")]
    pub min_results: i32,
    /// Maximum number of results to return
    #[serde(default = "default_max_results")]
    pub max_results: i32,
}

/// Input parameters for web_fetch tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WebFetchArgs {
    /// A valid HTTP/HTTPS web address pointing to the target page
    pub url: String,
}

// ---------------------------------------------------------------------------
// Codex security configuration (ported from codex-mcp-rs)
// ---------------------------------------------------------------------------

type SecurityConfig = codex::SecurityConfig;
type CodexOutput = codex::CodexOutput;

fn get_security_config(warnings: &mut Vec<String>) -> SecurityConfig {
    codex::get_security_config(warnings)
}

// ---------------------------------------------------------------------------
// Codex timeout resolution (ported from codex-mcp-rs)
// ---------------------------------------------------------------------------

fn get_default_timeout_with_warning() -> codex::DefaultTimeoutResult {
    codex::get_default_timeout_with_warning()
}

// ---------------------------------------------------------------------------
// Codex output formatting (ported from codex-mcp-rs)
// ---------------------------------------------------------------------------

fn merge_warnings(
    security_warnings: Vec<String>,
    result_warnings: Option<String>,
) -> Option<String> {
    codex::merge_warnings(security_warnings, result_warnings)
}

fn attach_warnings(error_msg: String, warnings: Option<String>) -> String {
    codex::attach_warnings(error_msg, warnings)
}

fn build_codex_output(
    result: &codex::CodexResult,
    return_all_messages: bool,
    warnings: Option<String>,
) -> CodexOutput {
    codex::build_codex_output(result, return_all_messages, warnings)
}

fn apply_security_restrictions(
    mut args: CodexArgs,
    security: &SecurityConfig,
) -> (CodexArgs, Vec<String>) {
    let mut sandbox = args.sandbox.clone();
    let mut yolo = args.yolo;
    let mut skip_git_repo_check = args.skip_git_repo_check;
    let warnings = codex::apply_security_restrictions(
        &mut sandbox,
        &mut yolo,
        &mut skip_git_repo_check,
        security,
    );
    args.sandbox = sandbox;
    args.yolo = yolo;
    args.skip_git_repo_check = skip_git_repo_check;
    (args, warnings)
}

// ---------------------------------------------------------------------------
// URI helpers
// ---------------------------------------------------------------------------

/// Convert a `file://` URI to a local [`PathBuf`].
///
/// Handles both Unix (`file:///home/user`) and Windows (`file:///D:/path`)
/// forms.  Returns `None` for non-file URIs or malformed strings.
fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let path_str = uri.strip_prefix("file://")?;
    // On Windows, file:///D:/foo → strip the leading '/' before the drive letter
    #[cfg(windows)]
    let path_str = path_str
        .strip_prefix('/')
        .filter(|s| s.chars().nth(1) == Some(':'))
        .unwrap_or(path_str);
    if path_str.is_empty() {
        return None;
    }
    Some(PathBuf::from(path_str))
}

// ---------------------------------------------------------------------------
// UnifiedServer
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct UnifiedServer {
    tool_router: ToolRouter<UnifiedServer>,
    capabilities: Capabilities,
    /// MCP client workspace roots, populated during on_initialized via roots/list request.
    /// Passed to Gemini CLI as --include-directories to allow file access beyond CWD.
    roots: Arc<RwLock<Vec<PathBuf>>>,
}

impl UnifiedServer {
    pub fn new(capabilities: Capabilities) -> Self {
        Self {
            tool_router: Self::tool_router(),
            capabilities,
            roots: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Shared helper for running Gemini CLI and formatting the result.
    /// Used by both `gemini` and `gemini_image` tools.
    async fn run_gemini(
        opts: gemini::Options,
        return_all_messages: bool,
    ) -> Result<CallToolResult, McpError> {
        let result = match gemini::run(opts).await {
            Ok(r) => r,
            Err(e) => {
                return Err(McpError::internal_error(
                    format!("Failed to execute gemini: {}", e),
                    None,
                ));
            }
        };

        if result.success {
            let mut response_text = format!(
                "success: true\nSESSION_ID: {}\nagent_messages: {}",
                result.session_id, result.agent_messages
            );

            if return_all_messages && !result.all_messages.is_empty() {
                response_text.push_str(&format!(
                    "\nall_messages: {} events captured",
                    result.all_messages.len()
                ));
                if let Ok(json) = serde_json::to_string_pretty(&result.all_messages) {
                    response_text.push_str(&format!("\n\nFull event log:\n{}", json));
                }
            }

            Ok(CallToolResult::success(vec![Content::text(response_text)]))
        } else {
            let mut error_msg = result.error.unwrap_or_else(|| "Unknown error".to_string());

            if return_all_messages && !result.all_messages.is_empty() {
                error_msg.push_str(&format!(
                    "\n\nCaptured {} events before failure:",
                    result.all_messages.len()
                ));
                if let Ok(json) = serde_json::to_string_pretty(&result.all_messages) {
                    error_msg.push_str(&format!("\n{}", json));
                }
            }

            Err(McpError::internal_error(error_msg, None))
        }
    }
}

#[tool_router]
impl UnifiedServer {
    /// Invokes the Gemini CLI to execute AI-driven tasks, returning structured JSON events and a session identifier for conversation continuity.
    ///
    /// **Return structure:**
    /// - `success`: boolean indicating execution status
    /// - `SESSION_ID`: unique identifier for resuming this conversation in future calls
    /// - `agent_messages`: concatenated assistant response text
    /// - `all_messages`: (optional) complete array of JSON events when `return_all_messages=True`
    /// - `error`: error description when `success=False`
    ///
    /// **Best practices:**
    /// - Always capture and reuse `SESSION_ID` for multi-turn interactions
    /// - Enable `sandbox` mode when file modifications should be isolated
    /// - Use `return_all_messages` only when detailed execution traces are necessary (increases payload size)
    #[tool(
        name = "gemini",
        description = "Invokes the Gemini CLI to execute AI-driven tasks, returning structured JSON events and a session identifier for conversation continuity."
    )]
    async fn gemini(
        &self,
        Parameters(args): Parameters<GeminiArgs>,
    ) -> Result<CallToolResult, McpError> {
        if !self.capabilities.gemini_available {
            return Err(McpError::internal_error(
                "Gemini CLI not found in PATH. Install gemini CLI or set GEMINI_BIN env var.",
                None,
            ));
        }

        if args.prompt.trim().is_empty() {
            return Err(McpError::invalid_params(
                "PROMPT is required and must be a non-empty, non-whitespace string",
                None,
            ));
        }

        if let Some(ref model) = args.model {
            if model.trim().is_empty() {
                return Err(McpError::invalid_params(
                    "Model overrides must be explicitly requested as a non-empty, non-whitespace string",
                    None,
                ));
            }
        }

        if let Some(timeout) = args.timeout_secs {
            if !(MIN_TIMEOUT_SECS..=MAX_TIMEOUT_SECS).contains(&timeout) {
                return Err(McpError::invalid_params(
                    format!(
                        "timeout_secs must be between {} and {} seconds",
                        MIN_TIMEOUT_SECS, MAX_TIMEOUT_SECS
                    ),
                    None,
                ));
            }
        }

        let session_id = args.session_id.filter(|s| !s.is_empty());
        let model = args.model.filter(|m| !m.trim().is_empty());

        // Read MCP client roots to pass as --include-directories to Gemini CLI
        let include_directories = self.roots.read().await.clone();

        let opts = gemini::Options {
            prompt: args.prompt,
            sandbox: args.sandbox,
            session_id,
            return_all_messages: args.return_all_messages,
            model,
            timeout_secs: args.timeout_secs,
            include_directories,
            model_env_fallback: gemini::get_force_model(),
            api_key: gemini::get_api_key(),
            api_base_url: gemini::get_api_url(),
        };

        Self::run_gemini(opts, args.return_all_messages).await
    }

    /// Generates images using the Gemini API directly (not via CLI).
    /// Returns the generated image(s) as base64-encoded content along with any text response.
    ///
    /// **Return structure:**
    /// - Image content(s) as base64-encoded data with mime type
    /// - Text description from the model (if any)
    ///
    /// **Required environment variables:**
    /// - `GEMINI_IMAGE_API_KEY`: API key for image generation
    /// - `GEMINI_API_URL`: Base URL for the Gemini API
    /// - `GEMINI_IMAGE_MODEL`: Model name for image generation (e.g. "gemini-3-pro-image-preview")
    #[tool(
        name = "gemini_image",
        description = "Invokes the Gemini CLI for image generation tasks, using a dedicated image generation model configured via GEMINI_IMAGE_MODEL environment variable."
    )]
    async fn gemini_image(
        &self,
        Parameters(args): Parameters<GeminiImageArgs>,
    ) -> Result<CallToolResult, McpError> {
        if args.prompt.trim().is_empty() {
            return Err(McpError::invalid_params(
                "PROMPT is required and must be a non-empty, non-whitespace string",
                None,
            ));
        }

        let api_url = gemini::get_api_url().ok_or_else(|| {
            McpError::internal_error(
                "GEMINI_API_URL environment variable is not set",
                None,
            )
        })?;

        let api_key = gemini::get_image_api_key().ok_or_else(|| {
            McpError::internal_error(
                "GEMINI_IMAGE_API_KEY environment variable is not set",
                None,
            )
        })?;

        let model = args
            .model
            .filter(|m| !m.trim().is_empty())
            .or_else(|| gemini::get_image_model())
            .ok_or_else(|| {
                McpError::internal_error(
                    "No model specified and GEMINI_IMAGE_MODEL environment variable is not set",
                    None,
                )
            })?;

        match gemini_image_api::generate_image(
            &api_url,
            &api_key,
            &model,
            &args.prompt,
            args.aspect_ratio.as_deref(),
            args.image_size.as_deref(),
        )
        .await
        {
            Ok(result) => {
                let mut contents: Vec<Content> = Vec::new();
                let mut saved_paths: Vec<String> = Vec::new();

                for (idx, (data, mime_type)) in result.images.iter().enumerate() {
                    contents.push(Content::image(data.as_str(), mime_type.as_str()));

                    // Save image to current working directory
                    let ext = match mime_type.as_str() {
                        "image/png" => "png",
                        "image/jpeg" | "image/jpg" => "jpg",
                        "image/webp" => "webp",
                        "image/gif" => "gif",
                        _ => "png",
                    };
                    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
                    let filename = if result.images.len() == 1 {
                        format!("gemini_image_{}.{}", timestamp, ext)
                    } else {
                        format!("gemini_image_{}_{}.{}", timestamp, idx + 1, ext)
                    };

                    match base64::Engine::decode(
                        &base64::engine::general_purpose::STANDARD,
                        data,
                    ) {
                        Ok(bytes) => {
                            // Save directory priority:
                            // 1. output_dir parameter (caller specifies)
                            // 2. First MCP workspace root
                            // 3. Current working directory
                            let save_dir = if let Some(dir) = args
                                .output_dir
                                .as_ref()
                                .filter(|s| !s.trim().is_empty())
                            {
                                PathBuf::from(dir)
                            } else {
                                self.roots
                                    .read()
                                    .await
                                    .first()
                                    .cloned()
                                    .unwrap_or_else(|| {
                                        std::env::current_dir()
                                            .unwrap_or_else(|_| PathBuf::from("."))
                                    })
                            };
                            let save_path = save_dir.join(&filename);
                            match std::fs::write(&save_path, &bytes) {
                                Ok(_) => {
                                    let abs_path = save_path
                                        .canonicalize()
                                        .unwrap_or(save_path)
                                        .display()
                                        .to_string();
                                    saved_paths.push(abs_path);
                                }
                                Err(e) => {
                                    eprintln!("[gemini_image] Failed to save {}: {}", filename, e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("[gemini_image] Failed to decode base64: {}", e);
                        }
                    }
                }

                if let Some(ref text) = result.text {
                    contents.push(Content::text(text));
                }

                if !saved_paths.is_empty() {
                    let paths_text = saved_paths
                        .iter()
                        .map(|p| format!("- {}", p))
                        .collect::<Vec<_>>()
                        .join("\n");
                    contents.push(Content::text(format!(
                        "Saved to:\n{}",
                        paths_text
                    )));
                }

                if contents.is_empty() {
                    contents.push(Content::text("Image generation completed but no content was returned."));
                }

                Ok(CallToolResult::success(contents))
            }
            Err(e) => Err(McpError::internal_error(
                format!("Image generation failed: {}", e),
                None,
            )),
        }
    }

    /// Executes a non-interactive Codex session via CLI to perform AI-assisted coding tasks in a secure workspace.
    /// This tool wraps the 'codex exec' command, enabling model-driven code generation, debugging, or automation based on natural language prompts.
    /// It supports resuming ongoing sessions for continuity and enforces sandbox policies to prevent unsafe operations.
    #[tool(
        name = "codex",
        description = "Execute Codex CLI for AI-assisted coding tasks"
    )]
    async fn codex(
        &self,
        Parameters(args): Parameters<CodexArgs>,
    ) -> Result<CallToolResult, McpError> {
        if !self.capabilities.codex_available {
            return Err(McpError::internal_error(
                "Codex CLI not found in PATH. Install codex CLI or set CODEX_BIN env var.",
                None,
            ));
        }

        let mut security_warnings = Vec::new();
        let security = get_security_config(&mut security_warnings);

        if args.prompt.is_empty() {
            return Err(McpError::invalid_params(
                "PROMPT is required and must be a non-empty string",
                None,
            ));
        }

        if args.cd.as_os_str().is_empty() {
            return Err(McpError::invalid_params(
                "cd is required and must be a non-empty string",
                None,
            ));
        }

        let (mut args, restriction_warnings) = apply_security_restrictions(args, &security);
        security_warnings.extend(restriction_warnings);

        match args.timeout_secs {
            None => {
                let default_result = get_default_timeout_with_warning();
                args.timeout_secs = Some(default_result.value);
                if let Some(warning) = default_result.warning {
                    security_warnings.push(warning);
                }
            }
            Some(0) => {
                let default_result = get_default_timeout_with_warning();
                security_warnings.push(format!(
                    "Timeout of 0 seconds is invalid; using default of {} seconds",
                    default_result.value
                ));
                if let Some(warning) = default_result.warning {
                    security_warnings.push(warning);
                }
                args.timeout_secs = Some(default_result.value);
            }
            Some(timeout) if timeout > MAX_TIMEOUT_SECS => {
                security_warnings.push(format!(
                    "Timeout of {} seconds exceeds maximum of {} seconds; capping to maximum",
                    timeout, MAX_TIMEOUT_SECS
                ));
                args.timeout_secs = Some(MAX_TIMEOUT_SECS);
            }
            Some(_) => {}
        }

        let working_dir = &args.cd;
        let canonical_working_dir = working_dir.canonicalize().map_err(|e| {
            McpError::invalid_params(
                format!(
                    "working directory does not exist or is not accessible: {} ({})",
                    working_dir.display(),
                    e
                ),
                None,
            )
        })?;

        if !canonical_working_dir.is_dir() {
            return Err(McpError::invalid_params(
                format!(
                    "working directory is not a directory: {}",
                    working_dir.display()
                ),
                None,
            ));
        }

        let mut canonical_image_paths = Vec::new();
        for img_path in &args.image {
            let resolved_path = if img_path.is_absolute() {
                img_path.clone()
            } else {
                canonical_working_dir.join(img_path)
            };

            let canonical = resolved_path.canonicalize().map_err(|e| {
                McpError::invalid_params(
                    format!(
                        "image file does not exist or is not accessible: {} ({})",
                        resolved_path.display(),
                        e
                    ),
                    None,
                )
            })?;

            if !canonical.is_file() {
                return Err(McpError::invalid_params(
                    format!("image path is not a file: {}", resolved_path.display()),
                    None,
                ));
            }

            canonical_image_paths.push(canonical);
        }

        let opts = codex::Options {
            prompt: args.prompt,
            working_dir: canonical_working_dir,
            sandbox: args.sandbox,
            session_id: args.session_id,
            skip_git_repo_check: args.skip_git_repo_check,
            return_all_messages: args.return_all_messages,
            return_all_messages_limit: args.return_all_messages_limit,
            image_paths: canonical_image_paths,
            model: args.model,
            yolo: args.yolo,
            profile: args.profile,
            timeout_secs: args.timeout_secs,
            force_stdin: args.force_stdin,
        };

        let result = match codex::run(opts).await {
            Ok(r) => r,
            Err(e) => {
                let warning_text = merge_warnings(security_warnings.clone(), None);
                let error_msg =
                    attach_warnings(format!("Failed to execute codex: {}", e), warning_text);
                return Err(McpError::internal_error(error_msg, None));
            }
        };

        let combined_warnings = merge_warnings(security_warnings.clone(), result.warnings.clone());
        let output = build_codex_output(&result, args.return_all_messages, combined_warnings);

        let json_output = serde_json::to_string(&output).map_err(|e| {
            McpError::internal_error(format!("Failed to serialize output: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json_output)]))
    }

    /// Performs a third-party web search based on the given query and returns the results as a JSON string.
    #[tool(
        name = "web_search",
        description = "Performs a third-party web search based on the given query and returns the results as a JSON string. The query should be a clear, self-contained natural-language search query."
    )]
    async fn web_search(
        &self,
        Parameters(args): Parameters<WebSearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        if !self.capabilities.grok_available {
            return Err(McpError::internal_error(
                "GROK_API_URL or GROK_API_KEY not configured. Set both environment variables to enable web search.",
                None,
            ));
        }

        if args.query.trim().is_empty() {
            return Err(McpError::invalid_params(
                "query is required and must be a non-empty string",
                None,
            ));
        }

        let platform = args.platform.unwrap_or_default();

        match grok::tools::web_search(&args.query, &platform, args.min_results, args.max_results)
            .await
        {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Err(McpError::internal_error(
                format!("Web search failed: {}", e),
                None,
            )),
        }
    }

    /// Fetches and extracts the complete content from a specified URL and returns it as a structured Markdown document.
    #[tool(
        name = "web_fetch",
        description = "Fetches and extracts the complete content from a specified URL and returns it as a structured Markdown document. The URL should be a valid HTTP/HTTPS web address."
    )]
    async fn web_fetch(
        &self,
        Parameters(args): Parameters<WebFetchArgs>,
    ) -> Result<CallToolResult, McpError> {
        if !self.capabilities.grok_available {
            return Err(McpError::internal_error(
                "GROK_API_URL or GROK_API_KEY not configured. Set both environment variables to enable web fetch.",
                None,
            ));
        }

        if args.url.trim().is_empty() {
            return Err(McpError::invalid_params(
                "url is required and must be a non-empty string",
                None,
            ));
        }

        match grok::tools::web_fetch(&args.url).await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Err(McpError::internal_error(
                format!("Web fetch failed: {}", e),
                None,
            )),
        }
    }

    /// Returns the current Grok Search configuration information and tests the connection.
    #[tool(
        name = "get_config_info",
        description = "Returns the current Grok Search MCP server configuration information and tests the connection. Useful for verifying environment variables, testing API connectivity, and debugging configuration issues."
    )]
    async fn get_config_info(&self) -> Result<CallToolResult, McpError> {
        match grok::tools::get_config_info().await {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => Err(McpError::internal_error(
                format!("Failed to get config info: {}", e),
                None,
            )),
        }
    }
}

#[tool_handler]
impl ServerHandler for UnifiedServer {
    async fn on_initialized(&self, context: NotificationContext<RoleServer>) {
        // Request workspace roots from the MCP client.
        // These are passed to Gemini CLI as --include-directories so it can
        // access files outside its inherited CWD (which MCP hosts may set to
        // an internal directory like F:\Windsurf).
        // Use a short timeout — some clients don't support roots/list and
        // the call would block indefinitely without one.
        let roots_future = context.peer.list_roots();
        match tokio::time::timeout(std::time::Duration::from_secs(3), roots_future).await {
            Ok(Ok(roots_result)) => {
                let dirs: Vec<PathBuf> = roots_result
                    .roots
                    .iter()
                    .filter_map(|root| file_uri_to_path(&root.uri))
                    .collect();
                if !dirs.is_empty() {
                    eprintln!(
                        "ikuncode-aimcp: received {} workspace root(s) from MCP client",
                        dirs.len()
                    );
                    *self.roots.write().await = dirs;
                }
            }
            Ok(Err(e)) => {
                eprintln!(
                    "ikuncode-aimcp: failed to list roots from MCP client (non-fatal): {}",
                    e
                );
            }
            Err(_) => {
                eprintln!(
                    "ikuncode-aimcp: list_roots timed out (client may not support roots/list, non-fatal)"
                );
            }
        }
    }

    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Unified AI MCP server providing gemini, codex, and grok search tools. \
                 Use 'gemini' for AI-driven tasks via Gemini CLI, 'codex' for AI-assisted coding \
                 via Codex CLI, 'web_search' for web searches, 'web_fetch' for fetching web content, \
                 and 'get_config_info' for configuration status."
                    .to_string(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_args_deserialization() {
        let json = r#"{
            "PROMPT": "test prompt",
            "sandbox": true,
            "SESSION_ID": "session-123",
            "return_all_messages": false,
            "model": "gemini-pro"
        }"#;

        let args: GeminiArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.prompt, "test prompt");
        assert!(args.sandbox);
        assert_eq!(args.session_id, Some("session-123".to_string()));
        assert!(!args.return_all_messages);
        assert_eq!(args.model, Some("gemini-pro".to_string()));
    }

    #[test]
    fn test_gemini_args_empty_session_id_treated_as_some() {
        let json = r#"{
            "PROMPT": "test prompt",
            "SESSION_ID": ""
        }"#;

        let args: GeminiArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, Some("".to_string()));
    }

    #[test]
    fn test_codex_args_deserialization() {
        let json = r#"{
            "PROMPT": "fix bug",
            "cd": "/tmp/project",
            "sandbox": "read-only",
            "image": [],
            "yolo": false
        }"#;

        let args: CodexArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.prompt, "fix bug");
        assert_eq!(args.cd, PathBuf::from("/tmp/project"));
        assert_eq!(args.sandbox, SandboxPolicy::ReadOnly);
        assert!(!args.yolo);
    }

    #[test]
    fn test_web_search_args_defaults() {
        let json = r#"{"query": "rust programming"}"#;

        let args: WebSearchArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.query, "rust programming");
        assert_eq!(args.platform, None);
        assert_eq!(args.min_results, 3);
        assert_eq!(args.max_results, 10);
    }

    #[test]
    fn test_web_fetch_args_deserialization() {
        let json = r#"{"url": "https://example.com"}"#;

        let args: WebFetchArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.url, "https://example.com");
    }

    #[test]
    fn test_resolve_env_bool_truthy() {
        let mut warnings = Vec::new();
        assert_eq!(
            codex::resolve_env_bool("K", Some("1".into()), &mut warnings),
            Some(true)
        );
        assert_eq!(
            codex::resolve_env_bool("K", Some("true".into()), &mut warnings),
            Some(true)
        );
        assert_eq!(
            codex::resolve_env_bool("K", Some("yes".into()), &mut warnings),
            Some(true)
        );
        assert_eq!(
            codex::resolve_env_bool("K", Some("on".into()), &mut warnings),
            Some(true)
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_resolve_env_bool_falsy() {
        let mut warnings = Vec::new();
        assert_eq!(
            codex::resolve_env_bool("K", Some("0".into()), &mut warnings),
            Some(false)
        );
        assert_eq!(
            codex::resolve_env_bool("K", Some("false".into()), &mut warnings),
            Some(false)
        );
        assert_eq!(
            codex::resolve_env_bool("K", Some("off".into()), &mut warnings),
            Some(false)
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_resolve_env_bool_invalid() {
        let mut warnings = Vec::new();
        assert_eq!(
            codex::resolve_env_bool("TEST_KEY", Some("maybe".into()), &mut warnings),
            None
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("TEST_KEY"));
    }

    #[test]
    fn test_resolve_env_bool_empty() {
        let mut warnings = Vec::new();
        assert_eq!(
            codex::resolve_env_bool("K", Some("".into()), &mut warnings),
            None
        );
        assert_eq!(codex::resolve_env_bool("K", None, &mut warnings), None);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_merge_warnings_combines() {
        let combined = merge_warnings(vec!["security".into()], Some("result".into())).unwrap();
        assert!(combined.contains("security"));
        assert!(combined.contains("result"));
    }

    #[test]
    fn test_merge_warnings_empty() {
        assert!(merge_warnings(vec![], None).is_none());
    }

    #[test]
    fn test_attach_warnings_appends() {
        let message = attach_warnings(
            "failure".to_string(),
            Some("warn-one\nwarn-two".to_string()),
        );
        assert!(message.contains("failure"));
        assert!(message.contains("Warnings: warn-one"));
        assert!(message.contains("warn-two"));
    }

    #[test]
    fn test_apply_security_restrictions_downgrades() {
        let args = CodexArgs {
            prompt: "test".to_string(),
            cd: PathBuf::from("/tmp"),
            sandbox: SandboxPolicy::DangerFullAccess,
            session_id: None,
            skip_git_repo_check: true,
            return_all_messages: false,
            return_all_messages_limit: None,
            image: vec![],
            model: None,
            yolo: true,
            profile: None,
            timeout_secs: None,
            force_stdin: false,
        };
        let security = SecurityConfig {
            allow_danger_full_access: false,
            allow_yolo: false,
            allow_skip_git_check: false,
        };

        let (updated, warnings) = apply_security_restrictions(args, &security);
        assert_eq!(warnings.len(), 3);
        assert_eq!(updated.sandbox, SandboxPolicy::ReadOnly);
        assert!(!updated.yolo);
        assert!(!updated.skip_git_repo_check);
    }

    #[test]
    fn test_resolve_timeout_default() {
        let result = codex::resolve_timeout_from_env(Err(std::env::VarError::NotPresent));
        assert_eq!(result.value, crate::shared::DEFAULT_TIMEOUT_SECS);
        assert!(result.warning.is_none());
    }

    #[test]
    fn test_resolve_timeout_valid() {
        let result = codex::resolve_timeout_from_env(Ok("1800".into()));
        assert_eq!(result.value, 1800);
        assert!(result.warning.is_none());
    }

    #[test]
    fn test_resolve_timeout_caps_max() {
        let result = codex::resolve_timeout_from_env(Ok("9999".into()));
        assert_eq!(result.value, MAX_TIMEOUT_SECS);
        assert!(result.warning.is_some());
        assert!(result.warning.unwrap().contains("exceeds maximum"));
    }

    #[test]
    fn test_resolve_timeout_rejects_zero() {
        let result = codex::resolve_timeout_from_env(Ok("0".into()));
        assert_eq!(result.value, crate::shared::DEFAULT_TIMEOUT_SECS);
        assert!(result.warning.is_some());
    }

    #[test]
    fn test_resolve_timeout_rejects_invalid() {
        let result = codex::resolve_timeout_from_env(Ok("not-a-number".into()));
        assert_eq!(result.value, crate::shared::DEFAULT_TIMEOUT_SECS);
        assert!(result.warning.is_some());
    }

    #[test]
    fn test_resolve_timeout_empty() {
        let result = codex::resolve_timeout_from_env(Ok("".into()));
        assert_eq!(result.value, crate::shared::DEFAULT_TIMEOUT_SECS);
        assert!(result.warning.is_none());
    }

    #[test]
    fn test_build_codex_output_success() {
        let result = codex::CodexResult {
            success: true,
            session_id: "sess-1".into(),
            agent_messages: "done".into(),
            agent_messages_truncated: false,
            all_messages: vec![],
            all_messages_truncated: false,
            error: None,
            warnings: None,
        };
        let output = build_codex_output(&result, false, None);
        assert!(output.success);
        assert_eq!(output.session_id, "sess-1");
        assert!(output.all_messages.is_none());
    }

    #[test]
    fn test_file_uri_to_path_windows() {
        // Windows-style file URI
        let path = file_uri_to_path("file:///D:/Desk/ai-tools/aimcp");
        assert!(path.is_some());
        #[cfg(windows)]
        assert_eq!(path.unwrap(), PathBuf::from("D:/Desk/ai-tools/aimcp"));
        #[cfg(not(windows))]
        assert_eq!(path.unwrap(), PathBuf::from("/D:/Desk/ai-tools/aimcp"));
    }

    #[test]
    fn test_file_uri_to_path_unix() {
        let path = file_uri_to_path("file:///home/user/project");
        assert!(path.is_some());
        // On all platforms, /home/user/project is preserved
        let p = path.unwrap();
        assert!(p.to_string_lossy().contains("home"));
    }

    #[test]
    fn test_file_uri_to_path_non_file_uri() {
        assert!(file_uri_to_path("https://example.com").is_none());
        assert!(file_uri_to_path("").is_none());
        assert!(file_uri_to_path("not-a-uri").is_none());
    }

    #[test]
    fn test_file_uri_to_path_empty_path() {
        assert!(file_uri_to_path("file://").is_none());
    }

    #[test]
    fn test_unified_server_new() {
        let caps = Capabilities {
            gemini_available: true,
            gemini_path: Some(PathBuf::from("/usr/bin/gemini")),
            codex_available: false,
            codex_path: None,
            grok_available: true,
        };
        let server = UnifiedServer::new(caps);
        assert!(server.capabilities.gemini_available);
        assert!(!server.capabilities.codex_available);
        assert!(server.capabilities.grok_available);
    }
}
