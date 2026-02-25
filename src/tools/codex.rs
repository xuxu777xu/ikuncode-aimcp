use anyhow::{Context, Result};
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::shared::{DEFAULT_TIMEOUT_SECS, MAX_TIMEOUT_SECS};

/// Sandbox policy for model-generated commands
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxPolicy {
    /// Read-only access (safe for exploration)
    #[default]
    ReadOnly,
    /// Write access within workspace (modify files)
    WorkspaceWrite,
    /// Full system access (dangerous)
    DangerFullAccess,
}

impl SandboxPolicy {
    pub fn as_str(&self) -> &str {
        match self {
            SandboxPolicy::ReadOnly => "read-only",
            SandboxPolicy::WorkspaceWrite => "workspace-write",
            SandboxPolicy::DangerFullAccess => "danger-full-access",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Options {
    pub prompt: String,
    pub working_dir: PathBuf,
    pub sandbox: SandboxPolicy,
    pub session_id: Option<String>,
    pub skip_git_repo_check: bool,
    pub return_all_messages: bool,
    pub return_all_messages_limit: Option<usize>,
    pub image_paths: Vec<PathBuf>,
    pub model: Option<String>,
    pub yolo: bool,
    pub profile: Option<String>,
    pub timeout_secs: Option<u64>,
    pub force_stdin: bool,
}

#[derive(Debug)]
pub struct CodexResult {
    pub success: bool,
    pub session_id: String,
    pub agent_messages: String,
    pub agent_messages_truncated: bool,
    pub all_messages: Vec<HashMap<String, Value>>,
    pub all_messages_truncated: bool,
    pub error: Option<String>,
    pub warnings: Option<String>,
}

#[derive(Debug)]
struct ReadLineResult {
    bytes_read: usize,
    truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValidationMode {
    Full,
    Skip,
}

async fn read_line_with_limit<R: AsyncBufReadExt + Unpin>(
    reader: &mut R,
    buf: &mut Vec<u8>,
    max_len: usize,
) -> std::io::Result<ReadLineResult> {
    let mut total_read = 0;
    let mut truncated = false;

    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            break;
        }

        for (i, &byte) in available.iter().enumerate() {
            if !truncated && buf.len() < max_len {
                buf.push(byte);
                total_read += 1;
            } else if !truncated {
                truncated = true;
            }

            if byte == b'\n' {
                reader.consume(i + 1);
                return Ok(ReadLineResult {
                    bytes_read: total_read,
                    truncated,
                });
            }
        }

        let consumed = available.len();
        reader.consume(consumed);
    }

    Ok(ReadLineResult {
        bytes_read: total_read,
        truncated,
    })
}

const MAX_CLI_PROMPT_LEN: usize = 800;

const SPECIAL_CHARS: &[char] = &[
    '\n', '\\', '"', '\'', '`', '$', '%', '^', '!', '&', '|', '<', '>', '(', ')',
];

fn needs_stdin_mode(prompt: &str) -> bool {
    prompt.len() > MAX_CLI_PROMPT_LEN || prompt.contains(SPECIAL_CHARS)
}

fn normalize_timeout_secs(timeout_secs: Option<u64>) -> u64 {
    match timeout_secs {
        None | Some(0) => DEFAULT_TIMEOUT_SECS,
        Some(value) if value > MAX_TIMEOUT_SECS => MAX_TIMEOUT_SECS,
        Some(value) => value,
    }
}

pub async fn run(opts: Options) -> Result<CodexResult> {
    let timeout_secs = normalize_timeout_secs(opts.timeout_secs);

    let opts = Options {
        timeout_secs: Some(timeout_secs),
        ..opts
    };

    let duration = std::time::Duration::from_secs(timeout_secs);
    match tokio::time::timeout(duration, run_internal(opts)).await {
        Ok(result) => result,
        Err(_) => {
            let result = CodexResult {
                success: false,
                session_id: String::new(),
                agent_messages: String::new(),
                agent_messages_truncated: false,
                all_messages: Vec::new(),
                all_messages_truncated: false,
                error: Some(format!(
                    "Codex execution timed out after {} seconds",
                    timeout_secs
                )),
                warnings: None,
            };
            Ok(enforce_required_fields(result, ValidationMode::Skip))
        }
    }
}

async fn run_internal(opts: Options) -> Result<CodexResult> {
    let codex_bin = std::env::var("CODEX_BIN").unwrap_or_else(|_| "codex".to_string());

    #[cfg(windows)]
    let mut cmd = {
        let comspec = std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string());
        let mut c = Command::new(comspec);
        c.args(["/D", "/S", "/C", &codex_bin]);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = Command::new(codex_bin);

    cmd.args(["exec", "--sandbox", opts.sandbox.as_str(), "--cd"]);
    cmd.arg(opts.working_dir.as_os_str());
    cmd.arg("--json");

    for image_path in &opts.image_paths {
        cmd.arg("--image");
        cmd.arg(image_path);
    }
    if let Some(ref model) = opts.model {
        cmd.args(["--model", model]);
    }
    if let Some(ref profile) = opts.profile {
        cmd.args(["--profile", profile]);
    }
    if opts.yolo {
        cmd.arg("--yolo");
    }
    if opts.skip_git_repo_check {
        cmd.arg("--skip-git-repo-check");
    }
    if opts.return_all_messages {
        cmd.arg("--return-all-messages");
        if let Some(limit) = opts.return_all_messages_limit {
            cmd.args(["--return-all-messages-limit", &limit.to_string()]);
        }
    }

    if let Some(ref session_id) = opts.session_id {
        cmd.args(["resume", session_id]);
    }

    let use_stdin = opts.force_stdin || needs_stdin_mode(&opts.prompt);
    if use_stdin {
        cmd.args(["--", "-"]);
        cmd.stdin(Stdio::piped());
    } else {
        cmd.args(["--", &opts.prompt]);
        cmd.stdin(Stdio::null());
    }

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.kill_on_drop(true);

    let mut child = cmd.spawn().context("Failed to spawn codex command")?;

    if use_stdin {
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(opts.prompt.as_bytes()).await {
                match e.kind() {
                    std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::NotConnected => {
                        eprintln!(
                            "Warning: codex process closed stdin early ({}); \
                             continuing to collect exit status and stderr",
                            e
                        );
                    }
                    _ => {
                        return Err(e).context("Failed to write prompt to codex stdin");
                    }
                }
            }
        }
    }

    let stdout = child.stdout.take().context("Failed to get stdout")?;
    let stderr = child.stderr.take().context("Failed to get stderr")?;

    let mut result = CodexResult {
        success: true,
        session_id: String::new(),
        agent_messages: String::new(),
        agent_messages_truncated: false,
        all_messages: Vec::new(),
        all_messages_truncated: false,
        error: None,
        warnings: None,
    };

    const MAX_MESSAGE_LIMIT: usize = 50000;
    const DEFAULT_MESSAGE_LIMIT: usize = 10000;
    const MAX_AGENT_MESSAGES_SIZE: usize = 10 * 1024 * 1024;
    const MAX_ALL_MESSAGES_SIZE: usize = 50 * 1024 * 1024;
    let message_limit = opts
        .return_all_messages_limit
        .unwrap_or(DEFAULT_MESSAGE_LIMIT)
        .min(MAX_MESSAGE_LIMIT);

    let mut all_messages_size: usize = 0;

    const MAX_STDERR_SIZE: usize = 1024 * 1024;
    const MAX_LINE_LENGTH: usize = 1024 * 1024;
    let stderr_handle = tokio::spawn(async move {
        let mut stderr_output = String::new();
        let mut stderr_reader = BufReader::new(stderr);
        let mut truncated = false;
        let mut line_buf = Vec::new();

        loop {
            line_buf.clear();
            match read_line_with_limit(&mut stderr_reader, &mut line_buf, MAX_LINE_LENGTH).await {
                Ok(read_result) => {
                    if read_result.bytes_read == 0 {
                        break;
                    }
                    let line = String::from_utf8_lossy(&line_buf);
                    let line = line.trim_end_matches('\n').trim_end_matches('\r');
                    let new_size = stderr_output.len() + line.len() + 1;
                    if new_size > MAX_STDERR_SIZE {
                        if !truncated {
                            if !stderr_output.is_empty() {
                                stderr_output.push('\n');
                            }
                            stderr_output.push_str("[... stderr truncated due to size limit ...]");
                            truncated = true;
                        }
                    } else if !truncated {
                        if !stderr_output.is_empty() {
                            stderr_output.push('\n');
                        }
                        stderr_output.push_str(line.as_ref());
                    }
                }
                Err(e) => {
                    eprintln!("Warning: Failed to read from stderr: {}", e);
                    break;
                }
            }
        }

        stderr_output
    });

    let mut reader = BufReader::new(stdout);
    let mut parse_error_seen = false;
    let mut line_buf = Vec::new();

    loop {
        line_buf.clear();
        match read_line_with_limit(&mut reader, &mut line_buf, MAX_LINE_LENGTH).await {
            Ok(read_result) => {
                if read_result.bytes_read == 0 {
                    break;
                }

                if read_result.truncated {
                    let error_msg = format!(
                        "Output line exceeded {} byte limit and was truncated, cannot parse JSON.",
                        MAX_LINE_LENGTH
                    );
                    result.success = false;
                    result.error = Some(error_msg);
                    if !parse_error_seen {
                        parse_error_seen = true;
                        let _ = child.start_kill();
                    }
                    continue;
                }

                let line = String::from_utf8_lossy(&line_buf);
                let line = line.trim_end_matches('\n').trim_end_matches('\r');

                if line.is_empty() {
                    continue;
                }

                if parse_error_seen {
                    continue;
                }

                let line_data: Value = match serde_json::from_str(line) {
                    Ok(data) => data,
                    Err(e) => {
                        record_parse_error(&mut result, &e, line);
                        if !parse_error_seen {
                            parse_error_seen = true;
                            let _ = child.start_kill();
                        }
                        continue;
                    }
                };

                if opts.return_all_messages {
                    if result.all_messages.len() < message_limit {
                        if let Ok(map) =
                            serde_json::from_value::<HashMap<String, Value>>(line_data.clone())
                        {
                            let message_size =
                                serde_json::to_string(&map).map(|s| s.len()).unwrap_or(0);
                            if all_messages_size + message_size <= MAX_ALL_MESSAGES_SIZE {
                                all_messages_size += message_size;
                                result.all_messages.push(map);
                            } else if !result.all_messages_truncated {
                                result.all_messages_truncated = true;
                            }
                        }
                    } else if !result.all_messages_truncated {
                        result.all_messages_truncated = true;
                    }
                }

                if let Some(thread_id) = line_data.get("thread_id").and_then(|v| v.as_str()) {
                    if !thread_id.is_empty() {
                        result.session_id = thread_id.to_string();
                    }
                }

                if let Some(item) = line_data.get("item").and_then(|v| v.as_object()) {
                    if let Some(item_type) = item.get("type").and_then(|v| v.as_str()) {
                        if item_type == "agent_message" {
                            if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                                let new_size = result.agent_messages.len() + text.len();
                                if new_size > MAX_AGENT_MESSAGES_SIZE {
                                    if !result.agent_messages_truncated {
                                        result.agent_messages.push_str(
                                    "\n[... Agent messages truncated due to size limit ...]",
                                );
                                        result.agent_messages_truncated = true;
                                    }
                                } else if !result.agent_messages_truncated {
                                    if !result.agent_messages.is_empty() && !text.is_empty() {
                                        result.agent_messages.push('\n');
                                    }
                                    result.agent_messages.push_str(text);
                                }
                            }
                        }
                    }
                }

                if let Some(line_type) = line_data.get("type").and_then(|v| v.as_str()) {
                    if line_type.contains("fail") || line_type.contains("error") {
                        result.success = false;
                        if let Some(error_obj) = line_data.get("error").and_then(|v| v.as_object())
                        {
                            if let Some(msg) = error_obj.get("message").and_then(|v| v.as_str()) {
                                result.error = Some(format!("codex error: {}", msg));
                            }
                        } else if let Some(msg) = line_data.get("message").and_then(|v| v.as_str())
                        {
                            result.error = Some(format!("codex error: {}", msg));
                        }
                    }
                }
            }
            Err(e) => {
                let io_error = std::io::Error::from(e.kind());
                record_parse_error(&mut result, &serde_json::Error::io(io_error), "");
                break;
            }
        }
    }

