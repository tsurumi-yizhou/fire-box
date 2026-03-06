# Error Handling

This document specifies the error model of the FireBox Client SDK, covering error types, the `Result` structure, and recommended practices for callers.

## Result Structure

Every protocol response includes a `Result` field:

```proto
message Result {
  required bool success;
  optional string message;
}
```

The SDK translates this into structured error types. When `success = false`, the SDK surfaces an error with the corresponding error code and human-readable message.

## Error Types

### Connection Errors

These errors occur during connection establishment (see `@client/connection.md`).

| Error | Description | Recovery |
|---|---|---|
| `ServiceNotFound` | FireBox backend is not running or the IPC endpoint was not found | Ensure the FireBox service is started |
| `ConnectionDenied` | The user denied authorization for the application | Inform the user; do not retry immediately |
| `ConnectionTimeout` | The connection timed out (e.g., user did not respond to authorization request) | Retry with a longer timeout or prompt the user |
| `TransportError` | Low-level IPC failure (pipe broken, socket error) | Retry connection |

### Operational Errors

These errors occur on API calls after a successful connection.

| Error | Description | Recovery |
|---|---|---|
| `InvalidRequest` | Malformed request (e.g., missing required fields, invalid model_id) | Fix the request parameters |
| `ModelNotFound` | The specified `model_id` does not exist | Re-query available models with `listModels()` |
| `UnsupportedCapability` | The model does not support the requested operation (e.g., embeddings on a chat-only model) | Check `model.capabilities` before calling |
| `StreamBusy` | Attempted to send a message while a previous response is still streaming | Wait for `done = true` before sending the next message |
| `StreamClosed` | Attempted to use a closed `ChatStream` | Create a new stream |
| `BackendError` | An internal error occurred in the FireBox backend or upstream provider | Retry after a brief delay |
| `RateLimited` | The upstream provider returned a rate limit error | Retry after the delay indicated in the error message |

### Client State Errors

| Error | Description | Recovery |
|---|---|---|
| `ClientClosed` | Attempted to use a `FireBoxClient` after `close()` was called | Create a new client |
| `Disconnected` | The IPC connection was unexpectedly lost | Create a new client and reconnect |

## Error Inspection

All SDK errors expose a consistent interface:

```
error.code       // Error type identifier (e.g., "ServiceNotFound")
error.message    // Human-readable description from the backend
error.retryable  // bool: whether the caller should retry
```

### Example

```
try:
    response = client.complete(model_id = "nonexistent", messages = [...])
catch error:
    if error.code == "ModelNotFound":
        models = client.listModels()
        // Pick a valid model and retry
    elif error.retryable:
        sleep(1000)
        // Retry the request
    else:
        // Terminal error, inform the user
        report(error.message)
```

## Retry Guidance

### Retryable Errors

The following errors are safe to retry:

- `TransportError` — Transient IPC issues.
- `BackendError` — Transient backend or upstream failures.
- `RateLimited` — The request was valid but throttled.
- `ConnectionTimeout` — The user may not have responded to the authorization request yet.

### Non-Retryable Errors

The following errors indicate a permanent issue that retrying will not resolve:

- `ServiceNotFound` — The service is not running.
- `ConnectionDenied` — The user explicitly denied access.
- `InvalidRequest` — The request is malformed.
- `ModelNotFound` — The model does not exist.
- `UnsupportedCapability` — The model lacks the requested feature.
- `ClientClosed` / `StreamClosed` — Client-side lifecycle error.

### Recommended Retry Strategy

For retryable errors, callers SHOULD implement exponential backoff with jitter:

```
max_retries = 3
base_delay_ms = 500

for attempt in range(max_retries):
    try:
        response = client.complete(...)
        break
    catch error:
        if not error.retryable or attempt == max_retries - 1:
            raise error
        delay = base_delay_ms * (2 ** attempt) + random(0, base_delay_ms)
        sleep(delay)
```

For `RateLimited` errors, the `error.message` may contain a suggested retry delay. Callers SHOULD respect this value when available.

## Best Practices

1. **Check capabilities before calling.** Query `getModelMetadata()` to verify the model supports the intended operation. This avoids `UnsupportedCapability` errors at runtime.

2. **Handle `Disconnected` gracefully.** IPC connections can be lost if the FireBox service restarts. Design callers to detect disconnection and re-establish the connection.

3. **Do not retry `ConnectionDenied`.** The user made an explicit choice. Repeated connection attempts after denial may result in temporary blocking.

4. **Close resources explicitly.** Always call `stream.close()` and `client.close()` when done. This releases server-side resources promptly rather than waiting for garbage collection or process exit.

5. **Validate tool call arguments.** When the model returns tool calls, validate `arguments_json` before executing. Malformed JSON or unexpected parameter values should be handled defensively by the caller.
