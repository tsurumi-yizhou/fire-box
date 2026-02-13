# crates/core/tests/

# crates/core/tests/

> ⚠️ Please update me promptly.

Integration tests directory. These tests require real provider credentials and are marked `#[ignore]`. Run them with `cargo test -- --ignored`.

## Test files

| File | Description |
|------|-------------|
| `openai.rs` | OpenAI integration tests. Requires `OPENAI_API_KEY` (optional `OPENAI_BASE_URL`). |
| `anthropic.rs` | Anthropic integration tests. Requires `ANTHROPIC_AUTH_TOKEN` (optional `ANTHROPIC_BASE_URL`). |
| `dashscope.rs` | DashScope OAuth device-code integration test. Starts from zero-auth, prints an authorization URL and user code; complete authorization in a browser. |
| `copilot.rs` | GitHub Copilot integration tests. Requires a pre-configured GitHub token (local config file or keyring). |

## Running

```sh
# Run a single integration test
cargo test --test openai -- --nocapture --ignored
cargo test --test copilot -- --nocapture --ignored
cargo test --test dashscope -- --nocapture --ignored
cargo test --test anthropic -- --nocapture --ignored
```
| File | Description |