    let status = child
        .wait()
        .await
        .context("Failed to wait for codex command")?;

    let stderr_output = match stderr_handle.await {
        Ok(output) => output,
        Err(e) => {
            eprintln!("Warning: Failed to join stderr task: {}", e);
            String::new()
        }
    };

    if !status.success() {
        result.success = false;
        let error_msg = if let Some(ref err) = result.error {
            err.clone()
        } else {
            format!("codex command failed with exit code: {:?}", status.code())
        };

        if !stderr_output.is_empty() {
            result.error = Some(format!("{}\nStderr: {}", error_msg, stderr_output));
        } else {
            result.error = Some(error_msg);
        }
    } else if !stderr_output.is_empty() {
        result.warnings = Some(stderr_output);
    }

    Ok(enforce_required_fields(result, ValidationMode::Full))
}

fn record_parse_error(result: &mut CodexResult, error: &serde_json::Error, line: &str) {
    let parse_msg = format!("JSON parse error: {}. Line: {}", error, line);
    result.success = false;
    result.error = match result.error.take() {
        Some(existing) if !existing.is_empty() => Some(format!("{existing}\n{parse_msg}")),
        _ => Some(parse_msg),
    };
}

fn push_warning(existing: Option<String>, warning: &str) -> Option<String> {
    match existing {
        Some(mut current) => {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(warning);
            Some(current)
        }
        None => Some(warning.to_string()),
    }
}

