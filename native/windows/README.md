# Fire Box — Windows Native Layer

This directory will contain the C++ implementation of:

- **COM Server**: Handle IPC from local apps via COM
- **WinUI GUI**: Real-time monitoring dashboard (token usage, connections, per-app stats)
- **User Approval Popups**: Show authorization prompts when new apps request AI access
- **System Service Registration**: Register as a Windows Service for lifecycle management

## Communication with Rust Core

The native layer communicates with `fire-box-core` via HTTP on a local TCP socket:

- **Commands** → `POST/GET /ipc/v1/*` endpoints
- **Events** ← SSE stream at `GET /ipc/v1/events`

## Build Integration

