# Local IPC AI Capability Protocol

IPC protocol between GUI app and backend service.
Uses native IPC per platform (COM / XPC / D-Bus).

## Common Types

```proto
enum ProviderType {
  PROVIDER_TYPE_API_KEY = 1;    // OpenAI, Anthropic, Ollama, vLLM
  PROVIDER_TYPE_OAUTH = 2;      // GitHub Copilot, DashScope
  PROVIDER_TYPE_LOCAL = 3;      // llama.cpp
}

enum MessageRole {
  MESSAGE_ROLE_SYSTEM = 1;
  MESSAGE_ROLE_USER = 2;
  MESSAGE_ROLE_ASSISTANT = 3;
  MESSAGE_ROLE_TOOL = 4;
}

message Result {
  required bool success;
  optional string message;
}

message OAuthChallenge {
  required string device_code;
  required string user_code;
  required string verification_uri;
  required int32 expires_in;
  required int32 interval;
}

message OAuthCredentials {
  required string access_token;
  optional string refresh_token;
  required int64 expiry_date_ms;
  optional string resource_url;
}

message Provider {
  required string provider_id;
  required string name;
  required ProviderType type;
  optional string base_url;
  optional string local_path;
  required bool enabled;
}

message Model {
  required string model_id;
  required string provider_id;
  required bool enabled;
  optional bool capability_chat;
  optional bool capability_streaming;
}

message Message {
  required MessageRole role;
  optional string content;
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

message ToolCall {
  required string id;
  required string name;
  required string arguments_json;
}

message MetricsSnapshot {
  optional int64 window_start_ms;
  optional int64 window_end_ms;
  optional int64 requests_total;
  optional int64 requests_failed;
  optional int64 prompt_tokens_total;
  optional int64 completion_tokens_total;
  optional double cost_total;
}
```

---

## Provider Management

### Add API Key Provider

For OpenAI, Anthropic, Ollama, vLLM, etc.

```proto
message AddApiKeyProviderRequest {
  required string name;
  required string provider_type;    // "openai", "anthropic", "ollama", "vllm"
  optional string api_key;          // Empty for Ollama
  optional string base_url;
}

message AddApiKeyProviderResponse {
  required Result result;
  optional string provider_id;
}
```

**Examples:**

```
// OpenAI
{ name: "My OpenAI", provider_type: "openai", api_key: "sk-..." }

// Ollama (no auth)
{ name: "Local Ollama", provider_type: "ollama", base_url: "http://localhost:11434" }

// vLLM
{ name: "vLLM", provider_type: "vllm", base_url: "http://localhost:8000/v1", api_key: "token-123" }
```

---

### Add OAuth Provider

**Step 1: Start OAuth**

```proto
message AddOAuthProviderRequest {
  required string name;
  required string provider_type;    // "copilot", "dashscope"
}

message AddOAuthProviderResponse {
  required Result result;
  optional string provider_id;
  optional OAuthChallenge challenge;
}
```

**Step 2: Complete OAuth**

```proto
message CompleteOAuthRequest {
  required string provider_id;
}

message CompleteOAuthResponse {
  required Result result;
  optional OAuthCredentials credentials;
}
```

---

### Add Local Provider

```proto
message AddLocalProviderRequest {
  required string name;
  required string model_path;
}

message AddLocalProviderResponse {
  required Result result;
  optional string provider_id;
}
```

---

### List Providers

```proto
message ListProvidersRequest {
}

message ListProvidersResponse {
  required Result result;
  repeated Provider providers;
}
```

---

### Delete Provider

```proto
message DeleteProviderRequest {
  required string provider_id;
}

message DeleteProviderResponse {
  required Result result;
}
```

---

## Model Management

### Get Models

```proto
message GetModelsRequest {
  optional string provider_id;
}

message GetModelsResponse {
  required Result result;
  repeated Model models;
}
```

---

### Set Model Enabled

```proto
message SetModelEnabledRequest {
  required string provider_id;
  required string model_id;
  required bool enabled;
}

message SetModelEnabledResponse {
  required Result result;
}
```

---

## Routing

### Set Route Rules

```proto
message RouteTarget {
  required string provider_id;
  required string model_id;
}

message SetRouteRulesRequest {
  required string alias;
  repeated RouteTarget targets;
}

message SetRouteRulesResponse {
  required Result result;
}
```

---

### Get Route Rules