fn enforce_required_fields(mut result: CodexResult, mode: ValidationMode) -> CodexResult {
    if mode == ValidationMode::Skip {
        return result;
    }

    if result.session_id.is_empty() && result.error.is_none() {
        result.success = false;
        result.error = Some("Failed to get SESSION_ID from the codex session.".to_string());
    }

    if result.agent_messages.is_empty() {
        let warning_msg = "No agent_messages returned; enable return_all_messages or check codex output for details.";
        result.warnings = push_warning(result.warnings.take(), warning_msg);
    }

    result
}

// --- Security configuration (ported from codex-mcp-rs/src/server.rs) ---

pub struct DefaultTimeoutResult {
    pub value: u64,
    pub warning: Option<String>,
}

pub fn resolve_timeout_from_env(
    env_result: Result<String, std::env::VarError>,
) -> DefaultTimeoutResult {
    match env_result {
        Ok(val) => {
            let trimmed = val.trim();
            if trimmed.is_empty() {
                return DefaultTimeoutResult {
                    value: DEFAULT_TIMEOUT_SECS,
                    warning: None,
                };
            }
            match trimmed.parse::<u64>() {
                Ok(0) => DefaultTimeoutResult {
                    value: DEFAULT_TIMEOUT_SECS,
                    warning: Some(format!(
                        "CODEX_DEFAULT_TIMEOUT=0 is invalid; using default of {} seconds",
                        DEFAULT_TIMEOUT_SECS
                    )),
                },
                Ok(secs) if secs > MAX_TIMEOUT_SECS => DefaultTimeoutResult {
                    value: MAX_TIMEOUT_SECS,
                    warning: Some(format!(
                        "CODEX_DEFAULT_TIMEOUT={} exceeds maximum of {} seconds; capping to maximum",
                        secs, MAX_TIMEOUT_SECS
                    )),
                },
                Ok(secs) => DefaultTimeoutResult {
                    value: secs,
                    warning: None,
                },
                Err(_) => DefaultTimeoutResult {
                    value: DEFAULT_TIMEOUT_SECS,
                    warning: Some(format!(
                        "CODEX_DEFAULT_TIMEOUT='{}' is not a valid number; using default of {} seconds",
                        trimmed, DEFAULT_TIMEOUT_SECS
                    )),
                },
            }
        }
        Err(std::env::VarError::NotUnicode(_)) => DefaultTimeoutResult {
            value: DEFAULT_TIMEOUT_SECS,
            warning: Some(format!(
                "CODEX_DEFAULT_TIMEOUT contains invalid UTF-8; using default of {} seconds",
                DEFAULT_TIMEOUT_SECS
            )),
        },
        Err(std::env::VarError::NotPresent) => DefaultTimeoutResult {
            value: DEFAULT_TIMEOUT_SECS,
            warning: None,
        },
    }
}

