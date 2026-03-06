# Control Protocol

This document provides a comprehensive specification of the Inter-Process Communication (IPC) protocol interface designed to enable the FireBox frontend application to configure, monitor, and administer the backend service. It is crucial to distinguish this management protocol from the Service Protocol, which serves a fundamentally different purpose by enabling client applications to consume artificial intelligence capabilities. The management protocol, in contrast, focuses exclusively on administrative and configuration functions.

## Service Lifecycle & Native Integration

FireBox is designed as a background service that operates independently of the frontend graphical interface. The specific mechanism for managing service lifecycle is implementation-defined and may vary by platform, but MUST ensure high availability and proper system integration with the underlying operating system.

### Manual Control

The GUI provides a dedicated **Start/Stop Service** button. The specific implementation for controlling the service is platform-dependent and MUST utilize appropriate system-level mechanisms to interact with the operating system's service manager. Service control operations MUST handle privilege escalation transparently when required.

### Model Discovery and Capability Verification

The backend service acts as an aggregator of model metadata and capabilities.

#### 1. Capability Source of Truth (`models.dev`)
The primary source of truth for public cloud model capabilities (e.g., GPT-4, Claude 3) is the **`models.dev/api.json`** registry. 
- **Update Logic:** The backend should periodically download and cache this JSON file.
- **Verification:** When a provider reports a new model ID, its capabilities are mapped against this registry.

#### 2. Provider-specific Model Discovery
For each configured provider, the backend performs dynamic discovery:
- **Cloud Providers (OpenAI, Anthropic, etc.):** Use the provider's `/models` endpoint to fetch the list of available model IDs for the user's API key.
- **Local Providers (llama.cpp, Ollama):** Query the local server's status or scan the local model directory.
- **UI Exposure:** This discovered list serves as the data source for the **Model Visibility Dialog** in the provider's configuration card.

### Decoupled Lifecycle

1.  **Independent Operation:** The backend service remains active regardless of whether the frontend GUI is open.
2.  **Manual Control:** The GUI provides a dedicated **Start/Stop Service** button that allows users to control service state.
3.  **GUI Limitation:** The system UI elements (such as tray menus or notifications) MUST NOT provide service control capabilities. They are restricted to navigation and monitoring functions only.

## Administrative IPC Verification

Access to the Control Protocol is strictly restricted to authorized administrative interfaces. 

### Access Control Rules

1.  **Caller Identity:** The backend service MUST verify the identity of any process attempting to connect to the Control IPC socket.
2.  **frontend-only Policy:** Currently, only applications residing within the `@frontend/**` path (representing the official FireBox GUI and helpers) are permitted to issue administrative commands.
3.  **Elevation:** Connection requests from unauthorized paths are silently dropped or explicitly rejected with a `PERMISSION_DENIED` error.

## Common Type Definitions

The protocol employs a set of common data structures that facilitate consistent communication between the frontend application and the backend service. These type definitions establish a shared vocabulary for representing providers, models, metrics, and authentication credentials.

```proto
enum ProviderType {
  PROVIDER_TYPE_API_KEY = 1;    // OpenAI, Anthropic, Gemini
  PROVIDER_TYPE_OAUTH = 2;      // GitHub Copilot, DashScope
  PROVIDER_TYPE_LOCAL = 3;      // llama.cpp (future)
}

message Result {
  required bool success;
  optional string message;
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

message MetricsSnapshot {
  optional int64 window_start_ms;
  optional int64 window_end_ms;
  optional int64 requests_total;
  optional int64 requests_failed;
  optional int64 prompt_tokens_total;
  optional int64 completion_tokens_total;
  optional double cost_total;
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
```

## Provider Management Operations

### Add API Key Provider

This operation facilitates the integration of API key-based providers into the FireBox service. Such providers, which include prominent services such as OpenAI, Anthropic, and Gemini, authenticate through the presentation of static API keys.

The `name` field serves as the **unique identifier** for the provider. If a provider with the same `name` already exists, its configuration is **overwritten** (upsert semantics). This eliminates the need for a separate "update" operation.

