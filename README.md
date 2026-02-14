# Fire Box

Stateful LLM API gateway with authentication and monitoring. Pure Rust workspace with CLI and FFI bindings.

## Features

- **Multi-protocol support**: OpenAI, Anthropic, DashScope (Qwen), GitHub Copilot
- **Local HTTP gateway**: OpenAI and Anthropic-compatible API endpoints
- **OAuth flows**: DashScope (PKCE) and Copilot (GitHub device code) automation
- **Secure storage**: Configuration and credentials persisted in OS keyring (Windows Credential Manager / macOS Keychain / Linux Secret Service)
- **Real-time metrics**: Token usage, request counts, active connections by model/provider/app
- **CLI management**: Command-line tools for provider and model configuration
- **FFI bindings**: UniFFI-based bindings for Swift, Kotlin, Python, and other languages

## Architecture

```
┌─────────────────┐
│  Client Apps    │ (HTTP requests to localhost:8080)
└────────┬────────┘
         │ (HTTP)
┌────────▼─────────┐
│  Fire Box CLI    │ (fire-box serve)
│  HTTP Gateway    │ (OpenAI/Anthropic compatible)
└────────┬─────────┘
         │
┌────────▼─────────┐
│  Core Library    │ (core crate)
│  - Auth          │ (app authorization)
│  - Provider      │ (protocol codecs, OAuth)
│  - Metrics       │ (usage tracking)
└────────┬─────────┘
         │ (HTTPS)
┌────────▼─────────┐
│  LLM Providers   │ (OpenAI, Anthropic, DashScope, Copilot)
└──────────────────┘
```

## Crates

- **core/** - Core library with LLM provider abstraction, auth, and metrics
- **ffi/** - UniFFI-based FFI bindings for native language integration
- **cli/** - Command-line interface for configuration and HTTP server

## Quick Start

### 1. Build

```sh
```sh
cargo build --release
```

### 2. Configure providers

Use the CLI to add LLM providers:

```sh
# Add OpenAI provider
./target/release/fire-box provider add openai-main --type openai --api-key sk-...

# Add Anthropic provider
./target/release/fire-box provider add anthropic-main --type anthropic --api-key sk-ant-...

# Add DashScope provider (with OAuth)
./target/release/fire-box provider add dashscope-main --type dashscope

# List providers
./target/release/fire-box provider list
```

### 3. Configure model mappings

Map model names to specific providers:

```sh
# Map gpt-4 to OpenAI
./target/release/fire-box model add gpt-4 --provider openai-main --provider-model gpt-4

# Map claude-3 to Anthropic
./target/release/fire-box model add claude-3-opus --provider anthropic-main --provider-model claude-3-opus-20240229

# List model mappings
./target/release/fire-box model list
```

### 4. Start HTTP server

Start a local OpenAI/Anthropic-compatible HTTP server:

```sh
./target/release/fire-box serve --port 8080
```

The server will be available at `http://localhost:8080` with OpenAI-compatible endpoints:
- `POST /v1/chat/completions` - Chat completions
- `POST /v1/completions` - Text completions
- `GET /health` - Health check

### 5. Use with OpenAI SDK

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:8080/v1",
    api_key="dummy"  # Not validated by Fire Box
)

response = client.chat.completions.create(
    model="gpt-4",
    messages=[{"role": "user", "content": "Hello!"}]
)
print(response.choices[0].message.content)
```

## CLI Commands

- `fire-box provider list` - List all providers
- `fire-box provider add <tag> --type <type> [--api-key <key>] [--base-url <url>]` - Add provider
- `fire-box provider remove <tag>` - Remove provider
- `fire-box model list` - List all model mappings
- `fire-box model add <model> --provider <tag> --provider-model <name>` - Add model mapping
- `fire-box model remove <model>` - Remove model mapping
- `fire-box app list` - List registered applications
- `fire-box app revoke <app-id>` - Revoke application access
- `fire-box serve [--port <port>] [--host <host>]` - Start HTTP server
- `fire-box metrics` - View current metrics

## Testing

### Unit tests

```sh
cargo test --lib
```

### Integration tests

Integration tests require real credentials or manual OAuth authorization:

```sh
# OpenAI test (requires OPENAI_API_KEY)
export OPENAI_API_KEY=sk-...
cargo test --test openai -- --nocapture --ignored

# Anthropic test (requires ANTHROPIC_AUTH_TOKEN)
export ANTHROPIC_AUTH_TOKEN=sk-ant-...
cargo test --test anthropic -- --nocapture --ignored

# DashScope OAuth test (interactive)
cargo test --test dashscope -- --nocapture --ignored

# Copilot OAuth test (interactive)
cargo test --test copilot -- --nocapture --ignored
```

## FFI Bindings

The `ffi/` crate provides UniFFI-based bindings for integration with:
- Swift (iOS, macOS)
- Kotlin (Android, JVM)
- Python
- Ruby
- Other UniFFI-supported languages

Build the FFI library:

```sh
cd ffi
cargo build --release
```

Generate bindings (example for Swift):

```sh
cargo run --bin uniffi-bindgen generate src/fire_box.udl --language swift
```

## License

This project is licensed under the MIT License. See the `LICENSE` file in the repository root for the full terms.
