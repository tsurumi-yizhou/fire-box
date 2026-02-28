# AGENTS.md

## Project Overview

Fire Box is a cross-platform AI gateway service written in Rust that provides unified access to multiple AI providers (OpenAI, Anthropic, GitHub Copilot, DashScope, llama.cpp). It runs as a system service (systemd on Linux, launchd on macOS, Windows Service) and exposes platform-specific IPC interfaces for client applications.

---

## Principles

### Naming Conventions

- **Variables and functions**: Always use `snake_case`. No exceptions.
- **Types and traits**: Use `PascalCase` (`ProviderConfig`, `StreamEvent`).
- **Constants**: Use `SCREAMING_SNAKE_CASE` (`MAX_RETRIES`, `DEFAULT_TIMEOUT`).
- **File names**: Use a single word (`config.rs`, `route.rs`, `metrics.rs`). Only use compound names when necessary, and then use `snake_case` (`native_core`).
- **Module paths**: Keep short and descriptive (`providers::openai`, not `providers::openai_provider`).

### Implementation Principles

1. **Async-first**: All I/O-bound operations (network calls, file access, IPC) must be `async`. Use `tokio` as the async runtime. Block only when interfacing with synchronous platform APIs (e.g., XPC), and wrap those with `tokio::task::spawn_blocking` or equivalent.

2. **Proper error handling — no `unwrap()`, no silent failures**:
   - Use `Result<T, E>` for all fallible operations. Propagate errors with `?`.
   - Define domain-specific error types with `thiserror` for library code. Use `anyhow` only in top-level entry points or tests where context is sufficient.
   - **Never** call `.unwrap()` or `.expect()` in production code. The only exception is in tests, where `.unwrap()` is acceptable for brevity.
   - **Never** swallow errors with `let _ = ...` or `println!`. Log errors with `tracing` or `log` at appropriate levels (`error!`, `warn!`), then propagate or handle them.
   - For streams and channels, handle `None` / closed-channel cases explicitly.

3. **Test coverage**:
   - Every public function and trait method must have at least one test.
   - Every provider must have a dedicated test file in `service/tests/`.
   - Every middleware module must have unit tests (inline `#[cfg(test)]`) or integration tests.
   - Test both happy paths and error paths (invalid input, network failure, timeout).
   - Use mock/stub providers for integration tests — do not depend on live API keys in CI.

4. **Security by default**:
   - Never store API keys or secrets in plain text.
   - Validate all inputs at system boundaries (IPC messages, HTTP responses).
   - Use constant-time comparison for secrets when applicable.
   - Zeroize sensitive data in memory when done.

5. **Minimal changes, incremental verification**:
   - Make the smallest meaningful change, then verify before moving on.
   - Do not batch multiple unrelated changes into one step.

6. **Prefer existing tool calls over bash**:
   - Use the provided tool functions (e.g., `ReadFile`, `StrReplaceFile`, `Grep`, `Glob`) for file operations, searching, and text manipulation.
   - Avoid using bash commands via `Shell` for tasks that can be accomplished with dedicated tools.
   - Reserve `Shell` for operations that truly require a shell environment (e.g., running build commands, executing scripts).

7. **Native-first: embrace platform-native capabilities**:
   - Leverage platform-native frameworks and APIs before introducing third-party dependencies.
   - On macOS: use `NSUserDefaults`, `Keychain`, `XPC`, `SwiftUI`.
   - On Windows: use `Windows Credential Manager`, `COM`, `WinUI 3`.
   - On Linux: use `Secret Service API`, `D-Bus`, `GTK 4` / `Adwaita`.
   - This reduces dependencies, improves integration, and respects platform conventions.

### Development Workflow

After **every minimal change**, run the following commands in order from the `service/` directory:

```bash
# 1. Format — auto-fix style issues
cargo fmt

# 2. Check — fast type-checking without full compilation
cargo check

# 3. Clippy — catch common mistakes and enforce idioms
cargo clippy -- -D warnings

# 4. Build — full compilation
cargo build

# 5. Test — run the full test suite
cargo test
```

All five must pass before considering a change complete. If any step fails, fix the issue before proceeding. Do not skip steps.

### Platform-Specific Code

Use conditional compilation for platform differences:

```rust
#[cfg(target_os = "macos")]
pub mod xpc;

#[cfg(target_os = "linux")]
pub mod dbus;

#[cfg(target_os = "windows")]
pub mod windows_com;
```

When adding platform-specific logic inside shared code, prefer `cfg` attributes over runtime checks. Keep platform-specific modules isolated — shared logic belongs in common modules.

---

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

---

## Architecture

### Three-Layer Design

1. **Interfaces Layer** (`service/src/interfaces/`): Platform-specific IPC
   - macOS: XPC via `xpc.rs`, `capability.rs`, `control.rs`, `xpc_codec.rs`
   - Linux: D-Bus via `dbus.rs`
   - Windows: COM LocalServer32 via `windows_com.rs`
   - Shared: `connections.rs` (macOS/Windows), `native_core.rs` (JSON dispatch)

2. **Middleware Layer** (`service/src/middleware/`): Cross-cutting concerns
   - `access.rs`: TOFU (Trust On First Use) access control enforcement
   - `config.rs`: AES-256-GCM encrypted configuration (`fire-box-store.enc`)
   - `route.rs`: Virtual model routing with failover/random strategy
   - `metrics.rs`: Lock-free atomic request/token/cost tracking
   - `metadata.rs`: Model capabilities from models.dev API and GGUF headers
   - `storage.rs` (in keyring crate): Secure credential storage in native keyrings

