# Connections

This page provides real-time monitoring of all active IPC connections to the service.

## Active Connection List

A table displaying each connected client process:

- **Client Name:** (e.g., "VS Code", "curl", "Unknown")
- **Process ID (PID):** The unique identifier for the calling process.
- **Connection Duration:** Calculated from `connected_at_ms` to the current time.
- **Requests Count:** Total number of requests made during the current session.

Only active connections are displayed. Terminated connections are excluded from the list.

## Actions

- **Refresh List:** Manually re-fetch the list of active connections.

## Data Source
Backend's `ListConnections` API.
