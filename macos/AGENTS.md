# macos/

> ⚠️ **Please update me promptly.**

macOS native layer — SwiftUI menu-bar GUI + XPC service + launchd agent configuration.

## Architecture

The Rust core (`libcore.a`) is linked as a static library into the Swift executable. On launch, `run_from_args()` is called on a background thread to start the IPC server. The Swift GUI then communicates with the core entirely through HTTP-over-Unix-domain-socket IPC (no FFI beyond the initial entry point).

## Build

```sh
# 1. Build the Rust static library (produces generated/libcore.a + generated/core.h)
cd core && cargo build

# 2. Build the Swift executable (SPM)
cd macos && swift build
```

## Subdirectories

| Path | Description |
|------|-------------|
| `Sources/CFireBoxCore/` | C bridge module — exposes `run_from_args()` header for Swift import |
| `Sources/FireBox/` | Main Swift executable — app entry, delegates, state management, IPC client, XPC service |
| `Resources/` | launchd plist and other runtime resources |

## Key files

| File | Description |
|------|-------------|
| `Package.swift` | Swift Package Manager manifest. Product `FireBox`. Targets: `Core` (C headers) and `FireBox` (Swift executable). Links `libcore.a` + system frameworks |
