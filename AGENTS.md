# Fire Box

这是一个无状态的LLM API网关服务。使用Rust实现。
只允许使用已有依赖，禁止引入新依赖。
每次修改完运行 `cargo build`、`cargo test`、`cargo check`、`cargo clippy`，确保无 warning。

## 当前状态（2026-02-12）

### 架构概述

- **服务模式**: 创建2个服务端点（在配置文件中指定）：
  - Unix Socket: 可配置路径，支持 ~ 展开（默认 `/tmp/fire-box.sock`）**（仅限 Unix/Linux/macOS）**
  - TCP 端口: 可配置地址（默认 `localhost:3000`）**（跨平台）**
- **Windows 支持**: 在 Windows 上 Unix Socket 功能被禁用，仅启用 TCP 服务器
- **协议支持**: 两个端点都通过路径区分协议，所有路径统一带 `/v1` 前缀：
  - OpenAI 协议：`/v1/chat/completions`（对话）、`/v1/models`（模型列表）、`/v1/embeddings`（嵌入）
  - Anthropic 协议：`/v1/messages`
  - 文件管理：`/v1/files`（上传/列表）、`/v1/files/{file_id}`（查询/删除）、`/v1/files/{file_id}/content`（下载）
- **Provider Fallback**: 每个 model 配置包含优先级排序的 provider 映射列表，每个映射指定 provider 和对应的 model_id，支持自动 fallback
- **模型路由**: 从请求体的 `model` 字段动态查找模型配置，支持多模型共享同一服务端点
- **模型元数据**: 启动时从 models.dev API 下载模型能力信息，在请求日志中记录模型能力
- **文件管理**: 完整实现文件上传功能
  - 各协议的 `decode_request` 提取 base64 编码的文件并存入文件管理器
  - 文件管理器使用内存 HashMap 存储，生成 UUID 作为 file_id
  - Provider 层在 `encode_request` 时惰性上传
  - 支持三种协议：OpenAI (image_url)、Anthropic (image/document)、DashScope (image_url/file)

### 核心功能

- **协议实现**: 
  - OpenAI: `src/protocols/openai.rs`
  - Anthropic: `src/protocols/anthropic.rs`
  - DashScope: `src/protocols/dashscope.rs`（Qwen 兼容模式，支持 `file` block、`reasoning_content`、usage-only chunk 等特性）
- **模型元数据**: `src/models.rs` 从 models.dev API 下载并管理模型能力信息
  - ModelRegistry: 存储和查询模型元数据
  - ModelCapabilities: 描述模型功能（tool_call, reasoning）和输入输出模态（text, image, pdf等）
  - 启动时异步加载，请求处理时记录能力信息
- **DashScope Token 管理**: 从 `oauth_creds_path` 指定的 JSON 文件中读取 access_token/refresh_token，access_token 过期时自动刷新并回写文件
- **Gateway Servers**: `src/server.rs` 创建2个服务端点（1个Unix socket + 1个TCP端口）并处理请求
- **Provider 客户端**: `src/provider.rs` 抽象出协议层接口（endpoint_path、request_headers、encode/parse）

### 配置结构

```json
{
  "log": { "level": "info" },
  "service": {
    "uds": "~/.fire-box.sock",
    "tcp": "localhost:3000"
  },
  "providers": [
    {
      "tag": "OpenAI",
      "type": "openai",
      "api_key": "...",
      "base_url": "https://api.openai.com/v1"
    },
    {
      "tag": "Anthropic",
      "type": "anthropic",
      "auth_token": "...",
      "base_url": "https://api.anthropic.com/v1"
    },
    {
      "tag": "通义千问",
      "type": "dashscope",
      "oauth_creds_path": "~/.qwen/oauth_creds.json"
    }
  ],
  "models": {
    "gpt-5.2": [
      {
        "provider": "OpenRouter",
        "model_id": "openai/gpt-5.2-chat"
      },
      {
        "provider": "OpenAI",
        "model_id": "gpt-5.2"
      }
    ]
  }
}
```

**配置说明**:
- `service`: 服务端点配置
  - `uds`: Unix socket 路径，支持 ~ 展开（Windows 上此配置被忽略）
  - `tcp`: TCP 监听地址
- `models` 现在是一个 HashMap，键为统一的模型标签（如 `gpt-5.2`）
- 每个模型包含一个 provider 映射列表，按优先级排序
- 每个映射包含：
  - `provider`: provider 的 tag
  - `model_id`: 该 provider 使用的实际模型 ID
- 支持不同 provider 使用不同的模型 ID（例如 OpenRouter 使用 `openai/gpt-5.2-chat`，而 OpenAI 使用 `gpt-5.2`）

### 已废弃

- ❌ 每个模型一个 Unix socket（现在只有一个统一的 socket）
- ❌ 命令行指定端口（现在在配置文件中指定）
- ❌ API key 验证（本地 Unix socket 无需验证）

## 质量保证

- ✅ `cargo build`: 编译通过，无警告
- ✅ `cargo check`: 无错误
- ✅ `cargo clippy`: 无警告

## 使用方式

1. 配置文件中定义 service、models 和 providers
2. 启动服务：`./fire-box config.json`
3. 服务创建端点（根据平台）：
   - **Unix/Linux/macOS**: Unix Socket + TCP（两种协议，根据 path 区分）
   - **Windows**: 仅 TCP（Unix Socket 不支持）
4. 客户端通过端点连接，使用对应协议路径：
   - `/v1/chat/completions` - OpenAI 协议
   - `/v1/messages` - Anthropic 协议
5. 网关从请求体的 `model` 字段查找配置，按优先级尝试 providers，支持 fallback

-- 自动记录：状态由开发代理在工作区修改后写入。
