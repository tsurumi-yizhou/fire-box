# Chat Completions

This document specifies the chat completion interface of the FireBox Client SDK, covering model discovery, non-streaming (synchronous) completions, streaming sessions, and tool calling. All operations require a connected `FireBoxClient` instance (see `@client/connection.md`).

## Model Discovery

Before sending completion requests, callers should discover which models are available and what capabilities they support.

### List Available Models

```
models = client.listModels()
// Returns: list of ServiceModel
```

Each `ServiceModel` includes:

| Field | Type | Description |
|---|---|---|
| `id` | string | The virtual model ID to use in requests |
| `display_name` | string | Human-readable name |
| `capabilities` | ModelCapabilities | Supported operations (chat, streaming, vision, tool_calling, embeddings) |
| `metadata` | ModelMetadata | Context window size, pricing tier, strengths, description |

### Get Model Metadata

```
model = client.getModelMetadata(model_id = "coding-assistant")
```

Returns detailed information about a specific model. Useful for checking constraints (e.g., context window size) before constructing a request.

### Capability Check

Callers SHOULD verify model capabilities before making requests. Sending an unsupported request (e.g., tool calls to a model without `tool_calling = true`) results in an `UnsupportedCapability` error.

```
model = client.getModelMetadata(model_id = "coding-assistant")
if not model.capabilities.tool_calling:
    // Do not send tools with this model
```

## Non-Streaming Completion

The synchronous completion API sends a request and returns the full response in a single call. This is suitable for simple interactions where latency is acceptable.

### API

```
response = client.complete(
    model_id    = "coding-assistant",
    messages    = [...],
    tools       = [...],         // Optional
    temperature = 0.7,           // Optional
    max_tokens  = 1024           // Optional
)
```

### Parameters

| Parameter | Type | Required | Description |
|---|---|---|---|
| `model_id` | string | Yes | The virtual model ID |
| `messages` | list of Message | Yes | Conversation history |
| `tools` | list of Tool | No | Available tools for the model to invoke |
| `temperature` | double | No | Sampling temperature (0.0 - 2.0) |
| `max_tokens` | int | No | Maximum tokens in the response |

### Response

| Field | Type | Description |
|---|---|---|
| `completion` | Message | The assistant's response message |
| `usage` | Usage | Token consumption statistics |
| `finish_reason` | string | Why generation stopped: `"stop"`, `"length"`, `"tool_calls"` |

### Message Structure

```
Message(
    role         = MessageRole,   // SYSTEM, USER, ASSISTANT, TOOL
    content      = string,        // Text content (optional for tool_calls)
    tool_calls   = [...],         // Present when role=ASSISTANT and model invokes tools
    tool_call_id = string         // Required when role=TOOL
)
```

### Basic Example

```
response = client.complete(
    model_id = "general",
    messages = [
        Message(role = SYSTEM, content = "You are a helpful assistant."),
        Message(role = USER, content = "What is 2+2?")
    ]
)

print(response.completion.content)  // "4"
print(response.usage.total_tokens)  // e.g., 28
```

## Streaming Completion

Streaming enables incremental delivery of the model's response, providing real-time feedback for interactive applications. The streaming API uses a session-based model with explicit lifecycle management.

### Lifecycle

```
1. createStream()    →  ChatStream handle
2. stream.send()     →  Send a user message (or tool result)
3. stream.receive()  →  Poll for response chunks (repeat until done)
4. stream.close()    →  Release the session
```

### Create Stream

```
stream = client.createStream(
    model_id    = "coding-assistant",
    temperature = 0.7,           // Optional
    max_tokens  = 2048           // Optional
)
```

Returns a `ChatStream` handle bound to a server-side session. The stream is reusable for multi-turn conversations within the same session.

### Send Message

```
stream.send(
    message = Message(role = USER, content = "Explain quicksort."),
    tools   = [...]  // Optional: update available tools
)
```

Sends a user message or tool result into the stream. The `tools` parameter allows updating the tool set with each message, accommodating dynamic tool configurations.

Calling `send()` while a previous response is still being received (i.e., before `done = true`) results in a `StreamBusy` error.

### Receive Chunks

```
while chunk = stream.receive(timeout_ms = 5000):
    if chunk.content:
        print(chunk.content, end = "")
    if chunk.tool_call_deltas:
        // Accumulate tool call fragments
        accumulate(chunk.tool_call_deltas)
    if chunk.done:
        print("\n[Done]", chunk.finish_reason)
        break
```

