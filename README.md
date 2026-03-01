# Fire Box

A high-performance, cross-platform AI gateway written in Rust. It supports multiple protocols, including OpenAI, Anthropic, DashScope.

[![Linux](https://github.com/tsurumi-yizhou/fire-box/actions/workflows/linux.yml/badge.svg)](https://github.com/tsurumi-yizhou/fire-box/actions/workflows/linux.yml)
[![macOS](https://github.com/tsurumi-yizhou/fire-box/actions/workflows/macos.yml/badge.svg)](https://github.com/tsurumi-yizhou/fire-box/actions/workflows/macos.yml)
[![Windows](https://github.com/tsurumi-yizhou/fire-box/actions/workflows/windows.yml/badge.svg)](https://github.com/tsurumi-yizhou/fire-box/actions/workflows/windows.yml)

## Architecture

Fire Box consists of two main components:

- **Service** (`firebox-service`): Background service that manages AI providers and handles requests
- **Client SDK** (`firebox-client`): Rust SDK for interacting with the FireBox service

## Requirements

### Build Dependencies

- **CMake** 3.20 or later
- **Rust** 1.70 or later (with cargo)
- **Platform-specific:**
  - macOS: Xcode Command Line Tools, Swift 6.1+
  - Windows: Visual Studio 2019+ or MSVC build tools, WiX Toolset (for MSI packaging)
  - Linux: GCC/Clang, pkg-config

### Runtime Dependencies

- **macOS:** macOS 15.0 or later
- **Windows:** Windows 10 or later
- **Linux:** glibc 2.31+, xdg-utils (for URL scheme registration)

## Quick Start

### Building from Source

```bash
# Clone the repository
git clone https://github.com/tsurumi-yizhou/fire-box.git
cd fire-box

# Configure with CMake
cmake -B build

# Build
cmake --build build

# Create installation package
cd build
cpack
```

### Installing

**macOS:**
```bash
# Install the generated .pkg
sudo installer -pkg FireBox-1.1.0-macOS.pkg -target /
```

**Windows:**
```powershell
# Run the generated installer
.\FireBox-1.1.0-Windows.msi
```

**Linux:**
```bash
# Install the generated .deb package
sudo dpkg -i firebox_1.1.0_amd64.deb
```

### Using URL Scheme

After installation, you can configure providers using `firebox://` URLs:

```
firebox://add-provider?type=openai&name=OpenAI&config=eyJiYXNlX3VybCI6Imh0dHBzOi8vYXBpLm9wZW5haS5jb20vdjEifQ==
```

The application will prompt for confirmation before adding the provider.

## Client SDK Usage

The FireBox client SDK supports all three platforms with native IPC:

- **macOS**: XPC (synchronous API)
- **Linux**: D-Bus (async API with `tokio`)
- **Windows**: COM (synchronous API, stub implementation)

Add the client SDK to your `Cargo.toml`:

```toml
[dependencies]
firebox-client = { path = "path/to/fire-box/client" }
tokio = { version = "1", features = ["full"] }  # Required for Linux
```

### Basic Example (macOS/Windows)

```rust
use firebox_client::{FireBoxClient, CompletionRequest, ChatMessage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client
    let client = FireBoxClient::new()?;

    // Check if service is running
    client.ping()?;

    // Add a provider
    client.add_api_key_provider(
        "OpenAI",
        "openai",
        "sk-...",
        None,
    )?;

    // List available models
    let models = client.get_all_models(false)?;
    println!("Available models: {}", models.len());

    // Send a completion request
    let request = CompletionRequest {
        model_id: "gpt-4".to_string(),
        messages: vec![
            ChatMessage {
                role: "user".to_string(),
                content: "Hello!".to_string(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ],
        tools: vec![],
        temperature: Some(0.7),
        max_tokens: None,
    };

    let response = client.complete(&request)?;
    println!("Response: {}", response.content);

    Ok(())
}
```

### Basic Example (Linux)

```rust
use firebox_client::{FireBoxClient, CompletionRequest, ChatMessage};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client (async on Linux)
    let client = FireBoxClient::new().await?;

    // Check if service is running
    client.ping().await?;

    // Add a provider
    client.add_api_key_provider(
        "OpenAI",
        "openai",
        "sk-...",
        None,
    ).await?;

    // List available models
    let models = client.get_all_models(false).await?;
    println!("Available models: {}", models.len());

    // Send a completion request
    let request = CompletionRequest {
        model_id: "gpt-4".to_string(),
        messages: vec![
            ChatMessage {
                role: "user".to_string(),
                content: "Hello!".to_string(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ],
        tools: vec![],
        temperature: Some(0.7),
        max_tokens: None,
    };

    let response = client.complete(&request).await?;
    println!("Response: {}", response.content);

    Ok(())
}
```

### Streaming Example (macOS/Windows)

```rust
use firebox_client::{FireBoxClient, CompletionRequest, ChatMessage, StreamChunk};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = FireBoxClient::new()?;

    let request = CompletionRequest {
        model_id: "gpt-4".to_string(),
        messages: vec![
            ChatMessage {
                role: "user".to_string(),
                content: "Tell me a story".to_string(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ],
        tools: vec![],
        temperature: Some(0.7),
        max_tokens: None,
    };

    // Start streaming
    let stream_id = client.stream_start(&request)?;
    println!("Stream started: {}", stream_id);

    // Poll for chunks
    loop {
        let chunks = client.stream_poll(&stream_id)?;

        if chunks.is_empty() {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }

        for chunk in chunks {
            match chunk {
                StreamChunk::Delta(text) => print!("{}", text),
                StreamChunk::Done { usage, finish_reason } => {
                    println!("\n\nDone! Reason: {:?}", finish_reason);
                    if let Some(u) = usage {
                        println!("Tokens used: {}", u.total_tokens);
                    }
                    return Ok(());
                }
                StreamChunk::Error(err) => {
                    eprintln!("Error: {}", err);
                    return Err(err.into());
                }
                _ => {}
            }
        }
    }
}
```

### Provider Management (macOS/Windows)

```rust
use firebox_client::FireBoxClient;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = FireBoxClient::new()?;

    // Add API key provider
    client.add_api_key_provider("OpenAI", "openai", "sk-...", None)?;

    // Add OAuth provider (GitHub Copilot)
    let oauth_info = client.add_oauth_provider("Copilot", "copilot")?;
    println!("Visit: {}", oauth_info.verification_uri);
    println!("Code: {}", oauth_info.user_code);

    // Complete OAuth flow
    client.complete_oauth("copilot")?;

    // List providers
    let providers = client.list_providers()?;
    for provider in providers {
        println!("{}: {} ({})",
            provider.profile_id,
            provider.display_name,
            if provider.enabled { "enabled" } else { "disabled" }
        );
    }

    // Delete provider
    client.delete_provider("openai")?;

    Ok(())
}
```

### Provider Management (Linux)

```rust
use firebox_client::FireBoxClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = FireBoxClient::new().await?;

    // Add API key provider
    client.add_api_key_provider("OpenAI", "openai", "sk-...", None).await?;

    // Add OAuth provider (GitHub Copilot)
    let oauth_info = client.add_oauth_provider("Copilot", "copilot").await?;
    println!("Visit: {}", oauth_info.verification_uri);
    println!("Code: {}", oauth_info.user_code);

    // Complete OAuth flow
    client.complete_oauth("copilot").await?;

    // List providers
    let providers = client.list_providers().await?;
    for provider in providers {
        println!("{}: {} ({})",
            provider.profile_id,
            provider.display_name,
            if provider.enabled { "enabled" } else { "disabled" }
        );
    }

    // Delete provider
    client.delete_provider("openai").await?;

    Ok(())
}
```

### Embeddings

```rust
use firebox_client::{FireBoxClient, EmbeddingRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = FireBoxClient::new()?;

    let request = EmbeddingRequest {
        model_id: "text-embedding-ada-002".to_string(),
        input: vec![
            "Hello world".to_string(),
            "Rust programming".to_string(),
        ],
    };

    let response = client.embed(&request)?;
    println!("Generated {} embeddings", response.embeddings.len());
    println!("Embedding dimension: {}", response.embeddings[0].len());

    Ok(())
}
```

## IPC Status (Service)

- `macOS`: native XPC with native dictionary serialization.
- `Linux`: native D-Bus service interface (`zbus`) with typed method signatures.
- `Windows`: native Named Pipe transport with length-prefixed binary frames (`bincode`), not JSON payloads.

## Error Handling Policy

- Provider registration flows fail fast and now roll back partial writes (provider config / provider index / metadata) on failure.
- IPC handlers return explicit structured errors instead of silently ignoring persistence failures.
- Windows pipe server enforces a max frame size to prevent unbounded memory allocation on malformed input.
- IPC listener task failures are logged with context for operational diagnosis.

## Development

### Running Tests

```bash
# Run all tests
cargo test

# Run tests for specific package
cargo test --package firebox-client
cargo test --package firebox-service

# Run with output
cargo test -- --nocapture
```

### Code Quality

```bash
# Format code
cargo fmt

# Check for errors
cargo check

# Run clippy
cargo clippy --all-targets --all-features -- -D warnings
```

## License

This project is licensed under the MIT License.
