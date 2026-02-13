# Fire Box

Stateful LLM API gateway with authentication and monitoring. Rust core plus platform-native layers (Swift/C++).

## Features

- **Multi-protocol support**: OpenAI, Anthropic, DashScope (Qwen), GitHub Copilot
- **IPC architecture**: All requests use an interprocess local socket (named pipes / UDS); no public HTTP port is exposed
- **OAuth flows**: DashScope (PKCE) and Copilot (GitHub device code) automation paths
- **Secure storage**: Configuration and credentials are persisted in the OS keyring (Windows Credential Manager / macOS Keychain / Linux Secret Service)
- **App authorization**: Native Layer intercepts local app requests; user approval is required before an app may call the gateway
- **Real-time metrics**: Token usage, request counts, active connections, broken down by model/provider/app
- **Streaming**: SSE event streams for chat, metrics, auth, and OAuth notifications

## Architecture

```
┌─────────────────┐
│  Local Apps     │ (via COM/XPC)
└────────┬────────┘
         │
┌────────▼─────────┐
│  Native Layer    │ (Swift on macOS / C++ on Windows)
│  - COM/XPC service│ (forwards to IPC, displays GUI)
└────────┬─────────┘
         │ (interprocess local socket)
┌────────▼─────────┐
│  Rust Core       │ (fire-box-core)
│  - IPC server    │ (auth, routing, monitoring)
│  - Provider client│ (protocol codecs, OAuth)
└────────┬─────────┘
         │ (HTTPS)
┌────────▼─────────┐
│  LLM Providers   │ (OpenAI, Anthropic, DashScope, Copilot)
└──────────────────┘
```

## Quick Start

### 1. Build

```sh
cargo build --release
```

### 2. Run daemon

```sh
./target/release/fire-box
```

On startup the service loads configuration from the OS keyring. On first run the configuration may be empty; the service will start and wait for the Native Layer to configure providers via IPC.

### 3. Configure via IPC

The Native Layer uses the IPC endpoints to add providers and models (the examples use HTTP over the local socket):

**Add an OpenAI provider**:
```sh
# Windows (named pipe)
curl --unix-socket //./pipe/fire-box-ipc \
  -X POST http://localhost/ipc/v1/providers \
  -H "Content-Type: application/json" \
  -d '{"tag":"OpenAI","type":"openai","base_url":"https://api.openai.com/v1","credential":"sk-..."}'

# Unix (UDS)
curl --unix-socket /tmp/fire-box-ipc.sock \
  -X POST http://localhost/ipc/v1/providers \
  -H "Content-Type: application/json" \
  -d '{"tag":"OpenAI","type":"openai","base_url":"https://api.openai.com/v1","credential":"sk-..."}'
```

**Add a model mapping**:
```sh
curl --unix-socket /tmp/fire-box-ipc.sock \
  -X POST http://localhost/ipc/v1/models \
  -H "Content-Type: application/json" \
  -d '{"tag":"gpt-4","provider_mappings":[{"provider":"OpenAI","model_id":"gpt-4"}]}'
```

OAuth providers (DashScope, Copilot) will trigger the device-code flow on first use; the Native Layer receives `oauth_open_url` events and can notify the user.

## Testing

### Unit tests

```sh
cargo test
```

Currently 17 unit tests pass (auth, metrics, models, protocols).

### Integration tests

Integration tests require real credentials or manual OAuth authorization; use `--ignored` and `--nocapture` when running:

```sh
cargo test --test protocol -- --nocapture --ignored
```

**OpenAI and Anthropic tests** require environment variables:

```sh
export OPENAI_API_KEY=sk-...
export OPENAI_BASE_URL=https://api.openai.com/v1  # optional
export ANTHROPIC_AUTH_TOKEN=sk-ant-...
export ANTHROPIC_BASE_URL=https://api.anthropic.com  # optional

cargo test --test protocol test_openai -- --nocapture --ignored
cargo test --test protocol test_anthropic -- --nocapture --ignored
```

**DashScope and Copilot OAuth tests** start from zero-auth and print an authorization URL and user code; copy the URL into a browser and complete the authorization:

```sh
cargo test --test protocol test_dashscope_oauth -- --nocapture --ignored
cargo test --test protocol test_copilot_oauth -- --nocapture --ignored
```

Tests print output such as:

```
╔════════════════════════════════════════════════════════════════╗
║ GitHub Copilot OAuth Authorization Required                   ║
╠════════════════════════════════════════════════════════════════╣
║ Provider:   Copilot-Test                                       ║
║ URL:        https://github.com/login/device                    ║
║ User Code:  ABCD-1234                                          ║
╚════════════════════════════════════════════════════════════════╝

👉 Copy the URL above to your browser and enter the user code.
```

## Service unit

An example `systemd` unit is provided as `fire-box.service` for reference:

```sh
sudo cp fire-box.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now fire-box
```

## License

This project is licensed under the Mozilla Public License 2.0 (MPL-2.0). See the `LICENSE` file in the repository root for the full terms.