pub fn get_default_timeout_with_warning() -> DefaultTimeoutResult {
    resolve_timeout_from_env(std::env::var("CODEX_DEFAULT_TIMEOUT"))
}

pub struct SecurityConfig {
    pub allow_danger_full_access: bool,
    pub allow_yolo: bool,
    pub allow_skip_git_check: bool,
}

pub fn resolve_env_bool(
    key: &str,
    env_val: Option<String>,
    warnings: &mut Vec<String>,
) -> Option<bool> {
    env_val.and_then(|v| {
        let normalized = v.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "1" | "true" | "yes" | "y" | "on" | "t" | "enable" | "enabled" => Some(true),
            "0" | "false" | "no" | "n" | "off" | "f" | "disable" | "disabled" => Some(false),
            "" => None,
            _ => {
                warnings.push(format!(
                    "Environment variable {} has unrecognized boolean value '{}'; defaulting to disabled.",
                    key, v
                ));
                None
            }
        }
    })
}

fn parse_env_bool(key: &str, warnings: &mut Vec<String>) -> Option<bool> {
    resolve_env_bool(key, std::env::var(key).ok(), warnings)
}

pub fn get_security_config(warnings: &mut Vec<String>) -> SecurityConfig {
    SecurityConfig {
        allow_danger_full_access: parse_env_bool("CODEX_ALLOW_DANGEROUS", warnings)
            .unwrap_or(false),
        allow_yolo: parse_env_bool("CODEX_ALLOW_YOLO", warnings).unwrap_or(false),
        allow_skip_git_check: parse_env_bool("CODEX_ALLOW_SKIP_GIT_CHECK", warnings)
            .unwrap_or(false),
    }
}

