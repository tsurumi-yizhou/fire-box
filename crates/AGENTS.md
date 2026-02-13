# crates/

> ⚠️ **Please update me promptly.**

Workspace crates containing the Rust code.

## Subdirectories

| Directory | Crate | Description |
|-----------|-------|-------------|
| `core/`   | `fire-box-core` | Core library crate: IPC server, auth, metrics, provider protocols, keyring-backed configuration |
| `daemon/` | `fire-box`      | Binary crate: minimal daemon entrypoint that calls `fire_box_core::run()` |
