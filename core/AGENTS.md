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

## UniFFI (UDL)

跨平台 FFI 使用 [UniFFI](https://mozilla.github.io/uniffi-rs/) 的 UDL 方式。

| File | Description |
|------|-------------|
| `src/core.udl` | 接口定义（WebIDL-like），声明导出给外部语言的函数 |
| `build.rs` | 编译时自动生成 C 头文件到 `generated/core.h`，并复制 `libcore.a` 到 `generated/libcore.a` |

`lib.rs` 通过 `uniffi::include_scaffolding!("core")` 引入生成的 Rust FFI 胶水代码。

## Key dependencies

Important external crates: `axum` (HTTP framework), `interprocess` (local socket), `keyring` (OS secure storage), `reqwest` (HTTP client), `oauth2` (OAuth PKCE flows), `tokio` (async runtime), `uniffi` (cross-platform FFI scaffolding), `uniffi_bindgen` (build-time foreign binding generation).