```proto
message GetRouteRulesRequest {
  required string alias;
}

message GetRouteRulesResponse {
  required Result result;
  repeated RouteTarget targets;
}
```

---

## Chat Completion

### Complete (non-streaming)

```proto
message CompleteRequest {
  required string model_id;
  repeated Message messages;
  repeated Tool tools;
}

message CompleteResponse {
  required Result result;
  optional Message completion;
  optional Usage usage;
}
```

---

### Stream (streaming)

**Create stream:**

```proto
message CreateStreamRequest {
  required string model_id;
}

message CreateStreamResponse {
  required Result result;
  optional string stream_id;
}
```

**Send message:**

```proto
message SendMessageRequest {
  required string stream_id;
  required Message message;
  repeated Tool tools;
}

message SendMessageResponse {
  required Result result;
}
```

**Receive chunk:**

```proto
message ReceiveStreamRequest {
  required string stream_id;
  optional int32 timeout_ms;
}

message ReceiveStreamResponse {
  required Result result;
  optional Message chunk;
  optional ToolCall tool_call;
  optional bool done;
  optional Usage usage;
}
```

**Close stream:**

```proto
message CloseStreamRequest {
  required string stream_id;
}

message CloseStreamResponse {
  required Result result;
}
```

---

## Metrics

### Get Metrics Snapshot

```proto
message GetMetricsSnapshotRequest {
}

message GetMetricsSnapshotResponse {
  required Result result;
  optional MetricsSnapshot snapshot;
}
```

---

### Get Metrics Range

```proto
message GetMetricsRangeRequest {
  required int64 start_ms;
  required int64 end_ms;
}

message GetMetricsRangeResponse {
  required Result result;
  repeated MetricsSnapshot snapshots;
}
```

---

## Connection Management

### List Connections

```proto
message Connection {
  required string connection_id;
  required string client_name;      // e.g. "VS Code", "curl"
  optional int64 requests_count;
}

message ListConnectionsRequest {
}

message ListConnectionsResponse {
  required Result result;
  repeated Connection connections;
}
```

### Close Connection

```proto
message CloseConnectionRequest {
  required string connection_id;
}

message CloseConnectionResponse {
  required Result result;
}
```

---

## Usage Examples

### Add OpenAI Provider

```
Request: AddApiKeyProvider {
  name: "My OpenAI",
  provider_type: "openai",
  api_key: "sk-abc123"
}

Response: {
  success: true,
  provider_id: "prov_abc123"
}
```

### Add Ollama Provider (no auth)

```
Request: AddApiKeyProvider {
  name: "Local Ollama",
  provider_type: "ollama",
  base_url: "http://localhost:11434"
}

Response: {
  success: true,
  provider_id: "prov_ollama123"
}
```

### Add Copilot (OAuth)

```
// Step 1: Start OAuth
Request: AddOAuthProvider {
  name: "Copilot",
  provider_type: "copilot"
}

Response: {
  success: true,
  provider_id: "prov_copilot123",
  challenge: {
    device_code: "abc...",
    user_code: "ABCD-1234",
    verification_uri: "https://github.com/login/device",
    expires_in: 900,
    interval: 1
  }
}

// User visits URL and enters code...

// Step 2: Complete OAuth
Request: CompleteOAuth {
  provider_id: "prov_copilot123"
}

Response: {
  success: true,
  credentials: {
    access_token: "...",
    refresh_token: "...",
    expiry_date_ms: 1234567890
  }
}
```

### Streaming Chat

```
// 1. Create stream
Request: CreateStream { model_id: "my-model" }
Response: { success: true, stream_id: "stream_123" }

// 2. Send message
Request: SendMessage {
  stream_id: "stream_123",
  message: { role: "user", content: "Hello" }
}
Response: { success: true }

// 3. Receive streaming chunks (loop until done=true)
Request: ReceiveStream { stream_id: "stream_123", timeout_ms: 5000 }
Response: { success: true, chunk: { role: "assistant", content: "Hel" } }

Request: ReceiveStream { stream_id: "stream_123", timeout_ms: 5000 }
Response: { success: true, chunk: { role: "assistant", content: "lo!" } }

Request: ReceiveStream { stream_id: "stream_123", timeout_ms: 5000 }
Response: { success: true, done: true, usage: { total_tokens: 10 } }

// 4. Close stream
Request: CloseStream { stream_id: "stream_123" }
Response: { success: true }
```
