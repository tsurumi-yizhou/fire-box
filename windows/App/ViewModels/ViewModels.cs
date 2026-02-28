using System;
using System.Collections.ObjectModel;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Linq;
using System.Runtime.CompilerServices;
using System.Text.Json.Nodes;
using System.Threading;
using System.Threading.Tasks;
using App.Services;

namespace App.ViewModels;

public abstract class ViewModelBase : INotifyPropertyChanged
{
    public event PropertyChangedEventHandler? PropertyChanged;

    protected void OnPropertyChanged([CallerMemberName] string? name = null)
        => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));

    protected bool Set<T>(ref T field, T value, [CallerMemberName] string? name = null)
    {
        if (EqualityComparer<T>.Default.Equals(field, value)) return false;
        field = value;
        OnPropertyChanged(name);
        return true;
    }
}

// ---------------------------------------------------------------------------
// Dashboard
// ---------------------------------------------------------------------------

public sealed class DashboardViewModel : ViewModelBase
{
    private readonly FireBoxComService _svc = FireBoxComService.Default;
    private CancellationTokenSource? _cts;

    private long _requestsTotal;
    private long _requestsFailed;
    private long _promptTokens;
    private long _completionTokens;
    private double _costTotal;
    private long _latencyAvgMs;
    private int _activeConnections;
    private string _status = ResourceHelper.GetString("DashboardStatusLoading");

    public long RequestsTotal     { get => _requestsTotal;     private set => Set(ref _requestsTotal, value); }
    public long RequestsFailed    { get => _requestsFailed;    private set => Set(ref _requestsFailed, value); }
    public long PromptTokens      { get => _promptTokens;      private set => Set(ref _promptTokens, value); }
    public long CompletionTokens  { get => _completionTokens;  private set => Set(ref _completionTokens, value); }
    public double CostTotal
    {
        get => _costTotal;
        private set
        {
            if (!Set(ref _costTotal, value)) return;
            OnPropertyChanged(nameof(CostTotalFormatted));
        }
    }
    public long LatencyAvgMs      { get => _latencyAvgMs;      private set => Set(ref _latencyAvgMs, value); }
    public int ActiveConnections  { get => _activeConnections; private set => Set(ref _activeConnections, value); }
    public string Status          { get => _status;            private set => Set(ref _status, value); }

    public string CostTotalFormatted => $"${_costTotal:F4}";

    public void StartPolling()
    {
        _cts = new CancellationTokenSource();
        _ = PollAsync(_cts.Token);
    }

    public void StopPolling() => _cts?.Cancel();

    private async Task PollAsync(CancellationToken ct)
    {
        while (!ct.IsCancellationRequested)
        {
            try
            {
                var snap = await _svc.GetMetricsSnapshotAsync();
                var s = snap["snapshot"]!;
                RequestsTotal    = s["requests_total"]?.GetValue<long>() ?? 0;
                RequestsFailed   = s["requests_failed"]?.GetValue<long>() ?? 0;
                PromptTokens     = s["prompt_tokens_total"]?.GetValue<long>() ?? 0;
                CompletionTokens = s["completion_tokens_total"]?.GetValue<long>() ?? 0;
                CostTotal        = s["cost_total"]?.GetValue<double>() ?? 0;
                LatencyAvgMs     = s["latency_avg_ms"]?.GetValue<long>() ?? 0;

                var conns = await _svc.ListConnectionsAsync();
                ActiveConnections = conns["connections"]?.AsArray().Count ?? 0;

                Status = ResourceHelper.GetString("DashboardStatusConnected");
            }
            catch (FireBoxException ex) { Status = ex.Message; }
            catch { Status = ResourceHelper.GetString("DashboardStatusUnavailable"); }

            await Task.Delay(5_000, ct).ConfigureAwait(ConfigureAwaitOptions.SuppressThrowing);
        }
    }
}

// ---------------------------------------------------------------------------
// Connections
// ---------------------------------------------------------------------------

public sealed class ConnectionsViewModel : ViewModelBase
{
    private readonly FireBoxComService _svc = FireBoxComService.Default;
    private bool _isLoading;

