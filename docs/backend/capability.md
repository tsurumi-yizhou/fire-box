# Capability Protocol

This document presents a comprehensive specification of the Inter-Process Communication (IPC) protocol interface through which external applications, hereinafter referred to as callers, may consume artificial intelligence capabilities provided by the FireBox service. The protocol establishes a standardized communication framework that enables diverse client applications to interact with the service in a consistent and predictable manner.

## Common Types

```proto
enum MessageRole {
  MESSAGE_ROLE_SYSTEM = 1;
  MESSAGE_ROLE_USER = 2;
  MESSAGE_ROLE_ASSISTANT = 3;
  MESSAGE_ROLE_TOOL = 4;
}

message ToolCall {
  required string id;
  required string name;
  required string arguments_json;
}

message Message {
  required MessageRole role;
  optional string content;
  
  // For role=ASSISTANT: The model may return tool calls
  repeated ToolCall tool_calls;
  
  // For streaming: Incremental updates
  repeated ToolCallDelta tool_call_deltas;
  
  // For role=TOOL: The tool output must correspond to a specific call ID
  optional string tool_call_id; 
}

message ToolCallDelta {
  required int32 index;  // Index in the tool_calls array
  optional string id;    // Present only in the first chunk
  optional string name;  // Present only in the first chunk
  optional string arguments_delta; // Incremental JSON string fragments
}

message Usage {
  optional int32 prompt_tokens;
  optional int32 completion_tokens;
  optional int32 total_tokens;
}

message Tool {
  required string name;
  optional string description;
  required string json_schema;
}

message Result {
  required bool success;
  optional string message;
}

message ModelCapabilities {
  optional bool chat = 1 [default = true];
  optional bool streaming = 2 [default = true];
  optional bool embeddings = 3 [default = false];
  optional bool vision = 4 [default = false];
  optional bool tool_calling = 5 [default = false];
}

message ModelMetadata {
  optional int32 context_window = 1;
  optional string pricing_tier = 2;
  repeated string strengths = 3;
  optional string description = 4;
}

// Model info exposed to callers, reflecting the routing capability contract
message ServiceModel {
  required string id;           // The virtual model ID available for use
  required string display_name; // Human-readable name
  optional ModelCapabilities capabilities;
  optional ModelMetadata metadata;
}

message Embedding {
  repeated double values;
  optional int32 index;
}
```

## Security and Trust (TOFU)

The service implements a **Trust On First Use (TOFU)** security model for all incoming IPC connections. This process is designed to be **completely transparent** to the calling application.

### Identity Verification

To ensure robust and tamper-proof identification of the calling process, the service verifies the identity of the caller using native operating system capabilities. The specific verification mechanism is implementation-defined and may vary by platform, but MUST securely bind the IPC connection to the calling process and prevent identity spoofing.

### Authorization Flow (Client Perspective)

1.  **Identity Verification:** The service identifies the calling process immediately upon connection.
2.  **First-Use Authorization:** If the application is not already in the allowlist:
    *   The service initiates an authorization interaction with the user (see `@frontend/helper.md`).
    *   The user may grant or deny access.
    *   Granted applications are added to the `allowlist`.
3.  **Approved:** The connection is accepted, and the backend continues processing the first request.
4.  **Denied/Unauthorized:** The connection is terminated by the service.
5.  **Blocked (Recent Denial):** Connections from applications that have been recently denied may be immediately rejected, with the specific timeout being implementation-defined.

### Protocol Messages (Optional/Status)

The caller *may* query their authorization status, but it is not mandatory for typical operation.

```proto
message AuthStatusRequest {
  // Implicitly uses the connection's process identity
}

message AuthStatusResponse {
  required Result result;
  required bool authorized;       // Whether the caller is currently authorized
  optional string app_name;       // The resolved display name of the calling application
}
```

## Discovery Mechanisms

### List Available Models

The discovery mechanism enables callers to ascertain which virtual models are presently available for utilization within the FireBox service, along with their guaranteed capabilities. This capability facilitates dynamic adaptation to the current service configuration without requiring prior knowledge of available models.

