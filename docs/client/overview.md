# Client SDK Overview

This document describes the design and usage of the FireBox Client SDK, a platform-agnostic library that enables third-party applications to consume AI capabilities provided by the FireBox service. The SDK abstracts the underlying Capability Protocol (defined in `@backend/capability.md`) into an ergonomic, high-level programming interface.

## Design Principles

1. **Platform-Agnostic API Surface:** The SDK exposes a uniform API regardless of the host operating system. All platform-specific details (IPC transport, process identity verification) are encapsulated within an internal transport layer.
2. **Minimal Dependencies:** The SDK should be implementable with only standard library facilities and a protobuf serialization layer. No external HTTP or networking libraries are required.
3. **Streaming-First:** Streaming is the primary interaction mode. Non-streaming (synchronous) completion is provided as a convenience wrapper.
4. **Explicit Error Handling:** All operations return structured results. The SDK never throws unstructured exceptions for protocol-level errors.

## Architecture

```
┌─────────────────────────────────────┐
│         Third-Party Application     │
├─────────────────────────────────────┤
│          FireBox Client SDK         │
│  ┌───────────┐  ┌────────────────┐  │
│  │ High-Level│  │   Transport    │  │
│  │    API    │──│    Layer       │  │
│  └───────────┘  └────────────────┘  │
├─────────────────────────────────────┤
│      OS IPC (platform-specific)     │
└─────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────┐
│        FireBox Backend Service      │
└─────────────────────────────────────┘
```

### Transport Layer

The transport layer is responsible for:
- **Service Discovery:** Locating the FireBox backend IPC endpoint on the current platform.
- **Connection Management:** Establishing, maintaining, and closing IPC connections.
- **Serialization:** Encoding and decoding protobuf messages over the IPC channel.

Third-party SDK implementors may substitute the transport layer for their platform while retaining the high-level API contract. See `@client/connection.md` for details.

### High-Level API

The high-level API provides the primary developer-facing interface:

- **`FireBoxClient`** — The main entry point. Manages connection lifecycle and exposes all operations.
- **`ChatStream`** — A stateful streaming session handle returned by streaming operations.

## Quick Start

The following pseudocode demonstrates the minimal integration path:

```
// 1. Create client (auto-discovers the FireBox service)
client = FireBoxClient.connect()

// 2. Discover available models
models = client.listModels()
model_id = models[0].id

// 3. Send a chat completion request
response = client.complete(
    model_id = model_id,
    messages = [
        Message(role = USER, content = "Hello, world!")
    ]
)

print(response.completion.content)

// 4. Disconnect when done
client.close()
```

### Streaming Example

```
client = FireBoxClient.connect()

stream = client.createStream(model_id = "coding-assistant")

stream.send(Message(role = USER, content = "Explain quicksort."))

while chunk = stream.receive():
    if chunk.done:
        break
    print(chunk.content, end = "")

stream.close()
client.close()
```

### Tool Calling Example

```
client = FireBoxClient.connect()

tools = [
    Tool(
        name = "get_weather",
        description = "Get current weather for a city",
        json_schema = '{"type":"object","properties":{"city":{"type":"string"}},"required":["city"]}'
    )
]

response = client.complete(
    model_id = "tool-capable-model",
    messages = [Message(role = USER, content = "What's the weather in Tokyo?")],
    tools = tools
)

// If the model invoked a tool, handle the tool call
if response.finish_reason == "tool_calls":
    tool_call = response.completion.tool_calls[0]
    tool_result = execute_tool(tool_call.name, tool_call.arguments_json)

    // Send the tool result back
    response = client.complete(
        model_id = "tool-capable-model",
        messages = [
            Message(role = USER, content = "What's the weather in Tokyo?"),
            response.completion,  // The assistant message with tool_calls
            Message(role = TOOL, content = tool_result, tool_call_id = tool_call.id)
        ],
        tools = tools
    )

print(response.completion.content)
client.close()
```

## SDK Modules

| Document | Description |
|---|---|
| `@client/connection.md` | Connection lifecycle, service discovery, and authorization |
| `@client/chat.md` | Chat completion and streaming operations |
| `@client/embeddings.md` | Text embedding operations |
| `@client/errors.md` | Error handling, retry strategies, and best practices |