    public ObservableCollection<ConnectionDto> Connections { get; } = [];

    public bool IsLoading { get => _isLoading; private set => Set(ref _isLoading, value); }

    public async Task LoadAsync()
    {
        IsLoading = true;
        try
        {
            var body = await _svc.ListConnectionsAsync();
            Connections.Clear();
            foreach (var item in body["connections"]?.AsArray() ?? [])
            {
                Connections.Add(new ConnectionDto(
                    item!["connection_id"]?.GetValue<string>() ?? "",
                    item["client_name"]?.GetValue<string>() ?? "",
                    item["app_path"]?.GetValue<string>() ?? "",
                    item["requests_count"]?.GetValue<long>() ?? 0,
                    item["connected_at_ms"]?.GetValue<long>() ?? 0));
            }
        }
        catch (Exception ex)
        {
            Trace.TraceWarning("[Connections] LoadAsync failed: {0}", ex.Message);
        }
        finally { IsLoading = false; }
    }
}

// ---------------------------------------------------------------------------
// Providers
// ---------------------------------------------------------------------------

public sealed class ProvidersViewModel : ViewModelBase
{
    private readonly FireBoxComService _svc = FireBoxComService.Default;
    private bool _isLoading;
    private string? _oauthPendingProviderId;
    private OAuthChallengeDto? _pendingChallenge;

    public ObservableCollection<ProviderDto> Providers { get; } = [];

    public bool IsLoading { get => _isLoading; private set => Set(ref _isLoading, value); }

    public OAuthChallengeDto? PendingChallenge
    {
        get => _pendingChallenge;
        private set => Set(ref _pendingChallenge, value);
    }

    public async Task LoadAsync()
    {
        IsLoading = true;
        try
        {
            var body = await _svc.ListProvidersAsync();
            Providers.Clear();
            foreach (var item in body["providers"]?.AsArray() ?? [])
            {
                Providers.Add(new ProviderDto(
                    item!["provider_id"]?.GetValue<string>() ?? "",
                    item["name"]?.GetValue<string>() ?? "",
                    (ProviderType)(item["type"]?.GetValue<int>() ?? 1),
                    item["base_url"]?.GetValue<string>() ?? ""));
            }
        }
        catch (Exception ex)
        {
            Trace.TraceWarning("[Providers] LoadAsync failed: {0}", ex.Message);
        }
        finally { IsLoading = false; }
    }

    public async Task AddApiKeyProviderAsync(
        string name, string providerType, string apiKey, string? baseUrl)
    {
        await _svc.AddApiKeyProviderAsync(name, providerType, apiKey, baseUrl);
        await LoadAsync();
    }

    /// Step 1 of OAuth: returns challenge for UI to display.
    public async Task<OAuthChallengeDto> StartOAuthAsync(string name, string providerType)
    {
        var body = await _svc.StartOAuthProviderAsync(name, providerType);
        _oauthPendingProviderId = body["provider_id"]?.GetValue<string>();
        var ch = body["challenge"]!;
        PendingChallenge = new OAuthChallengeDto(
            ch["device_code"]?.GetValue<string>() ?? "",
            ch["user_code"]?.GetValue<string>() ?? "",
            ch["verification_uri"]?.GetValue<string>() ?? "",
            ch["expires_in"]?.GetValue<long>() ?? 0,
            ch["interval"]?.GetValue<long>() ?? 5);
        return PendingChallenge;
    }

    /// Step 2 of OAuth: poll until user authorises.
    public async Task CompleteOAuthAsync()
    {
        if (_oauthPendingProviderId is null) return;
        await _svc.CompleteOAuthAsync(_oauthPendingProviderId);
        PendingChallenge = null;
        _oauthPendingProviderId = null;
        await LoadAsync();
    }

    public void CancelOAuth()
    {
        PendingChallenge = null;
        _oauthPendingProviderId = null;
    }

    public async Task AddLocalProviderAsync(string name, string modelPath)
    {
        await _svc.AddLocalProviderAsync(name, modelPath);
        await LoadAsync();
    }