3. **Provider Layer** (`service/src/providers/`): AI service integrations
   - `openai.rs`: OpenAI and compatible APIs (Ollama, vLLM)
   - `anthropic.rs`: Anthropic Claude API
   - `copilot.rs`: GitHub Copilot with OAuth device flow
   - `dashscope.rs`: Alibaba DashScope (Qwen models)
   - `llamacpp.rs`: Local llama.cpp subprocess management
   - `retry.rs`: Exponential backoff retry logic
   - `config.rs`: Provider configuration serialization
   - `consts.rs`: API endpoint URLs and constants
   - `shared.rs`: Common helper functions

### Key Abstractions

**Provider Trait** (`service/src/providers/mod.rs`):
- `complete()`: Non-streaming chat completion
- `complete_stream()`: Streaming chat with SSE
- `embed()`: Text embeddings
- `list_models()`: Model discovery
- `ProviderDyn`: Object-safe wrapper for dynamic dispatch

**Routing System**:
- Clients request virtual model IDs (e.g., `coding-model`, `fast-chat`)
- Routes map to ordered provider+model targets for failover
- Automatic retry on transient failures (network, rate limits)
- Strategies: `Failover` (ordered) and `Random` (load distribution)

**Security**:
- Trust On First Use (TOFU) access control with helper-based approval
- Credentials stored in OS keyring (Keychain/Credential Manager/Secret Service)
- Configuration encrypted at rest with AES-256-GCM
- Direct provider connections (no relay servers)

---

## Implementation Status

### Service Core — Complete
- `main.rs`: Platform-specific service entry (Windows Service, systemd, launchd)
- Graceful shutdown with atomic flag and signal handling
- Global tokio runtime for async operations

### Interfaces Layer — Complete (all platforms)
| Module | Platform | Status |
|--------|----------|--------|
| `xpc.rs` | macOS | Implemented — Mach XPC listener, protocol dispatch |
| `capability.rs` | macOS | Implemented — TOFU-gated AI capabilities |
| `control.rs` | macOS | Implemented — Provider/route/metrics management |
| `xpc_codec.rs` | macOS | Implemented — XPC marshalling helpers |
| `dbus.rs` | Linux | Implemented — D-Bus service interface |
| `windows_com.rs` | Windows | Implemented — COM LocalServer32 |
| `connections.rs` | macOS/Windows | Implemented — Connection registry |
| `native_core.rs` | macOS/Windows | Implemented — JSON dispatch layer |

### Middleware Layer — Complete
| Module | Status |
|--------|--------|
| `access.rs` | Implemented — TOFU access control |
| `config.rs` | Implemented — AES-256-GCM encrypted storage |
| `route.rs` | Implemented — Failover/random routing strategies |
| `metrics.rs` | Implemented — Atomic counters, cost tracking |
| `metadata.rs` | Implemented — models.dev + GGUF metadata |

### Provider Layer — Complete
| Module | Status |
|--------|--------|
| `openai.rs` | Implemented — OpenAI + compatible (Ollama, vLLM) |
| `anthropic.rs` | Implemented — Claude API |
| `copilot.rs` | Implemented — GitHub Copilot OAuth device flow |
| `dashscope.rs` | Implemented — DashScope/Qwen |
| `llamacpp.rs` | Implemented — Local subprocess management |
| `retry.rs` | Implemented — Exponential backoff |
| `config.rs` | Implemented — Config serialization |

### GUI Applications — Complete (all platforms)
| Platform | Framework | Status |
|----------|-----------|--------|
| macOS | SwiftUI | Implemented — Dashboard, providers, models, connections views |
| Windows | WinUI 3 (C#) | Implemented — Dashboard, providers, routes, allowlist, connections pages |
| Linux | GTK 4 / Adwaita (C++) | Implemented — Main window, D-Bus client, helper dialog |

### Test Suite — 13 integration tests
| Test File | Coverage Area |
|-----------|---------------|
| `anthropic_provider.rs` | Anthropic provider |
| `copilot_provider.rs` | GitHub Copilot provider |
| `dashscope_provider.rs` | DashScope provider |
| `llamacpp_provider.rs` | llama.cpp provider |
| `openai_provider.rs` | OpenAI provider |
| `integration.rs` | End-to-end routing, metrics, config roundtrips |
| `metadata_middleware.rs` | Metadata manager |
| `provider_config.rs` | Provider configuration |
| `provider_tests.rs` | Generic provider behavior |
| `provider_trait.rs` | Trait conformance |
| `provider_types.rs` | Type serialization |
| `retry_tests.rs` | Retry logic |
| `route_middleware.rs` | Routing and failover |

---

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

---

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

---

## Documentation References

- **SERVICE.md**: Comprehensive service architecture and middleware design
- **CAPABILITY.md**: Service Protocol for AI capability consumption (IPC spec)
- **CONTROL.md**: Management Protocol for configuration and monitoring (IPC spec)
- **ACCESS.md**: TOFU access control specification
- **APPLICATION.md**: Application architecture for all platform GUIs

These documents use formal technical language and provide the authoritative specification for the system design.
