# Fire Box

这是一个无状态的LLM API网关服务。使用Rust实现。
只允许使用已有依赖，禁止引入新依赖。
每次修改完运行 `cargo build`、`cargo test`、`cargo check`、`cargo clippy`，确保无 warning。

## 当前状态（2026-02-11）

- **核心功能**: 支持 OpenAI、Anthropic、DashScope 协议的请求/响应转换，支持同步与流式（SSE）模式。
- **会话**: 基于客户端源端口实现 session 管理，使用 UUID 标识会话。
- **路由**: 支持按 channel tag 与关键字匹配，并按 `routes.select` 中的 provider+model 顺序尝试上游。
- **文件管理**: 完整实现文件上传功能
  - **Channel 层接收**: 各协议的 `decode_request` 提取 base64 编码的文件（image、document）并存入文件管理器
  - **文件管理器**: `src/file_manager.rs` 使用内存 HashMap 存储文件，生成 UUID 作为 file_id
  - **Provider 层惰性上传**: 只在实际发送请求时（`encode_request`），从文件管理器读取文件并注入到上游请求中
  - 支持三种协议：OpenAI (image_url)、Anthropic (image/document)、DashScope (image_url/file)
- **DashScope 集成**: 已实现 DashScope（Qwen 兼容模式）支持：
  - 协议实现位于 `src/protocols/dashscope.rs`，处理 `file` block、`reasoning_content`、usage-only chunk 等特性。
  - 协议会在请求中附加 DashScope 特有 header（如 `X-DashScope-CacheControl`、`X-DashScope-UserAgent`）。
  - **Token 管理**: refresh_token 持久化到 `.dashscope_refresh_token` 文件，支持 token 轮转
- **重构**: 原 `codec_*` 模块已迁移到 `src/protocols/{openai,anthropic,dashscope}.rs`，并更新了所有引用。
- **provider 客户端重构**: `src/provider.rs` 抽象出协议层接口（endpoint_path、request_headers、encode/parse），移除重复实现。
- **质量保证**: 已运行 `cargo check`、`cargo clippy`、`cargo test`，当前无编译警告，单元测试全部通过。

## 配置要点

1. 在 `config.json` 的 `providers` 中添加 DashScope provider：

```json
{
  "tag": "DashScope",
  "type": "dashscope",
  "api_key": "sk-dashscope-xxxxx",
  "base_url": "https://api.qwen.example"
}
```

- 注意：最终上游 URL = `base_url` + `endpoint_path()`（当前 `endpoint_path()` 返回 `/chat/completions`）。若上游需要 prefix，如 `/compatible-mode/v1`，请把它包含在 `base_url` 中。

1. 在 `channels` 中添加 DashScope 类型的 channel（使客户端可按 DashScope/OpenAI-compatible API 调用网关）：

```json
{
  "type": "dashscope",
  "tag": "coding-dashscope",
  "port": 3002,
  "api_key": "your-channel-api-key"
}
```

1. 在 `routes` 的 `select` 中指定目标 provider 与 model（model 字符串会直接转发给上游）：

```json
{
  "select": [
    { "provider": "DashScope", "model": "qwen-large" }
  ]
}
```

## 下一步建议

- 若需要，我可以直接更新你的 `config.json`，加入上面的 provider/channel/route 示例并做一次简要的本地 smoke-test（curl 请求模拟）。
- 如需支持 multipart 文件上传，请确认是否允许为 `reqwest` 启用 `multipart` feature（会改变依赖配置）。

-- 自动记录：状态由开发代理在工作区修改后写入。
