```markdown
# Fire Box — Linux Native Layer

This directory will contain the C implementation of:

- **D-Bus Service**: Handle IPC from local apps via D-Bus / GIO
- **GTK4 GUI**: Real-time monitoring dashboard (token usage, connections, per-app stats)
- **User Approval Popups**: Show authorization prompts when new apps request AI access
- **System Service Registration**: Provide a systemd unit for lifecycle management

## Communication with Rust Core

The native layer communicates with `fire-box-core` via HTTP on a Unix domain socket:

- **Commands** → `POST/GET /ipc/v1/*` endpoints
- **Events** ← SSE stream at `GET /ipc/v1/events`

## Build Integration

Bootstrap build via Meson:

```bash
meson setup build
meson compile -C build
```