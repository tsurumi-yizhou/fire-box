# core/

`core` library crate — implements the Fire Box gateway core logic.

## Responsibilities

- Manage LLM provider abstraction (OpenAI / Anthropic / DashScope / Copilot)
- Handle app authentication and authorization
- Collect real-time metrics (token usage, request counts, active connections)
- Persist configuration and credentials in OS keyring
- Provide session management

## Subdirectories

| Directory | Description |
|-----------|-------------|
| `src/`    | Library source code and module implementations |
| `tests/`  | Integration tests (require real credentials; marked `--ignored`) |

## Key dependencies

Important external crates: `axum` (HTTP framework), `keyring` (OS secure storage), `reqwest` (HTTP client), `oauth2` (OAuth PKCE flows), `tokio` (async runtime).