Each chunk contains:

| Field | Type | Description |
|---|---|---|
| `content` | string | Incremental text delta (may be empty) |
| `tool_call_deltas` | list of ToolCallDelta | Incremental tool call fragments |
| `done` | bool | `true` when the response is complete |
| `usage` | Usage | Present only when `done = true` |
| `finish_reason` | string | Present only when `done = true` |

### Tool Call Deltas

When a model streams tool calls, the arguments arrive incrementally as JSON string fragments:

```
// First chunk:   ToolCallDelta(index=0, id="call_1", name="get_weather", arguments_delta="{\"ci")
// Second chunk:  ToolCallDelta(index=0, arguments_delta="ty\":\"To")
// Third chunk:   ToolCallDelta(index=0, arguments_delta="kyo\"}")
```

The caller is responsible for concatenating `arguments_delta` values by `index` to reconstruct the complete `arguments_json`.

### Multi-Turn Streaming

A single `ChatStream` supports multi-turn conversation:

```
stream = client.createStream(model_id = "general")

// Turn 1
stream.send(Message(role = USER, content = "Hello"))
while chunk = stream.receive():
    print(chunk.content, end = "")
    if chunk.done: break

// Turn 2
stream.send(Message(role = USER, content = "Tell me more"))
while chunk = stream.receive():
    print(chunk.content, end = "")
    if chunk.done: break

stream.close()
```

### Close Stream

```
stream.close()
```

Releases the server-side session and all associated resources. Calling `close()` on an already-closed stream is a no-op. After `close()`, all operations on the stream return a `StreamClosed` error.

Streams are also closed automatically when the parent `FireBoxClient` is closed.

## Tool Calling

Tool calling enables the model to invoke external functions defined by the caller. The flow applies to both streaming and non-streaming modes.

### Tool Definition

```
Tool(
    name        = "get_weather",
    description = "Get current weather for a given city",
    json_schema = '{"type":"object","properties":{"city":{"type":"string"}},"required":["city"]}'
)
```

The `json_schema` field is a JSON string describing the tool's input parameters in JSON Schema format.

### Non-Streaming Tool Flow

```
// 1. Send request with tools
response = client.complete(model_id, messages, tools)

// 2. Check if model wants to call a tool
while response.finish_reason == "tool_calls":
    tool_results = []
    for tc in response.completion.tool_calls:
        result = execute_locally(tc.name, tc.arguments_json)
        tool_results.append(
            Message(role = TOOL, content = result, tool_call_id = tc.id)
        )

    // 3. Send tool results back (append to conversation)
    messages.append(response.completion)  // assistant message with tool_calls
    messages.extend(tool_results)
    response = client.complete(model_id, messages, tools)

// 4. Final text response
print(response.completion.content)
```

### Streaming Tool Flow

```
stream = client.createStream(model_id)
stream.send(Message(role = USER, content = "What's the weather?"), tools = tools)

accumulated_calls = {}
while chunk = stream.receive():
    for delta in chunk.tool_call_deltas:
        if delta.id:
            accumulated_calls[delta.index] = {"id": delta.id, "name": delta.name, "args": ""}
        accumulated_calls[delta.index]["args"] += delta.arguments_delta
    if chunk.done:
        break

if chunk.finish_reason == "tool_calls":
    for call in accumulated_calls.values():
        result = execute_locally(call["name"], call["args"])
        stream.send(Message(role = TOOL, content = result, tool_call_id = call["id"]))

    // Receive final response
    while chunk = stream.receive():
        print(chunk.content, end = "")
        if chunk.done: break

stream.close()
```

## Protocol Mapping Reference

| SDK Method | Request Message | Response Message |
|---|---|---|
| `client.listModels()` | `ListAvailableModelsRequest` | `ListAvailableModelsResponse` |
| `client.getModelMetadata(id)` | `GetModelMetadataRequest` | `GetModelMetadataResponse` |
| `client.complete(...)` | `CompleteRequest` | `CompleteResponse` |
| `client.createStream(...)` | `CreateStreamRequest` | `CreateStreamResponse` |
| `stream.send(...)` | `SendMessageRequest` | `SendMessageResponse` |
| `stream.receive(...)` | `ReceiveStreamRequest` | `ReceiveStreamResponse` |
| `stream.close()` | `CloseStreamRequest` | `CloseStreamResponse` |
