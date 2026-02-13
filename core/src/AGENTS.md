# crates/core/src/

> ⚠️ **Please update me promptly.**

Source directory for the `fire-box-core` library crate.

## Modules

| File | Description |
|------|-------------|
| `lib.rs` | Crate entrypoint. Defines the shared `CoreState`, and `run()` which loads configuration from the keyring and starts the IPC server |
| `ipc.rs` | IPC server. Axum HTTP over an interprocess local socket, exposing REST endpoints (chat, auth, config CRUD) and SSE events (`auth_required`, `metrics_update`, `request_log`, `oauth_open_url`) |
| `auth.rs` | App authentication/authorization management. Maintains `AppAuthorization` records, supports register/approve/revoke and per-model restrictions, persisted to keyring |
| `provider.rs` | Provider client. Dispatches requests to protocol codecs based on provider type, supports streaming and non-streaming responses and fallback logic |
| `protocol.rs` | Unified internal types: `UnifiedRequest`, `UnifiedMessage`, `StreamEvent`, etc. Protocol codecs translate between this internal format and provider-specific formats |
| `config.rs` | Runtime configuration types. `Config` loads/saves providers, models, and settings to the keyring; re-exports keystore types |
| `keystore.rs` | OS keyring abstraction. Stores and retrieves provider API keys, auth tokens, app authorizations and full service configuration under service name "fire-box" |
| `metrics.rs` | Real-time metrics. Lock-free atomic counters and per-entity metrics broken down by model/provider/app |
| `models.rs` | Model metadata. Loads capabilities from models.dev (reasoning, tool_call, modalities, etc.) and builds a `ModelRegistry` |
| `session.rs` | Session management. Assigns stable UUIDs to client source ports for session lifetime |
| `filesystem.rs` | In-memory file storage. Stores uploaded files as base64, indexed by UUID for cross-provider attachments |

## Subdirectories

| Directory | Description |
|-----------|-------------|
| `protocols/` | LLM provider protocol codecs (one file per provider) |
