# cli/

`fire-box` binary crate — command-line interface for Fire Box gateway.

## Responsibilities

- Provide CLI for managing providers and model mappings
- Start local HTTP server compatible with OpenAI/Anthropic APIs
- Display metrics and application authorizations
- Manage application access control

## Modules

| Module | Description |
|--------|-------------|
| `main.rs` | CLI argument parsing and command dispatch |
| `commands.rs` | Command handlers (provider, model, app, metrics) |
| `server.rs` | HTTP server for OpenAI/Anthropic-compatible API |

## Commands

### Provider Management

```sh
# List all providers
fire-box provider list

# Add a provider
fire-box provider add <tag> --type <openai|anthropic|dashscope|copilot> [--api-key <key>] [--base-url <url>]

# Remove a provider
fire-box provider remove <tag>
```

### Model Management

```sh
# List all model mappings
fire-box model list

# Add a model mapping
fire-box model add <model> --provider <tag> --provider-model <name>

# Remove a model mapping
fire-box model remove <model>
```

### Application Management

```sh
# List registered applications
fire-box app list

# Revoke application access
fire-box app revoke <app-id>
```

### Server

```sh
# Start HTTP server on default port (8080)
fire-box serve

# Start on custom port and host
fire-box serve --port 3000 --host 0.0.0.0
```

### Metrics

```sh
# Display current metrics
fire-box metrics
```

## HTTP Server Endpoints

The HTTP server provides OpenAI/Anthropic-compatible endpoints:

- `POST /v1/chat/completions` - Chat completions (OpenAI format)
- `POST /v1/completions` - Text completions
- `GET /health` - Health check

## Example Usage

```sh
# Configure providers
fire-box provider add openai-main --type openai --api-key sk-...
fire-box provider add anthropic-main --type anthropic --api-key sk-ant-...

# Configure models
fire-box model add gpt-4 --provider openai-main --provider-model gpt-4
fire-box model add claude-3-opus --provider anthropic-main --provider-model claude-3-opus-20240229

# Start server
fire-box serve --port 8080

# In another terminal, test with curl:
curl http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

## Key dependencies

- `clap` - Command-line argument parsing
- `axum` - HTTP server framework
- `core` - Fire Box core library
