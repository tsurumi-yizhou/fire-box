namespace App.Services;

// ---------------------------------------------------------------------------
// Provider DTOs
// ---------------------------------------------------------------------------

public enum ProviderType { ApiKey = 1, OAuth = 2, Local = 3 }

public sealed record ProviderDto(
    string ProviderId,
    string Name,
    ProviderType Type,
    string BaseUrl);

// ---------------------------------------------------------------------------
// Model DTOs
// ---------------------------------------------------------------------------

public sealed record ModelDto(
    string ModelId,
    string ProviderId,
    string Owner,
    bool Enabled,
    int? ContextWindow);

// ---------------------------------------------------------------------------
// Route DTOs
// ---------------------------------------------------------------------------

public sealed record RouteTargetDto(string ProviderId, string ModelId);

public sealed record RouteCapabilitiesDto(
    bool Chat = true,
    bool Streaming = true,
    bool Embeddings = false,
    bool Vision = false,
    bool ToolCalling = false);

public sealed record RouteRuleDto(
    string VirtualModelId,
    string DisplayName,
    string Strategy,
    RouteTargetDto[] Targets,
    RouteCapabilitiesDto Capabilities);

// ---------------------------------------------------------------------------
// Metrics DTOs
// ---------------------------------------------------------------------------

public sealed record MetricsSnapshotDto(
    long WindowStartMs,
    long WindowEndMs,
    long RequestsTotal,
    long RequestsFailed,
    long PromptTokensTotal,
    long CompletionTokensTotal,
    long LatencyAvgMs,
    double CostTotal);

// ---------------------------------------------------------------------------
// Connection DTOs
// ---------------------------------------------------------------------------

public sealed record ConnectionDto(
    string ConnectionId,
    string ClientName,
    string AppPath,
    long RequestsCount,
    long ConnectedAtMs);

// ---------------------------------------------------------------------------
// Allowlist DTOs
// ---------------------------------------------------------------------------

public sealed record AllowlistEntryDto(string AppPath, string DisplayName);

// ---------------------------------------------------------------------------
// OAuth DTOs
// ---------------------------------------------------------------------------

public sealed record OAuthChallengeDto(
    string DeviceCode,
    string UserCode,
    string VerificationUri,
    long ExpiresIn,
    long Interval);
