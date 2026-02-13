# crates/core/

> ⚠️ **Please update me promptly.**

`fire-box-core` library crate — implements the gateway core logic.

## Responsibilities

- Provide an IPC HTTP server over an interprocess local socket (Axum)
- Manage app authentication and authorization (register, approve, revoke, per-model restrictions)
- Collect real-time metrics (token usage, request counts, active connections)
- Abstract LLM provider communication (OpenAI / Anthropic / DashScope / Copilot)
- Persist all configuration and credentials in the OS keyring

## Subdirectories

| Directory | Description |
|-----------|-------------|
| `src/`    | Library source code and module implementations |
| `tests/`  | Integration tests (require real credentials; marked `--ignored`) |

## Key dependencies

Important external crates: `axum` (HTTP framework), `interprocess` (local socket), `keyring` (OS secure storage), `reqwest` (HTTP client), `oauth2` (OAuth PKCE flows), `tokio` (async runtime).
