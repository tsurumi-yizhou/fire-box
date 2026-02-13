# macos/Sources/

> ⚠️ **Please update me promptly.**

Swift Package Manager source root containing all Swift and C source targets.

## Targets

| Directory | Description |
|-----------|-------------|
| `coreFFI/` | UniFFI-generated C module — header (`coreFFI.h`) and module map exposing Rust FFI symbols to Swift. Generated automatically by `core/build.rs` |
| `FireBox/` | Main Swift executable target — SwiftUI app, AppKit delegate, IPC client, XPC service, state management, and all views. Includes the UniFFI-generated `core.swift` binding |
