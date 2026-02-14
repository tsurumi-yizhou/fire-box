# Fire Box

Stateful LLM API gateway with authentication and monitoring. Pure Rust workspace with CLI and HTTP server for local use.

## Constraints

- Only use existing dependencies; no new dependencies allowed without approval
- After each modification, run `cargo build`, `cargo test`, `cargo check`, `cargo clippy` to ensure no warnings
- All code must be production-ready and well-documented

## Directory Structure

```
.
├── core/                (Core library crate with LLM provider abstraction)
└── cli/                 (Command-line interface and HTTP server)
```

## Architecture

**Two-crate architecture**:
1. **CLI Layer**: Command-line tools and local HTTP server (OpenAI/Anthropic compatible)
2. **Core Library**: LLM provider abstraction, auth, metrics, keyring storage, protocol implementations

**Supported LLM Providers**: OpenAI, Anthropic, DashScope, GitHub Copilot (remote APIs)

See [core/AGENTS.md](core/AGENTS.md) for detailed core library documentation.

## Build

```sh
# Build all crates
cargo build

# Build release
cargo build --release

# Build specific crate
cargo build -p core
cargo build -p fire-box
```

## Quality Assurance

- ✅ `cargo build`: Compile pass, no warnings
- ✅ `cargo check`: No errors
- ✅ `cargo clippy`: No warnings
- ✅ `cargo test --lib`: All unit tests pass

Run full quality check:

```sh
cargo build && cargo check && cargo clippy && cargo test --lib
```

## Module Documentation

- [core/](core/) — Core library with LLM provider abstraction
- [cli/](cli/) — Command-line interface and HTTP server

## Usage

### Configure Providers

```sh
# Add OpenAI provider
fire-box provider add openai-main --type openai --api-key sk-...

# Add Anthropic provider
fire-box provider add anthropic-main --type anthropic --api-key sk-ant-...

# List providers
fire-box provider list
```

### Configure Model Mappings

```sh
# Map gpt-4 to OpenAI
fire-box model add gpt-4 --provider openai-main --provider-model gpt-4

# List model mappings
fire-box model list
```

### Start HTTP Server

```sh
# Start server on default port (8080)
fire-box serve

# Start on custom port
fire-box serve --port 3000 --host 0.0.0.0
```

### View Metrics

```sh
fire-box metrics
```

-- Auto-logged: status written by dev agent after workspace modifications.
