# AGENTS.md

## Project Overview

Fire Box is a cross-platform AI gateway service written in Rust that provides unified access to multiple AI providers (OpenAI, Anthropic, GitHub Copilot, DashScope, llama.cpp). It runs as a system service (systemd on Linux, launchd on macOS, Windows Service) and exposes platform-specific IPC interfaces for client applications.

## Build Commands

### Full Build (All Platforms)

```bash
# Configure with CMake
cmake -B build

# Build everything
cmake --build build

# Create installation package
cd build
cpack
```

### Platform-Specific Builds

**Rust Service Only:**
```bash
cd service
cargo build --release
cargo test
```

**macOS App:**
```bash
cd macos
swift build -c release
```

**Windows App:**
```bash
cd windows/App
dotnet publish -c Release --self-contained
```

**Linux GUI:**
```bash
meson setup build linux --prefix=/usr
meson compile -C build
```

### Testing

```bash
# Run all Rust tests
cd service
cargo test

# Run specific test file
cargo test --test integration

# Run tests with output
cargo test -- --nocapture
```

## Architecture

### Three-Layer Design

1. **IPC Layer** (`service/src/ipc/`): Platform-specific communication
   - macOS: XPC (Mach IPC)
   - Linux: D-Bus (planned)
   - Windows: Named Pipes (planned)

2. **Middleware Layer** (`service/src/middleware/`): Cross-cutting concerns
   - `storage.rs`: Secure credential storage in native platform keyrings
   - `config.rs`: AES-256-GCM encrypted configuration (`fire-box-store.enc`)
   - `route.rs`: Virtual model routing with failover chains
   - `metrics.rs`: Lock-free atomic request/token/cost tracking
   - `metadata.rs`: Model capabilities fetched from models.dev API

3. **Provider Layer** (`service/src/providers/`): AI service integrations
   - `openai.rs`: OpenAI and compatible APIs (Ollama, vLLM)
   - `anthropic.rs`: Anthropic Claude API
   - `copilot.rs`: GitHub Copilot with OAuth device flow
   - `dashscope.rs`: Alibaba DashScope (Qwen models)
   - `llamacpp.rs`: Local llama.cpp inference

### Key Abstractions

**Provider Trait** (`service/src/providers/mod.rs`):
- `complete()`: Non-streaming chat completion
- `complete_stream()`: Streaming chat with SSE
- `embed()`: Text embeddings
- `list_models()`: Model discovery

**Routing System**:
- Clients request virtual model IDs (e.g., `coding-model`, `fast-chat`)
- Routes map to ordered provider+model targets for failover
- Automatic retry on transient failures (network, rate limits)

**Security**:
- Trust On First Use (TOFU) access control
- Credentials stored in OS keyring (Keychain/Credential Manager/Secret Service)
- Configuration encrypted at rest with AES-256-GCM
- Direct provider connections (no relay servers)

## Project Structure

```
fire-box/
├── service/          # Rust backend service
│   ├── src/
│   │   ├── ipc/      # Platform IPC implementations
│   │   ├── middleware/  # Storage, config, routing, metrics
│   │   └── providers/   # AI provider integrations
│   └── tests/        # Integration tests
├── macos/            # Swift macOS app and helper
├── windows/          # C# .NET Windows app and helper
├── linux/            # C++ GTK4/Adwaita Linux GUI
├── client/           # Rust client library (planned)
└── docs/
    ├── SERVICE.md    # Service architecture design
    ├── CAPABILITY.md # Service Protocol IPC spec
    ├── CONTROL.md    # Management Protocol IPC spec
    └── AGENTS.md     # Implementation status tracker
```

## Important Implementation Details

### Configuration Storage

- **Location**: `~/.config/fire-box/fire-box-store.enc` (or platform equivalent)
- **Encryption**: AES-256-GCM with key stored in OS keyring
- **Contents**: Provider index, route rules, model states, display names
- **Access**: Use `load_config()` and `update_config()` from `middleware::config`

### Credential Management

- **Never** store API keys in plain text files
- Use `middleware::storage::{set_secret, get_secret, delete_secret}`
- Keys stored with service name `fire-box` and account format `provider:{provider_id}`
- Supports biometric protection on macOS/Windows

### Routing Configuration

Routes are defined as virtual model IDs mapping to ordered target lists:

```rust
// Example route with failover
{
  "coding-model": [
    {"provider_id": "openai-1", "model": "gpt-4"},
    {"provider_id": "anthropic-1", "model": "claude-3-opus"}
  ]
}
```

Access via `middleware::route::{set_route_rules, get_route_targets, get_next_target}`

### Adding New Providers

1. Implement the `Provider` trait in `service/src/providers/`
2. Add provider type to `ProviderConfig` enum
3. Update routing logic to instantiate provider
4. Add tests in `service/tests/`
5. Update metadata mapping if needed

### Platform-Specific Code

Use conditional compilation for platform differences:

```rust
#[cfg(target_os = "macos")]
pub mod xpc;

#[cfg(target_os = "linux")]
pub mod dbus;

#[cfg(target_os = "windows")]
pub mod named_pipes;
```

## Common Development Tasks

### Running the Service Locally

```bash
cd service
RUST_LOG=debug cargo run
```

### Debugging IPC on macOS

```bash
# Check if service is running
launchctl list | grep firebox

# View service logs
log stream --predicate 'subsystem == "com.firebox.service"'
```

### Updating Dependencies

```bash
# Update workspace dependencies in root Cargo.toml
# Then update service/Cargo.toml to use workspace versions
cargo update
```

### Cross-Platform Compilation

The project uses CMake as the top-level build system to coordinate Rust (service), Swift (macOS), .NET (Windows), and Meson (Linux) builds. When adding new files, update the appropriate platform build configuration.

## Documentation References

- **SERVICE.md**: Comprehensive service architecture and middleware design
- **CAPABILITY.md**: Service Protocol for AI capability consumption (IPC spec)
- **CONTROL.md**: Management Protocol for configuration and monitoring (IPC spec)
- **AGENTS.md**: Current implementation status and gaps

These documents use formal technical language and provide the authoritative specification for the system design.
