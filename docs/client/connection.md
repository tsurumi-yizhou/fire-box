# Connection

This document specifies the connection lifecycle of the FireBox Client SDK, covering service discovery, connection establishment, authorization, and disconnection. The SDK handles all platform-specific transport details internally, exposing a uniform interface to the caller.

## Service Discovery

Before establishing a connection, the SDK must locate the FireBox backend service endpoint. The discovery mechanism is platform-dependent but transparent to the caller. The SDK MUST protect these platform-specific details internally and present a uniform interface across all platforms.

## Connection Establishment

```
client = FireBoxClient.connect()          // Auto-discover
client = FireBoxClient.connect(address)   // Explicit endpoint
```

### `FireBoxClient.connect` Behavior

1. **Resolve Endpoint:** Discover the FireBox service endpoint using platform-specific mechanisms (transparent to caller).
2. **Open Transport:** Establish the IPC connection to the resolved endpoint.
3. **Identity Verification:** The FireBox backend identifies the calling process. This step is **transparent** to the SDK — no explicit authentication message is required from the caller.
4. **Authorization:** If the calling application is unknown to FireBox (first use), the backend initiates an authorization flow with the user and waits for a decision. From the SDK's perspective, the `connect()` call is blocked until the user approves or denies the connection.
5. **Return:** On success, return a connected `FireBoxClient` instance. On denial or timeout, return a `ConnectionDenied` error.

### Connection Timeout

The SDK exposes an optional timeout parameter:

```
client = FireBoxClient.connect(timeout_ms = 30000)
```

If authorization is pending (TOFU popup) and the timeout elapses, the SDK closes the transport and returns a `ConnectionTimeout` error. The default timeout is implementation-defined but SHOULD be at least 60 seconds to allow sufficient time for user interaction.

## Authorization Status

After a successful connection, the caller may optionally query its authorization status. This is not required for normal operation but can be useful for diagnostics.

```
status = client.getAuthStatus()
// status.authorized  -> bool
// status.app_name    -> string (resolved display name)
```

### Protocol Mapping

```proto
// Request: implicit (uses connection identity)
// Response:
message AuthStatusResponse {
  required Result result;
  required bool authorized;
  optional string app_name;
}
```

## Connection Lifecycle

### State Diagram

```
  connect()
     │
     ▼
 ┌────────┐   deny/timeout   ┌────────┐
 │Connecting├────────────────►│ Failed │
 └────┬───┘                  └────────┘
      │ authorized
      ▼
 ┌────────┐    close()       ┌────────┐
 │Connected├────────────────►│ Closed │
 └────┬───┘                  └────────┘
      │ transport error
      ▼
 ┌────────────┐
 │Disconnected│
 └────────────┘
```

### States

- **Connecting:** The IPC transport is open but authorization may be pending. All API calls except `close()` will block or return an error.
- **Connected:** The connection is authorized and operational. All API calls are available.
- **Failed:** The connection could not be established (service not found, denied, or timed out). The `FireBoxClient` instance is not usable.
- **Disconnected:** The transport was unexpectedly closed by the backend or due to a network-level error. The SDK raises a `Disconnected` error on subsequent API calls. The caller should create a new `FireBoxClient` to reconnect.
- **Closed:** The caller explicitly called `close()`. The `FireBoxClient` instance is no longer usable.

### Reconnection

The SDK does **not** implement automatic reconnection. If the connection is lost, the caller is responsible for creating a new `FireBoxClient` instance. This design avoids hidden retry loops and gives the caller full control over reconnection policy.

## Disconnection

```
client.close()
```

Calling `close()` performs the following:
1. Closes any active `ChatStream` sessions associated with this client.
2. Sends a graceful shutdown signal to the backend (if the transport is still open).
3. Releases all transport resources.

Calling `close()` on an already-closed client is a no-op. After `close()`, all subsequent API calls on this client instance return a `ClientClosed` error.


