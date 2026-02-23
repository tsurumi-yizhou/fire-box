# FireBox Service Protocol

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
  
  // For role=TOOL: The tool output must correspond to a specific call ID
  optional string tool_call_id; 
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

// Minimal model info exposed to callers
message ServiceModel {
  required string id; // The alias or model ID available for use
}
```

## Discovery Mechanisms

### List Available Models

The discovery mechanism enables callers to ascertain which models or model aliases are presently available for utilization within the FireBox service. This capability facilitates dynamic adaptation to the current service configuration without requiring prior knowledge of available models.

```proto
message ListAvailableModelsRequest {
}

message ListAvailableModelsResponse {
  required Result result;
  repeated ServiceModel models;
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