```proto
message AddApiKeyProviderRequest {
  required string name;
  required string provider_type;    // "openai", "anthropic", "gemini"
  optional string api_key;
  optional string base_url;
}

message AddApiKeyProviderResponse {
  required Result result;
  optional string provider_id;
}
```

### Add OAuth Provider

This operation facilitates the integration of OAuth-based providers, such as GitHub Copilot and DashScope, into the FireBox service. Unlike API key-based providers, OAuth providers require a multi-step authentication flow to obtain access tokens. The process comprises two distinct steps: initiating the OAuth flow and completing the authentication.

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

**Step 2: Complete OAuth (Polling)**

The frontend MUST poll this endpoint until it receives a terminal result.

```proto
message CompleteOAuthRequest {
  required string provider_id;
}

message CompleteOAuthResponse {
  required Result result;

  enum OAuthStatus {
    OAUTH_STATUS_PENDING = 1;      // Still waiting for user authorization
    OAUTH_STATUS_SUCCESS = 2;      // Authorization complete
    OAUTH_STATUS_EXPIRED = 3;      // Device code expired
    OAUTH_STATUS_DENIED = 4;       // User rejected the request
    OAUTH_STATUS_NETWORK_ERROR = 5; 
  }
  required OAuthStatus status;
  optional OAuthCredentials credentials; // Present only if status = SUCCESS
}
```

**Implementation Note:** The backend should NOT save the provider until `SUCCESS` is achieved. If `EXPIRED` or `DENIED` is returned, the frontend should clean up any temporary state and notify the user.

### Add Local Provider (Future)

> **Note:** Local provider support (llama.cpp) is a long-term goal. This API is reserved for future use and is not currently exposed in the frontend.

This operation enables the integration of local model providers that execute entirely on the user's system without requiring external network connectivity. The llama.cpp provider exemplifies this category, operating by managing a local server process that serves models from the local filesystem.

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

### List Providers

This operation retrieves a comprehensive enumeration of all providers currently configured within the FireBox service, regardless of their operational status. The returned information includes provider identifiers, names, types, and enablement status.

```proto
message ListProvidersRequest {
}

message ListProvidersResponse {
  required Result result;
  repeated Provider providers;
}
```

### Delete Provider

This operation facilitates the removal of a provider from the service configuration. Upon removal, all associated models and routing rules referencing the provider are also eliminated, ensuring consistency within the system configuration.

```proto
message DeleteProviderRequest {
  required string provider_id;
}

message DeleteProviderResponse {
  required Result result;
}
```

## Model Configuration Operations

### Get All Models (Admin)

This operation retrieves a comprehensive enumeration of all models available across all configured providers, irrespective of any routing rules that may be in place. This administrative view enables complete visibility into the model inventory.

```proto
message GetAllModelsRequest {
  optional string provider_id;
}

message GetAllModelsResponse {
  required Result result;
  repeated Model models;
}
```

### Set Model Enabled

This operation enables administrators to selectively activate or deactivate individual models, providing granular control over which models are available for client applications to utilize. Disabling a model prevents it from being selected during request routing whilst preserving its configuration.

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

## Routing Configuration Operations

### Set Route Rules

This operation defines a virtual model and its required capabilities (a Capability Contract). The routing system ensures that any physical provider/model assigned to this rule fulfills these requirements. This abstraction layer enables client applications to reference models by stable identifiers with guaranteed capabilities, whilst the underlying provider configuration may change dynamically as long as the targets fulfill the contract.

**Deletion:** If `targets` is empty, the route rule for the specified `virtual_model_id` is **deleted**.

> **Note:** The validation process relies on the `models.dev` service as the primary source of truth for the capabilities of public cloud models (e.g., from OpenAI, Anthropic). For local models, capabilities are derived from the model file metadata.

