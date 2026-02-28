using System;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Threading.Tasks;
using System.Diagnostics;

namespace App.Services;

// ---------------------------------------------------------------------------
// COM interface — mirrors the Rust IFireBoxService vtable exactly
// ---------------------------------------------------------------------------

[ComImport]
[Guid("3B1A2C4D-5E6F-7A8B-9C0D-EF1234567890")]
[InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
internal interface IFireBoxServiceCom
{
    [PreserveSig]
    int Invoke(
        [MarshalAs(UnmanagedType.BStr)] string cmd,
        [MarshalAs(UnmanagedType.BStr)] string payload,
        [MarshalAs(UnmanagedType.BStr)] out string result);
}

// ---------------------------------------------------------------------------
// FireBoxComService — thin async wrapper used by ViewModels
// ---------------------------------------------------------------------------

public sealed class FireBoxComService : IDisposable
{
    private static readonly Guid Clsid = new("4C2B3D5E-6F7A-8B9C-0D1E-F12345678901");

    private IFireBoxServiceCom? _com;
    private bool _disposed;

    // Singleton lazy-init per process
    private static FireBoxComService? _instance;
    public static FireBoxComService Default => _instance ??= new FireBoxComService();

    private FireBoxComService() { }

    // -----------------------------------------------------------------------
    // Low-level invoke: runs COM call on a thread-pool thread
    // -----------------------------------------------------------------------

    public async Task<JsonNode> InvokeAsync(string cmd, object? payload = null)
    {
        var json = payload is null
            ? "{}"
            : JsonSerializer.Serialize(payload);

        var responseJson = await Task.Run(() =>
        {
            EnsureConnected();
            var hr = _com!.Invoke(cmd, json, out var result);
            if (hr < 0 && string.IsNullOrEmpty(result))
                Marshal.ThrowExceptionForHR(hr);
            return result;
        });

        var node = JsonNode.Parse(responseJson)
                   ?? throw new InvalidOperationException("Empty response");

        if (node["success"]?.GetValue<bool>() != true)
        {
            var msg = node["message"]?.GetValue<string>() ?? "unknown error";
            throw new FireBoxException(msg);
        }

        return node["body"] ?? JsonNode.Parse("null")!;
    }

    // -----------------------------------------------------------------------
    // Typed helper methods (called by ViewModels)
    // -----------------------------------------------------------------------

    public Task<JsonNode> PingAsync()
        => InvokeAsync("ping");

    public Task<JsonNode> ListProvidersAsync()
        => InvokeAsync("list_providers");

    public Task<JsonNode> AddApiKeyProviderAsync(
        string name, string providerType, string apiKey, string? baseUrl = null)
        => InvokeAsync("add_api_key_provider", new {
            name, provider_type = providerType, api_key = apiKey, base_url = baseUrl });

    public Task<JsonNode> StartOAuthProviderAsync(string name, string providerType)
        => InvokeAsync("add_oauth_provider", new { name, provider_type = providerType });

    public Task<JsonNode> CompleteOAuthAsync(string providerId)
        => InvokeAsync("complete_oauth", new { provider_id = providerId });

    public Task<JsonNode> AddLocalProviderAsync(string name, string modelPath)
        => InvokeAsync("add_local_provider", new { name, model_path = modelPath });

    public Task<JsonNode> DeleteProviderAsync(string providerId)
        => InvokeAsync("delete_provider", new { provider_id = providerId });

    public Task<JsonNode> GetAllModelsAsync(string? providerId = null)
        => InvokeAsync("get_all_models", new { provider_id = providerId });

    public Task<JsonNode> SetModelEnabledAsync(string providerId, string modelId, bool enabled)
        => InvokeAsync("set_model_enabled",
            new { provider_id = providerId, model_id = modelId, enabled });

    public Task<JsonNode> GetRouteRulesAsync(string? virtualModelId = null)
        => InvokeAsync("get_route_rules",
            virtualModelId is null ? null : new { virtual_model_id = virtualModelId });

    public Task<JsonNode> SetRouteRulesAsync(object request)
        => InvokeAsync("set_route_rules", request);

    public Task<JsonNode> GetMetricsSnapshotAsync()
        => InvokeAsync("get_metrics_snapshot");

    public Task<JsonNode> GetMetricsRangeAsync(long startMs, long endMs)
        => InvokeAsync("get_metrics_range", new { start_ms = startMs, end_ms = endMs });

    public Task<JsonNode> ListConnectionsAsync()
        => InvokeAsync("list_connections");

    public Task<JsonNode> GetAllowlistAsync()
        => InvokeAsync("get_allowlist");

    public Task<JsonNode> RemoveFromAllowlistAsync(string appPath)
        => InvokeAsync("remove_from_allowlist", new { app_path = appPath });

    public Task<JsonNode> ListAvailableModelsAsync()
        => InvokeAsync("list_available_models");

    // -----------------------------------------------------------------------
    // Connection management
    // -----------------------------------------------------------------------

    private void EnsureConnected()
    {
        if (_com is not null) return;
        var type = Type.GetTypeFromCLSID(Clsid)
                   ?? throw new InvalidOperationException("FireBox service CLSID not registered.");
        _com = (IFireBoxServiceCom)Activator.CreateInstance(type)!;
    }

    public bool TryReconnect()
    {
        _com = null;
        try { EnsureConnected(); return true; }
        catch { return false; }
    }

    ~FireBoxComService()
    {
        Dispose(false);
    }

    public void Dispose()
    {
        Dispose(true);
        GC.SuppressFinalize(this);
    }

    private void Dispose(bool disposing)
    {
        if (_disposed) return;
        _disposed = true;
        if (_com is not null && Marshal.IsComObject(_com))
            Marshal.ReleaseComObject(_com);
        _com = null;
    }
}

// ---------------------------------------------------------------------------
// Exception type
// ---------------------------------------------------------------------------

public sealed class FireBoxException(string message) : Exception(message);
