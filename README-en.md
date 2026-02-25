# aimcp

[中文文档](README.md)

Unified AI MCP Server — a single Rust binary that combines [Gemini CLI](https://github.com/google-gemini/gemini-cli), [Codex CLI](https://github.com/openai/codex), and [Grok Search](https://x.ai/) into one MCP server.

## Features

- **One binary, all tools** — configure a single MCP server instead of three
- **Runtime detection** — automatically detects which tools are available at startup; unavailable tools return clear error messages when called
- **AdaptiveStdio transport** — auto-detects JSONL and LSP-style framing for maximum client compatibility
- **GrokSearch in Rust** — zero Python dependency; web search and content fetching via Grok API with SSE streaming and retry

## Tools

| Tool | Source | Description |
|------|--------|-------------|
| `gemini` | Gemini CLI | AI-driven tasks with session continuity |
| `gemini_image` | Gemini CLI | AI image generation with dedicated model |
| `codex` | Codex CLI | AI-assisted coding with sandbox policies |
| `web_search` | Grok API | Web search returning structured JSON results |
| `web_fetch` | Grok API | Fetch web page content as Markdown |
| `get_config_info` | Grok API | Show configuration and test API connectivity |

## Tool Usage

### `gemini` — Execute Gemini CLI

| Parameter | Required | Type | Default | Description |
|-----------|----------|------|---------|-------------|
| `PROMPT` | **Yes** | string | — | Instruction for the task to send to Gemini |
| `sandbox` | No | bool | `false` | Run in sandbox mode (isolated execution) |
| `SESSION_ID` | No | string | — | Resume an existing session for multi-turn conversations |
| `return_all_messages` | No | bool | `false` | Return all messages including reasoning and tool calls |
| `model` | No | string | — | Model override. Falls back to `GEMINI_FORCE_MODEL` env var or Gemini CLI default |
| `timeout_secs` | No | int | 600 | Timeout in seconds (1–3600) |

**Return structure:**
- `success` — boolean indicating execution status
- `SESSION_ID` — unique identifier for resuming this conversation
- `agent_messages` — concatenated assistant response text
- `all_messages` — (optional) complete JSON events when `return_all_messages=true`
- `error` — error description when `success=false`

### `gemini_image` — Gemini Image Generation

| Parameter | Required | Type | Default | Description |
|-----------|----------|------|---------|-------------|
| `PROMPT` | **Yes** | string | — | Instruction for the image generation task |
| `sandbox` | No | bool | `false` | Run in sandbox mode (isolated execution) |
| `SESSION_ID` | No | string | — | Resume an existing session for multi-turn conversations |
| `return_all_messages` | No | bool | `false` | Return all messages including reasoning and tool calls |
| `model` | No | string | — | Model override. Falls back to `GEMINI_IMAGE_MODEL` env var or Gemini CLI default |
| `timeout_secs` | No | int | 600 | Timeout in seconds (1–3600) |

**Return structure:**
- `success` — boolean indicating execution status
- `SESSION_ID` — unique identifier for resuming this conversation
- `agent_messages` — concatenated assistant response text
- `all_messages` — (optional) complete JSON events when `return_all_messages=true`
- `error` — error description when `success=false`

### `codex` — Execute Codex CLI

| Parameter | Required | Type | Default | Description |
|-----------|----------|------|---------|-------------|
| `PROMPT` | **Yes** | string | — | Task instruction for Codex |
| `cd` | **Yes** | string | — | Working directory path |
| `sandbox` | No | string | `"read-only"` | Sandbox policy: `"read-only"`, `"workspace-write"`, or `"danger-full-access"` |
| `SESSION_ID` | No | string | — | Resume a previous session |
| `skip_git_repo_check` | No | bool | `false` | Allow running outside git repositories |
| `return_all_messages` | No | bool | `false` | Return full reasoning trace |
| `return_all_messages_limit` | No | int | 10000 | Max messages when `return_all_messages` is true |
| `image` | No | array | `[]` | Paths to image files to attach |
| `model` | No | string | — | Override the Codex model |
| `yolo` | No | bool | `false` | Run without approval prompts or sandboxing |
| `profile` | No | string | — | Config profile from `~/.codex/config.toml` |
| `timeout_secs` | No | int | 600 | Timeout in seconds (max: 3600) |
| `force_stdin` | No | bool | `false` | Force piping prompt via stdin. Auto-triggered for prompts >800 chars or containing special characters |

### `web_search` — Grok Web Search

| Parameter | Required | Type | Default | Description |
|-----------|----------|------|---------|-------------|
| `query` | **Yes** | string | — | Natural-language search query. Include constraints like topic, time range, language, or domain when helpful |
| `platform` | No | string | — | Focus on a specific platform (e.g., `"Twitter"`, `"GitHub"`, `"Reddit"`) |
| `min_results` | No | int | 3 | Minimum number of results to return |
| `max_results` | No | int | 10 | Maximum number of results to return |

### `web_fetch` — Fetch Web Content

| Parameter | Required | Type | Default | Description |
|-----------|----------|------|---------|-------------|
| `url` | **Yes** | string | — | A valid HTTP/HTTPS web address |

### `get_config_info` — Show Grok Configuration

No parameters. Returns current Grok configuration (API URL, model, retry settings) and tests API connectivity. API keys are read from environment variables only and never written to config files.

## Installation

### From source

```bash
cargo install --path .
```

### Build from source

```bash
git clone https://github.com/missdeer/aimcp.git
cd aimcp
cargo build --release
# Binary at target/release/aimcp
```

## Configuration

### Prerequisites

Install the CLI tools you want to use:

- **Gemini CLI** — `npm install -g @anthropic-ai/gemini-cli` or see [gemini-cli docs](https://github.com/google-gemini/gemini-cli)
- **Codex CLI** — `npm install -g @openai/codex` or see [codex docs](https://github.com/openai/codex)
- **Grok Search** — no binary needed, just set `GROK_API_URL` and `GROK_API_KEY`

### Environment Variables

#### Gemini

| Variable | Description |
|----------|-------------|
| `GEMINI_API_KEY` | API key for `gemini` tool, overrides `GOOGLE_API_KEY` on child process |
| `GEMINI_IMAGE_API_KEY` | API key for `gemini_image` tool, can differ from `GEMINI_API_KEY` |
| `GEMINI_API_URL` | Gemini API endpoint URL (shared by both tools), overrides `GOOGLE_GEMINI_BASE_URL` on child process |
| `GEMINI_BIN` | Override path to the gemini binary |
| `GEMINI_DEFAULT_TIMEOUT` | Default timeout in seconds (default: 600) |
| `GEMINI_FORCE_MODEL` | Default model for regular tasks (used when `gemini` tool has no model specified) |
| `GEMINI_IMAGE_MODEL` | Default model for image generation (used when `gemini_image` tool has no model specified) |
| `GEMINI_INCLUDE_DIRS` | Comma-separated extra directories for Gemini CLI `--include-directories` |

#### Codex

| Variable | Description |
|----------|-------------|
| `CODEX_BIN` | Override path to the codex binary |
| `CODEX_DEFAULT_TIMEOUT` | Default timeout in seconds (default: 600) |
| `CODEX_ALLOW_DANGEROUS` | Allow `danger-full-access` sandbox mode (`true`/`false`) |
| `CODEX_ALLOW_YOLO` | Allow yolo mode (`true`/`false`) |
| `CODEX_ALLOW_SKIP_GIT_CHECK` | Allow skipping git repo check (`true`/`false`) |

#### Grok Search

| Variable | Required | Description |
|----------|----------|-------------|
| `GROK_API_URL` | **Yes** | Grok API endpoint (e.g., `https://api.x.ai/v1`) |
| `GROK_API_KEY` | **Yes** | Grok API key |
| `GROK_MODEL` | No | Override default model (default: `grok-4-fast`) |
| `GROK_DEBUG` | No | Enable debug logging (`true`/`false`) |
| `GROK_RETRY_MAX_ATTEMPTS` | No | Max retry attempts (default: 3) |
| `GROK_RETRY_MULTIPLIER` | No | Backoff multiplier (default: 1.0) |
| `GROK_RETRY_MAX_WAIT` | No | Max retry wait in seconds (default: 10) |

## MCP Client Configuration

### Generic

```json
{
  "mcpServers": {
    "aimcp": {
      "command": "aimcp",
      "env": {
        "GEMINI_API_KEY": "your-gemini-api-key",
        "GEMINI_IMAGE_API_KEY": "your-gemini-image-api-key",
        "GEMINI_API_URL": "https://generativelanguage.googleapis.com",
        "GEMINI_FORCE_MODEL": "gemini-3.1-pro-preview",
        "GEMINI_IMAGE_MODEL": "gemini-3-pro-image-preview",
        "GROK_API_URL": "https://api.x.ai/v1",
        "GROK_API_KEY": "your-key"
      }
    }
  }
}
```

### Windsurf / Cursor / Claude Desktop

Add to your MCP settings file:

```json
{
  "mcpServers": {
    "aimcp": {
      "command": "aimcp",
      "env": {
        "GEMINI_API_KEY": "your-gemini-api-key",
        "GEMINI_IMAGE_API_KEY": "your-gemini-image-api-key",
        "GEMINI_API_URL": "https://generativelanguage.googleapis.com",
        "GEMINI_FORCE_MODEL": "gemini-3.1-pro-preview",
        "GEMINI_IMAGE_MODEL": "gemini-3-pro-image-preview",
        "GROK_API_URL": "https://api.x.ai/v1",
        "GROK_API_KEY": "xai-..."
      }
    }
  }
}
```

## Gemini Workspace Access

The Gemini CLI sandboxes file access to its working directory. When MCP hosts (e.g., Windsurf) set a custom CWD, Gemini may fail to access project files.

**aimcp handles this automatically**: on initialization, it requests the MCP client's workspace roots via the `roots/list` protocol and passes them to Gemini CLI as `--include-directories`.

If your MCP client does not support `roots/list`, set `GEMINI_INCLUDE_DIRS` as a fallback:

```json
{
  "mcpServers": {
    "aimcp": {
      "env": {
        "GEMINI_INCLUDE_DIRS": "D:/projects/myapp,D:/projects/other"
      }
    }
  }
}
```

## Startup Output

On startup, aimcp logs tool detection results to stderr:

```
[aimcp] Starting...
[aimcp] Tools detection:
  Gemini:  ✓ (/usr/local/bin/gemini)
  Codex:   ✗ (not found)
  Grok:    ✓ (API key configured)
```

## Architecture

```
aimcp/src/
├── main.rs           # Entry point: clap + UnifiedServer + AdaptiveStdio
├── lib.rs            # Module declarations
├── server.rs         # UnifiedServer: all tools + runtime availability checks
├── transport.rs      # AdaptiveStdio (JSONL/LSP auto-detection)
├── detection.rs      # Runtime tool availability detection
├── shared.rs         # Shared utilities (Job Object, timeouts, find_binary)
└── tools/
    ├── mod.rs
    ├── gemini.rs     # Gemini CLI wrapper
    ├── codex.rs      # Codex CLI wrapper with security policies
    └── grok/
        ├── mod.rs
        ├── config.rs     # Config singleton + env vars + persistence
        ├── prompts.rs    # Search/fetch prompt constants
        ├── provider.rs   # Grok API client with SSE streaming + retry
        └── tools.rs      # web_search, web_fetch, get_config_info, switch_model
```

## License

GPL-3.0-or-later