pub fn merge_warnings(
    mut security_warnings: Vec<String>,
    result_warnings: Option<String>,
) -> Option<String> {
    if let Some(w) = result_warnings {
        security_warnings.push(w);
    }
    if security_warnings.is_empty() {
        None
    } else {
        Some(security_warnings.join("\n"))
    }
}

pub fn attach_warnings(mut error_msg: String, warnings: Option<String>) -> String {
    if let Some(w) = warnings {
        if !w.is_empty() {
            error_msg = format!("{error_msg}\nWarnings: {w}");
        }
    }
    error_msg
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CodexOutput {
    pub success: bool,
    #[serde(rename = "SESSION_ID")]
    pub session_id: String,
    pub agent_messages: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_messages_truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_messages: Option<Vec<HashMap<String, Value>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_messages_truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<String>,
}

pub fn build_codex_output(
    result: &CodexResult,
    return_all_messages: bool,
    warnings: Option<String>,
) -> CodexOutput {
    CodexOutput {
        success: result.success,
        session_id: result.session_id.clone(),
        agent_messages: result.agent_messages.clone(),
        agent_messages_truncated: result.agent_messages_truncated.then_some(true),
        all_messages: return_all_messages.then_some(result.all_messages.clone()),
        all_messages_truncated: (return_all_messages && result.all_messages_truncated)
            .then_some(true),
        error: result.error.clone(),
        warnings,
    }
}

pub fn apply_security_restrictions(
    sandbox: &mut SandboxPolicy,
    yolo: &mut bool,
    skip_git_repo_check: &mut bool,
    security: &SecurityConfig,
) -> Vec<String> {
    let mut warnings = Vec::new();

    if !security.allow_danger_full_access && *sandbox == SandboxPolicy::DangerFullAccess {
        warnings.push("Security warning: danger-full-access sandbox mode was downgraded to read-only. Set CODEX_ALLOW_DANGEROUS=true to enable.".to_string());
        *sandbox = SandboxPolicy::ReadOnly;
    }

    if !security.allow_yolo && *yolo {
        warnings.push(
            "Security warning: yolo mode was disabled. Set CODEX_ALLOW_YOLO=true to enable."
                .to_string(),
        );
        *yolo = false;
    }

    if !security.allow_skip_git_check && *skip_git_repo_check {
        warnings.push("Security warning: skip_git_repo_check was disabled. Set CODEX_ALLOW_SKIP_GIT_CHECK=true to enable.".to_string());
        *skip_git_repo_check = false;
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::VarError;

    #[test]
    fn test_options_creation() {
        let opts = Options {
            prompt: "test prompt".to_string(),
            working_dir: PathBuf::from("/tmp"),
            sandbox: SandboxPolicy::ReadOnly,
            session_id: None,
            skip_git_repo_check: true,
            return_all_messages: false,
            return_all_messages_limit: None,
            image_paths: vec![],
            model: None,
            yolo: false,
            profile: None,
            timeout_secs: None,
            force_stdin: false,
        };
        assert_eq!(opts.prompt, "test prompt");
        assert_eq!(opts.working_dir, PathBuf::from("/tmp"));
        assert_eq!(opts.sandbox, SandboxPolicy::ReadOnly);
        assert!(opts.skip_git_repo_check);
    }

    #[test]
    fn test_options_with_session() {
        let opts = Options {
            prompt: "resume task".to_string(),
            working_dir: PathBuf::from("/tmp"),
            sandbox: SandboxPolicy::WorkspaceWrite,
            session_id: Some("test-session-123".to_string()),
            skip_git_repo_check: false,
            return_all_messages: true,
            return_all_messages_limit: Some(5000),
            image_paths: vec![PathBuf::from("/path/to/image.png")],
            model: Some("claude-3-opus".to_string()),
            yolo: false,
            profile: Some("default".to_string()),
            timeout_secs: Some(600),
            force_stdin: false,
        };
        assert_eq!(opts.session_id, Some("test-session-123".to_string()));
        assert_eq!(opts.model, Some("claude-3-opus".to_string()));
        assert!(opts.return_all_messages);
        assert!(!opts.skip_git_repo_check);
        assert_eq!(opts.sandbox, SandboxPolicy::WorkspaceWrite);
        assert_eq!(opts.timeout_secs, Some(600));
    }

    #[test]
    fn test_sandbox_policy_as_str() {
        assert_eq!(SandboxPolicy::ReadOnly.as_str(), "read-only");
        assert_eq!(SandboxPolicy::WorkspaceWrite.as_str(), "workspace-write");
        assert_eq!(
            SandboxPolicy::DangerFullAccess.as_str(),
            "danger-full-access"
        );
    }

    #[test]
    fn test_sandbox_policy_default() {
        assert_eq!(SandboxPolicy::default(), SandboxPolicy::ReadOnly);
    }

    #[test]
    fn test_record_parse_error_sets_failure_and_appends_message() {
        let mut result = CodexResult {
            success: true,
            session_id: "session".to_string(),
            agent_messages: "ok".to_string(),
            agent_messages_truncated: false,
            all_messages: Vec::new(),
            all_messages_truncated: false,
            error: Some("existing".to_string()),
            warnings: None,
        };
        let err = serde_json::from_str::<Value>("not-json").unwrap_err();
        record_parse_error(&mut result, &err, "not-json");
        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("JSON parse error"));
        assert!(result.error.as_ref().unwrap().contains("existing"));
    }

    #[test]
    fn test_enforce_required_fields_warns_on_missing_agent_messages() {
        let result = CodexResult {
            success: true,
            session_id: "session".to_string(),
            agent_messages: String::new(),
            agent_messages_truncated: false,
            all_messages: vec![HashMap::new()],
            all_messages_truncated: false,
            error: None,
            warnings: None,
        };
        let updated = enforce_required_fields(result, ValidationMode::Full);
        assert!(updated.success);
        assert!(updated
            .warnings
            .as_ref()
            .unwrap()
            .contains("No agent_messages"));
    }

    #[test]
    fn test_enforce_required_fields_requires_session_id() {
        let result = CodexResult {
            success: true,
            session_id: String::new(),
            agent_messages: "msg".to_string(),
            agent_messages_truncated: false,
            all_messages: Vec::new(),
            all_messages_truncated: false,
            error: None,
            warnings: None,
        };
        let updated = enforce_required_fields(result, ValidationMode::Full);
        assert!(!updated.success);
        assert!(updated
            .error
            .as_ref()
            .unwrap()
            .contains("Failed to get SESSION_ID"));
    }

    #[test]
    fn test_push_warning_appends_with_newline() {
        let combined = push_warning(Some("first".to_string()), "second").unwrap();
        assert!(combined.contains("first"));
        assert!(combined.contains("second"));
        assert!(combined.contains('\n'));
    }

    #[test]
    fn test_enforce_required_fields_skips_validation_when_requested() {
        let result = CodexResult {
            success: false,
            session_id: String::new(),
            agent_messages: String::new(),
            agent_messages_truncated: false,
            all_messages: Vec::new(),
            all_messages_truncated: false,
            error: Some("Codex execution timed out after 10 seconds".to_string()),
            warnings: None,
        };
        let updated = enforce_required_fields(result, ValidationMode::Skip);
        assert!(!updated.success);
        assert_eq!(
            updated.error.unwrap(),
            "Codex execution timed out after 10 seconds"
        );
        assert!(updated.warnings.is_none());
        assert!(updated.session_id.is_empty());
    }

    #[test]
    fn test_enforce_required_fields_skips_session_id_when_error_exists() {
        let result = CodexResult {
            success: false,
            session_id: String::new(),
            agent_messages: String::new(),
            agent_messages_truncated: false,
            all_messages: Vec::new(),
            all_messages_truncated: false,
            error: Some(
                "Output line exceeded 1048576 byte limit and was truncated, cannot parse JSON."
                    .to_string(),
            ),
            warnings: None,
        };
        let updated = enforce_required_fields(result, ValidationMode::Full);
        assert!(!updated.success);
        let error = updated.error.unwrap();
        assert!(error.contains("truncated"));
        assert!(
            !error.contains("SESSION_ID"),
            "Should not add session_id error when truncation error exists"
        );
        assert!(updated.warnings.is_some());
        assert!(updated.warnings.unwrap().contains("No agent_messages"));
    }

    #[test]
    fn test_needs_stdin_short_clean_prompt() {
        assert!(!needs_stdin_mode("simple prompt"));
    }

    #[test]
    fn test_needs_stdin_long_prompt() {
        let long = "a".repeat(801);
        assert!(needs_stdin_mode(&long));
    }

    #[test]
    fn test_needs_stdin_exact_boundary() {
        let exact = "a".repeat(800);
        assert!(!needs_stdin_mode(&exact));
    }

    #[test]
    fn test_needs_stdin_special_chars() {
        assert!(needs_stdin_mode("line1\nline2"));
        assert!(needs_stdin_mode(r"path\to\file"));
        assert!(needs_stdin_mode(r#"say "hello""#));
        assert!(needs_stdin_mode("it's a test"));
        assert!(needs_stdin_mode("echo `date`"));
        assert!(needs_stdin_mode("$HOME/dir"));
        assert!(needs_stdin_mode("100%done"));
        assert!(needs_stdin_mode("a^b"));
        assert!(needs_stdin_mode("!important"));
        assert!(needs_stdin_mode("a&b"));
        assert!(needs_stdin_mode("a|b"));
        assert!(needs_stdin_mode("a<b"));
        assert!(needs_stdin_mode("a>b"));
        assert!(needs_stdin_mode("(group)"));
    }

    // --- Security config tests ---

    #[test]
    fn resolve_env_bool_accepts_truthy_values() {
        let mut warnings = Vec::new();
        assert_eq!(
            resolve_env_bool("K", Some("1".into()), &mut warnings),
            Some(true)
        );
        assert_eq!(
            resolve_env_bool("K", Some("true".into()), &mut warnings),
            Some(true)
        );
        assert_eq!(
            resolve_env_bool("K", Some("yes".into()), &mut warnings),
            Some(true)
        );
        assert_eq!(
            resolve_env_bool("K", Some("on".into()), &mut warnings),
            Some(true)
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn resolve_env_bool_accepts_falsy_values() {
        let mut warnings = Vec::new();
        assert_eq!(
            resolve_env_bool("K", Some("0".into()), &mut warnings),
            Some(false)
        );
        assert_eq!(
            resolve_env_bool("K", Some("false".into()), &mut warnings),
            Some(false)
        );
        assert_eq!(
            resolve_env_bool("K", Some("off".into()), &mut warnings),
            Some(false)
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn resolve_env_bool_warns_on_invalid() {
        let mut warnings = Vec::new();
        assert_eq!(
            resolve_env_bool("TEST_KEY", Some("maybe".into()), &mut warnings),
            None
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("TEST_KEY"));
        assert!(warnings[0].contains("maybe"));
    }

    #[test]
    fn resolve_env_bool_returns_none_for_empty() {
        let mut warnings = Vec::new();
        assert_eq!(resolve_env_bool("K", Some("".into()), &mut warnings), None);
        assert_eq!(resolve_env_bool("K", None, &mut warnings), None);
        assert!(warnings.is_empty());
    }

    #[test]
    fn merge_warnings_combines_security_and_result() {
        let combined = merge_warnings(vec!["security".into()], Some("result".into())).unwrap();
        assert!(combined.contains("security"));
        assert!(combined.contains("result"));
    }

    #[test]
    fn test_apply_security_restrictions_returns_warnings() {
        let mut sandbox = SandboxPolicy::DangerFullAccess;
        let mut yolo = true;
        let mut skip_git = true;
        let security = SecurityConfig {
            allow_danger_full_access: false,
            allow_yolo: false,
            allow_skip_git_check: false,
        };
        let warnings =
            apply_security_restrictions(&mut sandbox, &mut yolo, &mut skip_git, &security);
        assert_eq!(warnings.len(), 3);
        assert_eq!(sandbox, SandboxPolicy::ReadOnly);
        assert!(!yolo);
        assert!(!skip_git);
    }

    #[test]
    fn attach_warnings_appends_to_error_message() {
        let message = attach_warnings(
            "failure".to_string(),
            Some("warn-one\nwarn-two".to_string()),
        );
        assert!(message.contains("failure"));
        assert!(message.contains("Warnings: warn-one"));
        assert!(message.contains("warn-two"));
    }

    #[test]
    fn resolve_timeout_returns_default_when_env_not_set() {
        let result = resolve_timeout_from_env(Err(VarError::NotPresent));
        assert_eq!(result.value, DEFAULT_TIMEOUT_SECS);
        assert!(result.warning.is_none());
    }

    #[test]
    fn resolve_timeout_parses_valid_value() {
        let result = resolve_timeout_from_env(Ok("1800".into()));
        assert_eq!(result.value, 1800);
        assert!(result.warning.is_none());
    }

    #[test]
    fn resolve_timeout_trims_whitespace() {
        let result = resolve_timeout_from_env(Ok("  900  ".into()));
        assert_eq!(result.value, 900);
        assert!(result.warning.is_none());
    }

    #[test]
    fn resolve_timeout_treats_empty_as_unset() {
        let result = resolve_timeout_from_env(Ok("".into()));
        assert_eq!(result.value, DEFAULT_TIMEOUT_SECS);
        assert!(result.warning.is_none());
    }

    #[test]
    fn resolve_timeout_treats_whitespace_only_as_unset() {
        let result = resolve_timeout_from_env(Ok("   ".into()));
        assert_eq!(result.value, DEFAULT_TIMEOUT_SECS);
        assert!(result.warning.is_none());
    }

    #[test]
    fn resolve_timeout_caps_values_exceeding_max() {
        let result = resolve_timeout_from_env(Ok("9999".into()));
        assert_eq!(result.value, MAX_TIMEOUT_SECS);
        assert!(result.warning.is_some());
        assert!(result.warning.unwrap().contains("exceeds maximum"));
    }

    #[test]
    fn resolve_timeout_rejects_zero() {
        let result = resolve_timeout_from_env(Ok("0".into()));
        assert_eq!(result.value, DEFAULT_TIMEOUT_SECS);
        assert!(result.warning.is_some());
        assert!(result.warning.unwrap().contains("invalid"));
    }

    #[test]
    fn resolve_timeout_rejects_invalid_string() {
        let result = resolve_timeout_from_env(Ok("not-a-number".into()));
        assert_eq!(result.value, DEFAULT_TIMEOUT_SECS);
        assert!(result.warning.is_some());
        assert!(result.warning.unwrap().contains("not a valid number"));
    }
}
