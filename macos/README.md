# Fire Box — macOS Native Layer

This directory will contain the Swift implementation of:

- **XPC Service**: Handle IPC from local apps via XPC
- **SwiftUI GUI**: Real-time monitoring dashboard (token usage, connections, per-app stats)
- **User Approval Popups**: Show authorization prompts when new apps request AI access
- **System Service Registration**: Register as a launchd service for lifecycle management

## Communication with Rust Core

The native layer communicates with `fire-box-core` via HTTP on a local TCP socket:

- **Commands** → `POST/GET /ipc/v1/*` endpoints
- **Events** ← SSE stream at `GET /ipc/v1/events`

## Build Integration

Built via `build.rs` in the workspace root, invoked by `cargo build`.
Swift sources are compiled with `swiftc` and linked into the final binary.