```proto
message ListAvailableModelsRequest {
}

message ListAvailableModelsResponse {
  required Result result;
  repeated ServiceModel models;
}
```

### Get Model Metadata

This operation retrieves detailed capabilities and metadata for a specific virtual model ID. This is useful for clients that need to understand constraints (like context window size) or capabilities (like vision support) before initiating a request.

```proto
message GetModelMetadataRequest {
  required string model_id;
}

message GetModelMetadataResponse {
  required Result result;
  optional ServiceModel model;
}
```

## Chat Completion Operations

### Complete (Non-streaming)

The non-streaming completion operation implements a synchronous request-response paradigm, wherein the caller submits a complete request and receives the entire response in a single transaction. This approach is particularly suitable for scenarios where latency is acceptable and the caller prefers to receive the complete response before proceeding with subsequent operations.

```proto
message CompleteRequest {
  required string model_id;
  repeated Message messages;
  repeated Tool tools;
  optional double temperature;
  optional int32 max_tokens;
}

message CompleteResponse {
  required Result result;
  optional Message completion;
  optional Usage usage;
  optional string finish_reason; // "stop", "length", "tool_calls", etc.
}
```

### Stream (Streaming)

The streaming operation implements a stateful session-based paradigm that enables incremental delivery of response content. This approach is advantageous in scenarios where immediate feedback is desired or where responses may be lengthy, as it allows callers to begin processing partial results before the complete response has been generated.

**1. Create Stream**

The stream creation operation initializes a new streaming session, establishing the necessary state and configuration parameters for subsequent message exchanges within that session.

```proto
message CreateStreamRequest {
  required string model_id;
  optional double temperature;
  optional int32 max_tokens;
}

message CreateStreamResponse {
  required Result result;
  optional string stream_id;
}
```

**2. Send Message**

The message transmission operation enables callers to submit user messages or tool execution results to an active streaming session. It is noteworthy that tool definitions, which specify the capabilities available to the model during response generation, are typically transmitted either with the initial message or updated incrementally with subsequent messages. This flexibility accommodates both static tool configurations and dynamic scenarios where available tools may change during the course of a conversation.

```proto
message SendMessageRequest {
  required string stream_id;
  required Message message;
  repeated Tool tools; // Optional: Update available tools
}

message SendMessageResponse {
  required Result result;
}
```

**3. Receive Chunk**

The chunk reception operation implements a polling mechanism whereby callers retrieve successive fragments of the streaming response. This iterative process continues until the response is complete, as indicated by the service.

```proto
message ReceiveStreamRequest {
  required string stream_id;
  optional int32 timeout_ms;
}

message ReceiveStreamResponse {
  required Result result;
  
  // Incremental content updates
  optional Message chunk; // Role is usually ASSISTANT. Content is a delta.
  
  // Tool call handling in streams
  // Note: Complex tool calls might be streamed. 
  // For simplicity in this version, tool_calls might arrive fully formed or as deltas.
  // We use the 'chunk' Message structure which has 'tool_calls'. 
  // If 'tool_calls' is present in the chunk, it's a delta or full object depending on implementation.
  
  optional bool done;
  optional Usage usage; // Sent when done=true
  optional string finish_reason; // Sent when done=true
}
```

**4. Close Stream**

```proto
message CloseStreamRequest {
  required string stream_id;
}

message CloseStreamResponse {
  required Result result;
}
```

## Embedding Operations

### Embed

The embedding operation transforms textual input into high-dimensional vector representations, enabling semantic search, similarity calculations, and other vector-based operations. This capability is fundamental to applications requiring semantic understanding of text, such as retrieval-augmented generation systems, document clustering, and semantic search implementations. The operation supports batch processing of multiple input texts, thereby enabling efficient vectorization of document collections.

```proto
message EmbedRequest {
  required string model_id;
  repeated string inputs; // Batch embedding support
  optional string encoding_format; // "float" or "base64"
}

message EmbedResponse {
  required Result result;
  repeated Embedding embeddings;
  optional Usage usage;
}
```