    public async Task DeleteProviderAsync(string providerId)
    {
        await _svc.DeleteProviderAsync(providerId);
        var provider = Providers.FirstOrDefault(p => p.ProviderId == providerId);
        if (provider is not null) Providers.Remove(provider);
    }
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

public sealed class RoutesViewModel : ViewModelBase
{
    private readonly FireBoxComService _svc = FireBoxComService.Default;
    private bool _isLoading;

    public ObservableCollection<RouteRuleDto> Routes { get; } = [];

    public bool IsLoading { get => _isLoading; private set => Set(ref _isLoading, value); }

    public async Task LoadAsync()
    {
        IsLoading = true;
        try
        {
            var body = await _svc.GetRouteRulesAsync();
            Routes.Clear();
            foreach (var item in body["rules"]?.AsArray() ?? [])
            {
                var caps = item!["capabilities"];
                var targets = (item["targets"]?.AsArray() ?? [])
                    .Select(t => new RouteTargetDto(
                        t!["provider_id"]?.GetValue<string>() ?? "",
                        t["model_id"]?.GetValue<string>() ?? ""))
                    .ToArray();
                Routes.Add(new RouteRuleDto(
                    item["virtual_model_id"]?.GetValue<string>() ?? "",
                    item["display_name"]?.GetValue<string>() ?? "",
                    item["strategy"]?.GetValue<string>() ?? "failover",
                    targets,
                    new RouteCapabilitiesDto(
                        caps?["chat"]?.GetValue<bool>() ?? true,
                        caps?["streaming"]?.GetValue<bool>() ?? true,
                        caps?["embeddings"]?.GetValue<bool>() ?? false,
                        caps?["vision"]?.GetValue<bool>() ?? false,
                        caps?["tool_calling"]?.GetValue<bool>() ?? false)));
            }
        }
        catch (Exception ex)
        {
            Trace.TraceWarning("[Routes] LoadAsync failed: {0}", ex.Message);
        }
        finally { IsLoading = false; }
    }

    public async Task SaveRouteAsync(
        string virtualModelId, string displayName, string strategy,
        RouteTargetDto[] targets, RouteCapabilitiesDto caps)
    {
        await _svc.SetRouteRulesAsync(new {
            virtual_model_id = virtualModelId,
            display_name     = displayName,
            strategy,
            targets          = targets.Select(t => new { provider_id = t.ProviderId, model_id = t.ModelId }),
            capabilities     = new {
                chat = caps.Chat, streaming = caps.Streaming, embeddings = caps.Embeddings,
                vision = caps.Vision, tool_calling = caps.ToolCalling },
        });
        await LoadAsync();
    }

    public async Task DeleteRouteAsync(string virtualModelId)
    {
        await _svc.InvokeAsync("delete_route", new { virtual_model_id = virtualModelId });
        var rule = Routes.FirstOrDefault(r => r.VirtualModelId == virtualModelId);
        if (rule is not null) Routes.Remove(rule);
    }
}

// ---------------------------------------------------------------------------
// Allowlist
// ---------------------------------------------------------------------------

public sealed class AllowlistViewModel : ViewModelBase
{
    private readonly FireBoxComService _svc = FireBoxComService.Default;
    private bool _isLoading;

    public ObservableCollection<AllowlistEntryDto> Entries { get; } = [];

    public bool IsLoading { get => _isLoading; private set => Set(ref _isLoading, value); }

    public async Task LoadAsync()
    {
        IsLoading = true;
        try
        {
            var body = await _svc.GetAllowlistAsync();
            Entries.Clear();
            foreach (var item in body["apps"]?.AsArray() ?? [])
            {
                Entries.Add(new AllowlistEntryDto(
                    item!["app_path"]?.GetValue<string>() ?? "",
                    item["display_name"]?.GetValue<string>() ?? ""));
            }
        }
        catch (Exception ex)
        {
            Trace.TraceWarning("[Allowlist] LoadAsync failed: {0}", ex.Message);
        }
        finally { IsLoading = false; }
    }

    public async Task RevokeAsync(string appPath)
    {
        await _svc.RemoveFromAllowlistAsync(appPath);
        var entry = Entries.FirstOrDefault(e => e.AppPath == appPath);
        if (entry is not null) Entries.Remove(entry);
    }
}
