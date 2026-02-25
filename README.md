# aimcp

[English](README-en.md)

统一 AI MCP 服务器 — 一个 Rust 二进制文件，将 [Gemini CLI](https://github.com/google-gemini/gemini-cli)、[Codex CLI](https://github.com/openai/codex) 和 [Grok Search](https://x.ai/) 整合为一个 MCP 服务器。

## 特性

- **一个二进制，全部工具** — 只需配置一个 MCP 服务器，取代三个
- **运行时检测** — 启动时自动检测可用工具；不可用的工具在被调用时返回清晰的错误信息
- **AdaptiveStdio 传输** — 自动检测 JSONL 和 LSP 帧格式，最大化客户端兼容性
- **纯 Rust 的 GrokSearch** — 零 Python 依赖；通过 Grok API 实现 Web 搜索和内容抓取，支持 SSE 流式传输和重试

## 工具列表

| 工具 | 来源 | 描述 |
|------|------|------|
| `gemini` | Gemini CLI | AI 驱动的任务执行，支持会话连续性 |
| `gemini_image` | Gemini CLI | AI 图像生成，使用专用生图模型 |
| `codex` | Codex CLI | AI 辅助编码，支持沙箱策略 |
| `web_search` | Grok API | Web 搜索，返回结构化 JSON 结果 |
| `web_fetch` | Grok API | 抓取网页内容并转为 Markdown |
| `get_config_info` | Grok API | 显示配置信息并测试 API 连接 |

## 工具使用说明

### `gemini` — 执行 Gemini CLI

| 参数 | 必填 | 类型 | 默认值 | 描述 |
|------|------|------|--------|------|
| `PROMPT` | **是** | string | — | 发送给 Gemini 的任务指令 |
| `sandbox` | 否 | bool | `false` | 在沙箱模式下运行（隔离执行） |
| `SESSION_ID` | 否 | string | — | 恢复已有会话，用于多轮对话 |
| `return_all_messages` | 否 | bool | `false` | 返回所有消息（含推理过程和工具调用） |
| `model` | 否 | string | — | 模型覆盖。回退到 `GEMINI_FORCE_MODEL` 环境变量或 Gemini CLI 默认值 |
| `timeout_secs` | 否 | int | 600 | 超时时间，单位秒（1–3600） |

**返回结构：**
- `success` — 执行状态（布尔值）
- `SESSION_ID` — 用于恢复对话的唯一标识符
- `agent_messages` — 拼接的助手回复文本
- `all_messages` — （可选）`return_all_messages=true` 时返回完整的 JSON 事件
- `error` — `success=false` 时的错误描述

### `gemini_image` — Gemini 图像生成

| 参数 | 必填 | 类型 | 默认值 | 描述 |
|------|------|------|--------|------|
| `PROMPT` | **是** | string | — | 发送给 Gemini 的图像生成指令 |
| `sandbox` | 否 | bool | `false` | 在沙箱模式下运行（隔离执行） |
| `SESSION_ID` | 否 | string | — | 恢复已有会话，用于多轮对话 |
| `return_all_messages` | 否 | bool | `false` | 返回所有消息（含推理过程和工具调用） |
| `model` | 否 | string | — | 模型覆盖。回退到 `GEMINI_IMAGE_MODEL` 环境变量或 Gemini CLI 默认值 |
| `timeout_secs` | 否 | int | 600 | 超时时间，单位秒（1–3600） |

**返回结构：**
- `success` — 执行状态（布尔值）
- `SESSION_ID` — 用于恢复对话的唯一标识符
- `agent_messages` — 拼接的助手回复文本
- `all_messages` — （可选）`return_all_messages=true` 时返回完整的 JSON 事件
- `error` — `success=false` 时的错误描述

### `codex` — 执行 Codex CLI

| 参数 | 必填 | 类型 | 默认值 | 描述 |
|------|------|------|--------|------|
| `PROMPT` | **是** | string | — | 发送给 Codex 的任务指令 |
| `cd` | **是** | string | — | 工作目录路径 |
| `sandbox` | 否 | string | `"read-only"` | 沙箱策略：`"read-only"`、`"workspace-write"` 或 `"danger-full-access"` |
| `SESSION_ID` | 否 | string | — | 恢复之前的会话 |
| `skip_git_repo_check` | 否 | bool | `false` | 允许在 Git 仓库外运行 |
| `return_all_messages` | 否 | bool | `false` | 返回完整的推理轨迹 |
| `return_all_messages_limit` | 否 | int | 10000 | `return_all_messages` 为 true 时的最大消息数 |
| `image` | 否 | array | `[]` | 要附加的图片文件路径 |
| `model` | 否 | string | — | 覆盖 Codex 模型 |
| `yolo` | 否 | bool | `false` | 无需确认直接运行，跳过所有沙箱限制 |
| `profile` | 否 | string | — | `~/.codex/config.toml` 中的配置文件名 |
| `timeout_secs` | 否 | int | 600 | 超时时间，单位秒（最大 3600） |
| `force_stdin` | 否 | bool | `false` | 强制通过 stdin 传递 prompt。对于超过 800 字符或包含特殊字符的 prompt 会自动触发 |

### `web_search` — Grok Web 搜索

| 参数 | 必填 | 类型 | 默认值 | 描述 |
|------|------|------|--------|------|
| `query` | **是** | string | — | 自然语言搜索查询。可包含主题、时间范围、语言或域名等约束 |
| `platform` | 否 | string | — | 聚焦特定平台（如 `"Twitter"`、`"GitHub"`、`"Reddit"`） |
| `min_results` | 否 | int | 3 | 最少返回结果数 |
| `max_results` | 否 | int | 10 | 最多返回结果数 |

### `web_fetch` — 抓取网页内容

| 参数 | 必填 | 类型 | 默认值 | 描述 |
|------|------|------|--------|------|
| `url` | **是** | string | — | 有效的 HTTP/HTTPS 网址 |

### `get_config_info` — 显示 Grok 配置

无参数。返回当前 Grok 配置（API URL、模型、重试设置）并测试 API 连接。API Key 仅从环境变量读取，不会写入配置文件。

## 安装

### 方式一：下载预编译二进制（推荐）

从 [GitHub Releases](https://github.com/xuxu777xu/ai-cli-mcp/releases) 下载对应平台的二进制文件：

| 平台 | 文件 |
|------|------|
| Windows x64 | `aimcp-x86_64-pc-windows-msvc.exe` |
| macOS Apple Silicon | `aimcp-aarch64-apple-darwin` |
| macOS Intel | `aimcp-x86_64-apple-darwin` |
| Linux x64 | `aimcp-x86_64-unknown-linux-gnu` |

下载后放到 `PATH` 目录中即可使用。macOS / Linux 需要添加执行权限：

```bash
chmod +x aimcp-*
mv aimcp-* /usr/local/bin/aimcp
```

### 方式二：npm 安装

```bash
npm install -g @xuxu7777xu/aimcp
```

安装时自动从 GitHub Releases 下载预编译二进制。若下载失败则回退到 `cargo install`（需要 Rust 工具链）。

### 方式三：cargo 安装

```bash
cargo install --git https://github.com/xuxu777xu/ai-cli-mcp.git
```

### 方式四：从源码编译

```bash
git clone https://github.com/xuxu777xu/ai-cli-mcp.git
cd ai-cli-mcp
cargo build --release
# 二进制文件位于 target/release/aimcp
```

## 配置

### 前置条件

根据你需要的工具，安装对应的 CLI：

- **Gemini CLI** — `npm install -g @google/gemini-cli` 或参见 [gemini-cli 文档](https://github.com/google-gemini/gemini-cli)
- **Codex CLI** — `npm install -g @openai/codex` 或参见 [codex 文档](https://github.com/openai/codex)
- **Grok Search** — 无需安装，只需设置 `GROK_API_URL` 和 `GROK_API_KEY` 环境变量

### 环境变量

#### Gemini

| 变量 | 描述 |
|------|------|
| `GEMINI_API_KEY` | Gemini CLI 的 API 密钥（`gemini` 工具使用），设置后会覆盖子进程的 `GOOGLE_API_KEY` |
| `GEMINI_IMAGE_API_KEY` | 图像生成的 API 密钥（`gemini_image` 工具使用），可与 `GEMINI_API_KEY` 不同 |
| `GEMINI_API_URL` | Gemini API 端点 URL（两个工具共用），设置后会覆盖子进程的 `GOOGLE_GEMINI_BASE_URL` |
| `GEMINI_BIN` | 覆盖 gemini 二进制文件路径 |
| `GEMINI_DEFAULT_TIMEOUT` | 默认超时时间，单位秒（默认：600） |
| `GEMINI_FORCE_MODEL` | 常规任务的默认模型（当 `gemini` 工具未指定 model 时使用） |
| `GEMINI_IMAGE_MODEL` | 图像生成的默认模型（当 `gemini_image` 工具未指定 model 时使用） |
| `GEMINI_INCLUDE_DIRS` | 逗号分隔的额外目录，传给 Gemini CLI 的 `--include-directories` |

#### Codex

| 变量 | 描述 |
|------|------|
| `CODEX_BIN` | 覆盖 codex 二进制文件路径 |
| `CODEX_DEFAULT_TIMEOUT` | 默认超时时间，单位秒（默认：600） |
| `CODEX_ALLOW_DANGEROUS` | 允许 `danger-full-access` 沙箱模式（`true`/`false`） |
| `CODEX_ALLOW_YOLO` | 允许 yolo 模式（`true`/`false`） |
| `CODEX_ALLOW_SKIP_GIT_CHECK` | 允许跳过 Git 仓库检查（`true`/`false`） |

#### Grok Search

| 变量 | 必填 | 描述 |
|------|------|------|
| `GROK_API_URL` | **是** | Grok API 端点（如 `https://api.x.ai/v1`） |
| `GROK_API_KEY` | **是** | Grok API 密钥 |
| `GROK_MODEL` | 否 | 覆盖默认模型（默认：`grok-4-fast`） |
| `GROK_DEBUG` | 否 | 启用调试日志（`true`/`false`） |
| `GROK_RETRY_MAX_ATTEMPTS` | 否 | 最大重试次数（默认：3） |
| `GROK_RETRY_MULTIPLIER` | 否 | 退避乘数（默认：1.0） |
| `GROK_RETRY_MAX_WAIT` | 否 | 最大重试等待时间，单位秒（默认：10） |

## MCP 客户端配置

### 通用配置

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

添加到 MCP 配置文件：

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

## Gemini 工作区访问

Gemini CLI 将文件访问限制在其工作目录内。当 MCP 宿主（如 Windsurf）设置了自定义 CWD 时，Gemini 可能无法访问项目文件。

**aimcp 自动处理此问题**：初始化时，它通过 MCP `roots/list` 协议请求客户端的工作区根目录，并将其作为 `--include-directories` 传递给 Gemini CLI。

如果你的 MCP 客户端不支持 `roots/list`，可设置 `GEMINI_INCLUDE_DIRS` 作为备选方案：

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

## 启动输出

启动时，aimcp 会将工具检测结果输出到 stderr：

```
[aimcp] Starting...
[aimcp] Tools detection:
  Gemini:  ✓ (/usr/local/bin/gemini)
  Codex:   ✗ (not found)
  Grok:    ✓ (API key configured)
```

## 架构

```
aimcp/src/
├── main.rs           # 入口：clap + UnifiedServer + AdaptiveStdio
├── lib.rs            # 模块声明
├── server.rs         # UnifiedServer：所有工具 + 运行时可用性检查
├── transport.rs      # AdaptiveStdio（JSONL/LSP 自动检测）
├── detection.rs      # 运行时工具可用性检测
├── shared.rs         # 共享工具（Job Object、超时常量、find_binary）
└── tools/
    ├── mod.rs
    ├── gemini.rs     # Gemini CLI 包装器
    ├── codex.rs      # Codex CLI 包装器（含安全策略）
    └── grok/
        ├── mod.rs
        ├── config.rs     # 配置单例 + 环境变量 + 持久化
        ├── prompts.rs    # 搜索/抓取 prompt 常量
        ├── provider.rs   # Grok API 客户端（SSE 流式 + 重试）
        └── tools.rs      # web_search、web_fetch、get_config_info、switch_model
```

## 许可证

GPL-3.0-or-later
