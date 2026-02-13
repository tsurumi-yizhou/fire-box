# crates/core/src/protocols/

> ⚠️ **Please update me promptly.**

LLM provider protocol codecs. Each file implements conversion between provider-specific request/response JSON and the unified internal types defined in `protocol.rs`.

## Modules

| File | Description |
|------|-------------|
| `mod.rs` | Module declaration, re-exporting submodules |
| `openai.rs` | OpenAI chat-completions codec. Handles `messages` (including `image_url`, file attachments), streaming SSE parsing, and usage extraction |
| `anthropic.rs` | Anthropic messages codec. Handles top-level `system` field, `document` blocks (base64 files), and Anthropic SSE event format (`content_block_delta` / `message_stop`) |
| `dashscope.rs` | DashScope (Qwen) compatibility mode. OpenAI-compatible with extra headers (`X-DashScope-*`), OAuth2 device-code flow (PKCE), automatic access token refresh, and local creds caching |
| `copilot.rs` | GitHub Copilot Chat API codec. Multi-step authentication: read GitHub token from local config/keyring → exchange for Copilot session token → issue OpenAI-format requests with VS Code-specific headers (SessionId, MachineId, Editor-Version) |