```proto
message RouteTarget {
  required string provider_id;
  required string model_id;
}

enum RouteStrategy {
  ROUTE_STRATEGY_FAILOVER = 1; // Try targets in order
  ROUTE_STRATEGY_RANDOM = 2;   // Randomly select target
}

message ModelCapabilities {
  optional bool chat = 1 [default = true];
  optional bool streaming = 2 [default = true];
  optional bool embeddings = 3 [default = false];
  optional bool vision = 4 [default = false];
  optional bool tool_calling = 5 [default = false];
}

message RouteMetadata {
  optional int32 context_window = 1;      // Required minimum context window
  optional string pricing_tier = 2;       // Pricing category (e.g., "free", "low", "high")
  repeated string strengths = 3;          // Required strengths (e.g., "coding", "reasoning")
  optional string description = 4;        // Human-readable description of this virtual model's persona/specialty
}

message SetRouteRulesRequest {
  required string virtual_model_id;       // The virtual model ID exposed to clients (e.g., "coding-assistant")
  required string display_name;           // Human-readable name (e.g., "Enterprise Coding Assistant")
  required ModelCapabilities capabilities; // Capability contract targets must fulfill
  optional RouteMetadata metadata;        // Additional metadata and constraints
  
  repeated RouteTarget targets;           // Physical models that fulfill these requirements
  optional RouteStrategy strategy;        // Defaults to FAILOVER
}

message SetRouteRulesResponse {
  required Result result;
}
```

### Get Route Rules

This operation retrieves the routing configuration and capability contract for a specified virtual model identifier.

```proto
message GetRouteRulesRequest {
  required string virtual_model_id;
}

message GetRouteRulesResponse {
  required Result result;
  optional string display_name;
  optional ModelCapabilities capabilities;
  optional RouteMetadata metadata;
  repeated RouteTarget targets;
  optional RouteStrategy strategy;
}
```

### List Route Rules

This operation retrieves all configured routing rules, enabling the frontend to display a complete overview of the routing configuration.

```proto
message ListRouteRulesRequest {
}

message RouteRuleSummary {
  required string virtual_model_id;
  required string display_name;
  optional ModelCapabilities capabilities;
  optional RouteMetadata metadata;
  repeated RouteTarget targets;
  optional RouteStrategy strategy;
}

message ListRouteRulesResponse {
  required Result result;
  repeated RouteRuleSummary rules;
}
```

## Metrics and Monitoring Operations

### Get Metrics Snapshot

This operation retrieves the current aggregated performance and usage metrics, providing a real-time view of system activity. The metrics encompass request volumes, token consumption statistics, cost calculations, and error rates.

```proto
message GetMetricsSnapshotRequest {
}

message GetMetricsSnapshotResponse {
  required Result result;
  optional MetricsSnapshot snapshot;
}
```

### Get Metrics Range

This operation retrieves historical metrics data for a specified time range, enabling administrators to analyze trends and patterns in system usage over time. The returned data comprises a series of metric snapshots covering the requested temporal interval.

**Granularity:** Each returned snapshot covers a **one-hour** window. The backend aggregates raw metrics into hourly buckets. For a single-day query, this yields up to 24 snapshots.

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

## Connection Management

### List Connections

```proto
message Connection {
  required string connection_id;
  required string client_name;      // e.g. "VS Code", "curl"
  optional int32 pid;               // Process ID of the calling process
  optional int64 connected_at_ms;   // Epoch timestamp when the connection was established
  optional int64 requests_count;
}

message ListConnectionsRequest {
}

message ListConnectionsResponse {
  required Result result;
  repeated Connection connections;  // Only active connections are returned
}
```

## Access Control (Revocation)

### Get Allowlist

Retrieve the list of applications currently authorized to use the service.

```proto
message AllowedApp {
  required string app_path;
  required string display_name;
  required int64 first_seen_ms;
  required int64 last_used_ms;
}

message GetAllowlistRequest {
}

message GetAllowlistResponse {
  required Result result;
  repeated AllowedApp apps;
}
```

### Remove From Allowlist

This operation revokes access privileges for a specified application by removing its entry from the persistent allowlist. Following this revocation, should the application subsequently attempt to establish a connection, the system will initiate the user approval workflow, thereby requiring explicit re-authorization.

```proto
message RemoveFromAllowlistRequest {
  required string app_path;
}

message RemoveFromAllowlistResponse {
  required Result result;
}
```

