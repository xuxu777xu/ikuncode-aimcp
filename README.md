# aimcp

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
| `codex` | Codex CLI | AI-assisted coding with sandbox policies |
| `web_search` | Grok API | Web search returning structured JSON results |
| `web_fetch` | Grok API | Fetch web page content as Markdown |
| `get_config_info` | Grok API | Show configuration and test API connectivity |
| `switch_model` | Grok API | Switch the Grok model and persist the setting |

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
| `GEMINI_BIN` | Override path to the gemini binary |
| `GEMINI_DEFAULT_TIMEOUT` | Default timeout in seconds (default: 600) |
| `GEMINI_FORCE_MODEL` | Force a specific model for all sessions |

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
        "GROK_API_URL": "https://api.x.ai/v1",
        "GROK_API_KEY": "xai-..."
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
