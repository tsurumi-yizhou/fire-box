use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use anyhow::Result;
use tokio::sync::{Mutex, Notify};
use tokio::task::JoinHandle;
use zbus::{ConnectionBuilder, dbus_interface, message::Header};

use super::connections::ConnectionRegistry;
use crate::middleware::{access, config, metadata as meta_mw, metrics, route};
use crate::providers::config as pconfig;
use crate::providers::{
    ChatMessage, CompletionRequest, EmbeddingRequest, StreamEvent, Tool, ToolCall,
    ToolCallFunction, ToolFunction, Usage,
};

/// Shared connection registry for D-Bus clients.
static DBUS_REGISTRY: LazyLock<ConnectionRegistry> = LazyLock::new(ConnectionRegistry::new);

/// Shared zbus connection for PID lookups via the bus daemon.
static DBUS_CONN: LazyLock<tokio::sync::OnceCell<zbus::Connection>> =
    LazyLock::new(tokio::sync::OnceCell::new);

pub async fn run_listener() -> Result<()> {
    let conn = ConnectionBuilder::session()?
        .name("com.firebox.Service")?
        .serve_at("/com/firebox/Service", FireBoxDbus)?
        .build()
        .await?;

    let _ = DBUS_CONN.set(conn);

    loop {
        tokio::time::sleep(Duration::from_secs(
            crate::providers::consts::IPC_LISTENER_SLEEP_SECS,
        ))
        .await;
    }
}

// ---------------------------------------------------------------------------
// TOFU helpers
// ---------------------------------------------------------------------------

async fn resolve_sender(sender: &str) -> (String, String) {
    let fallback = || (sender.to_string(), sender.to_string());

    let Some(conn) = DBUS_CONN.get() else {
        return fallback();
    };

    let pid = match zbus::fdo::DBusProxy::new(conn).await {
        Ok(proxy) => match proxy.get_connection_unix_process_id(sender.into()).await {
            Ok(pid) => pid,
            Err(e) => {
                tracing::warn!("D-Bus: failed to get PID for {sender}: {e}");
                return fallback();
            }
        },
        Err(e) => {
            tracing::warn!("D-Bus: failed to create DBusProxy: {e}");
            return fallback();
        }
    };

    resolve_pid_linux(pid)
}

fn resolve_pid_linux(pid: u32) -> (String, String) {
    let link = format!("/proc/{pid}/exe");
    match std::fs::read_link(&link) {
        Ok(path) => {
            let exe = path.to_string_lossy().into_owned();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| exe.clone());
            (exe, name)
        }
        Err(_) => {
            let fallback = format!("pid:{pid}");
            (fallback.clone(), fallback)
        }
    }
}

async fn check_caller_access(header: &Header<'_>) -> bool {
    let sender = match header.sender() {
        Ok(Some(s)) => s.to_string(),
        _ => return false,
    };

    let (app_path, display_name) = resolve_sender(&sender).await;

    match access::check_access(&app_path).await {
        access::AccessDecision::Allow => true,
        access::AccessDecision::Deny => false,
        access::AccessDecision::Unknown => {
            // Rate-limit repeated TOFU prompts for the same app.
            if access::is_tofu_rate_limited(&app_path) {
                tracing::warn!("TOFU rate limit exceeded for {app_path} — denying");
                return false;
            }
            let granted = show_tofu_prompt(&app_path, &display_name).await;
            if granted {
                if let Err(e) = access::grant_access(&app_path, &display_name).await {
                    tracing::warn!("Failed to persist TOFU grant for {app_path}: {e}");
                }
                true
            } else {
                access::record_tofu_failure(&app_path);
                if let Err(e) = access::deny_access(&app_path, &display_name).await {
                    tracing::warn!("Failed to persist TOFU deny for {app_path}: {e}");
                }
                false
            }
        }
    }
}

async fn show_tofu_prompt(app_path: &str, display_name: &str) -> bool {
    let timeout = crate::providers::consts::TOFU_PROMPT_TIMEOUT;
    let text = format!(
        "Allow \"{}\" to access FireBox?\n\n{}",
        display_name, app_path
    );
    let result = tokio::time::timeout(
        timeout,
        tokio::process::Command::new("zenity")
            .arg("--question")
            .arg("--title=FireBox Access Request")
            .arg(format!("--text={text}"))
            .output(),
    )
    .await;
    match result {
        Ok(Ok(o)) => o.status.success(),
        Ok(Err(_)) => true,
        Err(_) => {
            tracing::warn!(
                "TOFU prompt timed out after {}s — denying",
                timeout.as_secs()
            );
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Pending OAuth state
// ---------------------------------------------------------------------------

enum PendingOAuth {
    Copilot {
        device_code: String,
        interval: u64,
        expires_in: u64,
    },
    DashScope {
        flow: crate::providers::dashscope::QwenOAuthFlow,
    },
}

static PENDING_OAUTH: LazyLock<Mutex<HashMap<String, PendingOAuth>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// ---------------------------------------------------------------------------
// Streaming session state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum StreamChunk {
    Delta(String),
    ToolCalls(Vec<ToolCall>),
    Done {
        usage: Option<Usage>,
        finish_reason: Option<String>,
    },
    Error(String),
}

struct StreamSession {
    provider_id: String,
    model_id: String,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    messages: Mutex<Vec<ChatMessage>>,
    tools: Mutex<Vec<Tool>>,
    pending: Mutex<VecDeque<StreamChunk>>,
    notify: Notify,
    done: AtomicBool,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl StreamSession {
    fn new(
        provider_id: String,
        model_id: String,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
    ) -> Arc<Self> {
        Arc::new(Self {
            provider_id,
            model_id,
            temperature,
            max_tokens,
            messages: Mutex::new(Vec::new()),
            tools: Mutex::new(Vec::new()),
            pending: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
            done: AtomicBool::new(false),
            task: Mutex::new(None),
        })
    }
}

static SESSIONS: LazyLock<Mutex<HashMap<String, Arc<StreamSession>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn new_stream_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let addr = &nanos as *const u32 as usize;
    format!("s-{addr:x}-{nanos:x}")
}

// ---------------------------------------------------------------------------
// D-Bus interface — all methods use native D-Bus types, no JSON
// ---------------------------------------------------------------------------

struct FireBoxDbus;

#[dbus_interface(name = "com.firebox.Service")]
impl FireBoxDbus {
    // -----------------------------------------------------------------------
    // ping
    // -----------------------------------------------------------------------

    async fn ping(&self) -> (bool, String) {
        (true, String::new())
    }

    // -----------------------------------------------------------------------
    // Provider management
    // -----------------------------------------------------------------------

    /// Returns (success, provider_id, error).
    async fn add_api_key_provider(
        &self,
        #[zbus(header)] header: Header<'_>,
        name: String,
        provider_type: String,
        api_key: String,
        base_url: String,
    ) -> (bool, String, String) {
        if !check_caller_access(&header).await {
            return (false, String::new(), "access denied".into());
        }
        if name.is_empty() || provider_type.is_empty() {
            return (
                false,
                String::new(),
                "name and provider_type are required".into(),
            );
        }

        let cfg = match provider_type.as_str() {
            "openai" => pconfig::ProviderConfig::openai(
                &api_key,
                if base_url.is_empty() {
                    None
                } else {
                    Some(base_url.clone())
                },
            ),
            "anthropic" => pconfig::ProviderConfig::anthropic(
                &api_key,
                if base_url.is_empty() {
                    None
                } else {
                    Some(base_url.clone())
                },
            ),
            "ollama" => pconfig::ProviderConfig::ollama(if base_url.is_empty() {
                None
            } else {
                Some(base_url.clone())
            }),
            "vllm" => pconfig::ProviderConfig::vllm(
                if api_key.is_empty() {
                    None
                } else {
                    Some(api_key.clone())
                },
                if base_url.is_empty() {
                    None
                } else {
                    Some(base_url.clone())
                },
            ),
            other => {
                return (
                    false,
                    String::new(),
                    format!("unsupported provider_type: {other}"),
                );
            }
        };

        let profile_id = name.to_lowercase().replace(' ', "_");

        match pconfig::configure_provider(&profile_id, &cfg).await {
            Ok(_) => {
                if let Err(e) = pconfig::add_to_provider_index(&profile_id).await {
                    if let Err(e2) = pconfig::remove_provider(&profile_id).await {
                        tracing::warn!("Cleanup: failed to remove provider {profile_id}: {e2}");
                    }
                    return (
                        false,
                        String::new(),
                        format!("failed to update provider index: {e}"),
                    );
                }
                let pid = profile_id.clone();
                let nm = name.clone();
                if let Err(e) = config::update_config(move |d| {
                    d.display_names.insert(pid, nm);
                })
                .await
                {
                    if let Err(e2) = pconfig::remove_from_provider_index(&profile_id).await {
                        tracing::warn!("Cleanup: failed to remove {profile_id} from index: {e2}");
                    }
                    if let Err(e2) = pconfig::remove_provider(&profile_id).await {
                        tracing::warn!("Cleanup: failed to remove provider {profile_id}: {e2}");
                    }
                    return (
                        false,
                        String::new(),
                        format!("failed to persist provider metadata: {e}"),
                    );
                }
                (true, profile_id, String::new())
            }
            Err(e) => (false, String::new(), e.to_string()),
        }
    }

    /// Returns (success, provider_id, user_code, verification_uri, expires_in, error).
    async fn add_oauth_provider(
        &self,
        #[zbus(header)] header: Header<'_>,
        name: String,
        provider_type: String,
    ) -> (bool, String, String, String, u64, String) {
        if !check_caller_access(&header).await {
            return (
                false,
                String::new(),
                String::new(),
                String::new(),
                0,
                "access denied".into(),
            );
        }
        if name.is_empty() || provider_type.is_empty() {
            return (
                false,
                String::new(),
                String::new(),
                String::new(),
                0,
                "name and provider_type are required".into(),
            );
        }

        let profile_id = name.to_lowercase().replace(' ', "_");

        match provider_type.as_str() {
            "copilot" => {
                use crate::providers::copilot::CopilotProvider;
                match CopilotProvider::start_device_flow(None).await {
                    Ok(dc) => {
                        let cfg = pconfig::ProviderConfig::copilot_pending(None);
                        if let Err(e) = pconfig::configure_provider(&profile_id, &cfg).await {
                            return (
                                false,
                                String::new(),
                                String::new(),
                                String::new(),
                                0,
                                format!("failed to persist pending config: {e}"),
                            );
                        }
                        if let Err(e) = pconfig::add_to_provider_index(&profile_id).await {
                            let _ = pconfig::remove_provider(&profile_id).await;
                            return (
                                false,
                                String::new(),
                                String::new(),
                                String::new(),
                                0,
                                format!("failed to update provider index: {e}"),
                            );
                        }
                        let pid = profile_id.clone();
                        let nm = name.clone();
                        if let Err(e) = config::update_config(move |d| {
                            d.display_names.insert(pid, nm);
                        })
                        .await
                        {
                            let _ = pconfig::remove_from_provider_index(&profile_id).await;
                            let _ = pconfig::remove_provider(&profile_id).await;
                            return (
                                false,
                                String::new(),
                                String::new(),
                                String::new(),
                                0,
                                format!("failed to persist metadata: {e}"),
                            );
                        }
                        let user_code = dc.user_code.clone();
                        let uri = dc.verification_uri.clone();
                        let expires = dc.expires_in;
                        PENDING_OAUTH.lock().await.insert(
                            profile_id.clone(),
                            PendingOAuth::Copilot {
                                device_code: dc.device_code,
                                interval: dc.interval,
                                expires_in: dc.expires_in,
                            },
                        );
                        (true, profile_id, user_code, uri, expires, String::new())
                    }
                    Err(e) => (
                        false,
                        String::new(),
                        String::new(),
                        String::new(),
                        0,
                        e.to_string(),
                    ),
                }
            }
            "dashscope" => {
                use crate::providers::dashscope::QwenOAuthFlow;
                match QwenOAuthFlow::start(None).await {
                    Ok(flow) => {
                        let cfg = pconfig::ProviderConfig::DashScope(pconfig::DashScopeConfig {
                            access_token: None,
                            refresh_token: None,
                            resource_url: None,
                            expiry_date: None,
                            base_url: None,
                        });
                        if let Err(e) = pconfig::configure_provider(&profile_id, &cfg).await {
                            return (
                                false,
                                String::new(),
                                String::new(),
                                String::new(),
                                0,
                                format!("failed to persist pending config: {e}"),
                            );
                        }
                        if let Err(e) = pconfig::add_to_provider_index(&profile_id).await {
                            let _ = pconfig::remove_provider(&profile_id).await;
                            return (
                                false,
                                String::new(),
                                String::new(),
                                String::new(),
                                0,
                                format!("failed to update provider index: {e}"),
                            );
                        }
                        let pid = profile_id.clone();
                        let nm = name.clone();
                        if let Err(e) = config::update_config(move |d| {
                            d.display_names.insert(pid, nm);
                        })
                        .await
                        {
                            let _ = pconfig::remove_from_provider_index(&profile_id).await;
                            let _ = pconfig::remove_provider(&profile_id).await;
                            return (
                                false,
                                String::new(),
                                String::new(),
                                String::new(),
                                0,
                                format!("failed to persist metadata: {e}"),
                            );
                        }
                        let dc = flow.device_code_response();
                        let user_code = dc.user_code.clone();
                        let uri = dc.verification_uri.clone();
                        let expires = dc.expires_in;
                        PENDING_OAUTH
                            .lock()
                            .await
                            .insert(profile_id.clone(), PendingOAuth::DashScope { flow });
                        (true, profile_id, user_code, uri, expires, String::new())
                    }
                    Err(e) => (
                        false,
                        String::new(),
                        String::new(),
                        String::new(),
                        0,
                        e.to_string(),
                    ),
                }
            }
            other => (
                false,
                String::new(),
                String::new(),
                String::new(),
                0,
                format!("unsupported oauth provider_type: {other}"),
            ),
        }
    }

    /// Returns (success, error).
    async fn complete_oauth(
        &self,
        #[zbus(header)] header: Header<'_>,
        provider_id: String,
    ) -> (bool, String) {
        if !check_caller_access(&header).await {
            return (false, "access denied".into());
        }
        if provider_id.is_empty() {
            return (false, "provider_id is required".into());
        }

        let pending = PENDING_OAUTH.lock().await.remove(&provider_id);
        let Some(pending) = pending else {
            return (false, format!("no pending OAuth for {provider_id}"));
        };

        match pending {
            PendingOAuth::Copilot {
                device_code,
                interval,
                expires_in,
            } => {
                use crate::providers::copilot::CopilotProvider;
                match CopilotProvider::poll_for_token(None, &device_code, interval, expires_in)
                    .await
                {
                    Ok(token) => {
                        let cfg = pconfig::ProviderConfig::copilot(&token, None);
                        match pconfig::configure_provider(&provider_id, &cfg).await {
                            Ok(_) => (true, String::new()),
                            Err(e) => (false, e.to_string()),
                        }
                    }
                    Err(e) => (false, e.to_string()),
                }
            }
            PendingOAuth::DashScope { flow } => match flow.wait_for_token().await {
                Ok(creds) => {
                    let cfg = pconfig::ProviderConfig::dashscope_oauth(
                        &creds.access_token,
                        creds.refresh_token.as_deref().unwrap_or_default(),
                        creds.expiry_date.unwrap_or(0),
                        None,
                    );
                    match pconfig::configure_provider(&provider_id, &cfg).await {
                        Ok(_) => (true, String::new()),
                        Err(e) => (false, e.to_string()),
                    }
                }
                Err(e) => (false, e.to_string()),
            },
        }
    }

    /// Returns (success, provider_id, error).
    async fn add_local_provider(
        &self,
        #[zbus(header)] header: Header<'_>,
        name: String,
        model_path: String,
        context_size: u32,
        gpu_layers: i32,
    ) -> (bool, String, String) {
        if !check_caller_access(&header).await {
            return (false, String::new(), "access denied".into());
        }
        if name.is_empty() || model_path.is_empty() {
            return (
                false,
                String::new(),
                "name and model_path are required".into(),
            );
        }

        let profile_id = name.to_lowercase().replace(' ', "_");
        let cfg = pconfig::ProviderConfig::llamacpp(&model_path);

        match pconfig::configure_provider(&profile_id, &cfg).await {
            Ok(_) => {
                if let Err(e) = pconfig::add_to_provider_index(&profile_id).await {
                    let _ = pconfig::remove_provider(&profile_id).await;
                    return (
                        false,
                        String::new(),
                        format!("failed to update provider index: {e}"),
                    );
                }
                let pid = profile_id.clone();
                let nm = name.clone();
                if let Err(e) = config::update_config(move |d| {
                    d.display_names.insert(pid, nm);
                })
                .await
                {
                    let _ = pconfig::remove_from_provider_index(&profile_id).await;
                    let _ = pconfig::remove_provider(&profile_id).await;
                    return (
                        false,
                        String::new(),
                        format!("failed to persist metadata: {e}"),
                    );
                }
                (true, profile_id, String::new())
            }
            Err(e) => (false, String::new(), e.to_string()),
        }
    }

    /// Returns (success, provider_ids, error).
    async fn list_providers(
        &self,
        #[zbus(header)] header: Header<'_>,
    ) -> (bool, Vec<String>, String) {
        if !check_caller_access(&header).await {
            return (false, Vec::new(), "access denied".into());
        }
        (true, pconfig::load_provider_index().await, String::new())
    }

    /// Returns (success, error).
    async fn delete_provider(
        &self,
        #[zbus(header)] header: Header<'_>,
        provider_id: String,
    ) -> (bool, String) {
        if !check_caller_access(&header).await {
            return (false, "access denied".into());
        }
        if provider_id.is_empty() {
            return (false, "provider_id is required".into());
        }
        if let Err(e) = pconfig::remove_from_provider_index(&provider_id).await {
            return (false, format!("failed to remove from index: {e}"));
        }
        if let Err(e) = pconfig::remove_provider(&provider_id).await {
            return (false, format!("failed to remove provider config: {e}"));
        }
        let pid = provider_id.clone();
        if let Err(e) = config::update_config(move |d| {
            d.display_names.remove(&pid);
        })
        .await
        {
            tracing::warn!("Failed to remove display name for {provider_id}: {e}");
        }
        (true, String::new())
    }

    /// Returns (success, models as Vec<(model_id, enabled)>, error).
    async fn get_all_models(
        &self,
        #[zbus(header)] header: Header<'_>,
        provider_id: String,
    ) -> (bool, Vec<(String, bool)>, String) {
        if !check_caller_access(&header).await {
            return (false, Vec::new(), "access denied".into());
        }
        if provider_id.is_empty() {
            return (false, Vec::new(), "provider_id is required".into());
        }
        let provider = match pconfig::load_provider(&provider_id).await {
            Ok(p) => p,
            Err(e) => return (false, Vec::new(), e.to_string()),
        };
        let models = provider.list_models_dyn().await.unwrap_or_default();
        let mut out = Vec::new();
        for m in models {
            let enabled = route::is_model_enabled(&provider_id, &m.id).await;
            out.push((m.id, enabled));
        }
        (true, out, String::new())
    }

    /// Returns (success, error).
    async fn set_model_enabled(
        &self,
        #[zbus(header)] header: Header<'_>,
        provider_id: String,
        model_id: String,
        enabled: bool,
    ) -> (bool, String) {
        if !check_caller_access(&header).await {
            return (false, "access denied".into());
        }
        if provider_id.is_empty() || model_id.is_empty() {
            return (false, "provider_id and model_id are required".into());
        }
        let provider = match pconfig::load_provider(&provider_id).await {
            Ok(p) => p,
            Err(e) => return (false, e.to_string()),
        };
        let all_models: Vec<String> = provider
            .list_models_dyn()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|m| m.id)
            .collect();
        match route::toggle_model(&provider_id, &model_id, enabled, &all_models).await {
            Ok(_) => (true, String::new()),
            Err(e) => (false, e.to_string()),
        }
    }

    // -----------------------------------------------------------------------
    // Route rules
    // -----------------------------------------------------------------------

    /// Returns (success, error).
    /// `targets`: Vec of (provider_id, model_id).
    /// `capabilities`: (chat, streaming, embeddings, vision, tool_calling).
    /// `metadata_context_window`, `metadata_pricing_tier`, `metadata_strengths`, `metadata_description`.
    async fn set_route_rules(
        &self,
        #[zbus(header)] header: Header<'_>,
        virtual_model_id: String,
        display_name: String,
        strategy: String,
        targets: Vec<(String, String)>,
        cap_chat: bool,
        cap_streaming: bool,
        cap_embeddings: bool,
        cap_vision: bool,
        cap_tool_calling: bool,
        meta_context_window: u32,
        meta_pricing_tier: String,
        meta_strengths: Vec<String>,
        meta_description: String,
    ) -> (bool, String) {
        if !check_caller_access(&header).await {
            return (false, "access denied".into());
        }
        if virtual_model_id.is_empty() {
            return (false, "virtual_model_id is required".into());
        }
        let dn = if display_name.is_empty() {
            virtual_model_id.clone()
        } else {
            display_name
        };
        let strat = match strategy.as_str() {
            "random" => route::RouteStrategy::Random,
            _ => route::RouteStrategy::Failover,
        };
        let caps = route::RouteCapabilities {
            chat: cap_chat,
            streaming: cap_streaming,
            embeddings: cap_embeddings,
            vision: cap_vision,
            tool_calling: cap_tool_calling,
        };
        let meta = route::RouteMetadata {
            context_window: if meta_context_window == 0 {
                None
            } else {
                Some(meta_context_window)
            },
            pricing_tier: if meta_pricing_tier.is_empty() {
                None
            } else {
                Some(meta_pricing_tier)
            },
            strengths: meta_strengths,
            description: if meta_description.is_empty() {
                None
            } else {
                Some(meta_description)
            },
        };
        let tgts: Vec<route::RouteTarget> = targets
            .into_iter()
            .filter(|(p, m)| !p.is_empty() && !m.is_empty())
            .map(|(p, m)| route::RouteTarget {
                provider_id: p,
                model_id: m,
            })
            .collect();
        match route::set_route_rules_with_options(&virtual_model_id, &dn, caps, meta, tgts, strat)
            .await
        {
            Ok(_) => (true, String::new()),
            Err(e) => (false, e.to_string()),
        }
    }

    /// Returns (success, rules: Vec<(virtual_model_id, display_name, strategy, targets: Vec<(pid, mid)>)>, error).
    async fn get_route_rules(
        &self,
        #[zbus(header)] header: Header<'_>,
        virtual_model_id: String,
    ) -> (
        bool,
        Vec<(String, String, String, Vec<(String, String)>)>,
        String,
    ) {
        if !check_caller_access(&header).await {
            return (false, Vec::new(), "access denied".into());
        }
        let encode = |r: &route::RouteRule| {
            let strat = match r.strategy {
                route::RouteStrategy::Random => "random",
                route::RouteStrategy::Failover => "failover",
            };
            let tgts: Vec<(String, String)> = r
                .targets
                .iter()
                .map(|t| (t.provider_id.clone(), t.model_id.clone()))
                .collect();
            (
                r.virtual_model_id.clone(),
                r.display_name.clone(),
                strat.to_string(),
                tgts,
            )
        };
        if !virtual_model_id.is_empty() {
            match route::get_route_rules(&virtual_model_id).await {
                Ok(Some(rule)) => (true, vec![encode(&rule)], String::new()),
                Ok(None) => (
                    false,
                    Vec::new(),
                    format!("route '{}' not found", virtual_model_id),
                ),
                Err(e) => (false, Vec::new(), e.to_string()),
            }
        } else {
            match route::get_all_rules().await {
                Ok(rules) => (true, rules.iter().map(encode).collect(), String::new()),
                Err(e) => (false, Vec::new(), e.to_string()),
            }
        }
    }

    /// Returns (success, error).
    async fn delete_route(
        &self,
        #[zbus(header)] header: Header<'_>,
        virtual_model_id: String,
    ) -> (bool, String) {
        if !check_caller_access(&header).await {
            return (false, "access denied".into());
        }
        if virtual_model_id.is_empty() {
            return (false, "virtual_model_id required".into());
        }
        match route::delete_route_rules(&virtual_model_id).await {
            Ok(()) => (true, String::new()),
            Err(e) => (false, e.to_string()),
        }
    }

    // -----------------------------------------------------------------------
    // Metrics
    // -----------------------------------------------------------------------

    /// Returns (success, window_start_ms, window_end_ms, requests_total, requests_failed,
    ///          prompt_tokens_total, completion_tokens_total, latency_avg_ms, cost_total).
    async fn get_metrics_snapshot(
        &self,
        #[zbus(header)] header: Header<'_>,
    ) -> (bool, u64, u64, u64, u64, u64, u64, u64, f64) {
        if !check_caller_access(&header).await {
            return (false, 0, 0, 0, 0, 0, 0, 0, 0.0);
        }
        let s = metrics::get_snapshot();
        (
            true,
            s.window_start_ms,
            s.window_end_ms,
            s.requests_total,
            s.requests_failed,
            s.prompt_tokens_total,
            s.completion_tokens_total,
            s.latency_avg_ms,
            s.cost_total,
        )
    }

    /// Returns (success, buckets: Vec<(timestamp_ms, req_total, req_failed, prompt_tok, compl_tok, cost)>, error).
    async fn get_metrics_range(
        &self,
        #[zbus(header)] header: Header<'_>,
        start_ms: u64,
        end_ms: u64,
    ) -> (bool, Vec<(u64, u64, u64, u64, u64, f64)>, String) {
        if !check_caller_access(&header).await {
            return (false, Vec::new(), "access denied".into());
        }
        let buckets = metrics::get_metrics_range(start_ms, end_ms).await;
        let out: Vec<_> = buckets
            .iter()
            .map(|b| {
                (
                    b.timestamp_ms,
                    b.requests_total,
                    b.requests_failed,
                    b.prompt_tokens,
                    b.completion_tokens,
                    b.cost_total,
                )
            })
            .collect();
        (true, out, String::new())
    }

    // -----------------------------------------------------------------------
    // Connections + allowlist
    // -----------------------------------------------------------------------

    /// Returns (success, connections: Vec<(id, name, path, requests, connected_at_ms)>, error).
    async fn list_connections(
        &self,
        #[zbus(header)] header: Header<'_>,
    ) -> (bool, Vec<(String, String, String, u64, u64)>, String) {
        if !check_caller_access(&header).await {
            return (false, Vec::new(), "access denied".into());
        }
        let conns = DBUS_REGISTRY.list();
        let out: Vec<_> = conns
            .iter()
            .map(|c| {
                (
                    c.connection_id.clone(),
                    c.client_name.clone(),
                    c.app_path.clone(),
                    c.requests_count,
                    c.connected_at_ms,
                )
            })
            .collect();
        (true, out, String::new())
    }

    /// Returns (success, apps: Vec<(app_path, display_name)>, error).
    async fn get_allowlist(
        &self,
        #[zbus(header)] header: Header<'_>,
    ) -> (bool, Vec<(String, String)>, String) {
        if !check_caller_access(&header).await {
            return (false, Vec::new(), "access denied".into());
        }
        let entries = access::get_allowlist().await;
        let out: Vec<_> = entries
            .iter()
            .map(|e| (e.app_path.clone(), e.display_name.clone()))
            .collect();
        (true, out, String::new())
    }

    /// Returns (success, error).
    async fn remove_from_allowlist(
        &self,
        #[zbus(header)] header: Header<'_>,
        app_path: String,
    ) -> (bool, String) {
        if !check_caller_access(&header).await {
            return (false, "access denied".into());
        }
        if app_path.is_empty() {
            return (false, "app_path is required".into());
        }
        match access::remove_from_allowlist(&app_path).await {
            Ok(_) => (true, String::new()),
            Err(e) => (false, e.to_string()),
        }
    }

    // -----------------------------------------------------------------------
    // Capability: list_available_models
    // -----------------------------------------------------------------------

    /// Returns (success, models: Vec<(model_id, provider_id, owner, context_window)>, error).
    async fn list_available_models(
        &self,
        #[zbus(header)] header: Header<'_>,
    ) -> (bool, Vec<(String, String, String, u32)>, String) {
        if !check_caller_access(&header).await {
            return (false, Vec::new(), "access denied".into());
        }
        let index = pconfig::load_provider_index().await;
        let mut out = Vec::new();
        for profile_id in &index {
            let provider = match pconfig::load_provider(profile_id).await {
                Ok(p) => p,
                Err(_) => continue,
            };
            let models = provider.list_models_dyn().await.unwrap_or_default();
            for m in models {
                if route::is_model_enabled(profile_id, &m.id).await {
                    out.push((
                        m.id,
                        profile_id.clone(),
                        m.owner,
                        m.context_window.unwrap_or_default(),
                    ));
                }
            }
        }
        (true, out, String::new())
    }

    // -----------------------------------------------------------------------
    // Capability: get_model_metadata
    // -----------------------------------------------------------------------

    /// Returns (success, id, name, context_window, supports_tool_calling, supports_vision, error).
    async fn get_model_metadata(
        &self,
        #[zbus(header)] header: Header<'_>,
        model_id: String,
    ) -> (bool, String, String, u32, bool, bool, String) {
        if !check_caller_access(&header).await {
            return (
                false,
                String::new(),
                String::new(),
                0,
                false,
                false,
                "access denied".into(),
            );
        }
        if model_id.is_empty() {
            return (
                false,
                String::new(),
                String::new(),
                0,
                false,
                false,
                "model_id is required".into(),
            );
        }
        let found = {
            let mut mgr = meta_mw::MetadataManager::new();
            mgr.find_model(&model_id).await.unwrap_or(None)
        };
        match found {
            Some(m) => {
                let supports_vision = m
                    .modalities
                    .as_ref()
                    .map(|md| md.input.iter().any(|s| s == "image"))
                    .unwrap_or(false);
                let ctx = m.limit.as_ref().map(|l| l.context).unwrap_or(0) as u32;
                (
                    true,
                    m.id.clone(),
                    m.name.clone(),
                    ctx,
                    m.tool_call,
                    supports_vision,
                    String::new(),
                )
            }
            None => (
                false,
                String::new(),
                String::new(),
                0,
                false,
                false,
                format!("no metadata for model '{model_id}'"),
            ),
        }
    }

    // -----------------------------------------------------------------------
    // Capability: complete (non-streaming)
    // -----------------------------------------------------------------------

    /// `messages`: Vec of (role, content).
    /// Returns (success, role, content, finish_reason, prompt_tokens, completion_tokens, total_tokens, error).
    async fn complete(
        &self,
        #[zbus(header)] header: Header<'_>,
        model_id: String,
        messages: Vec<(String, String)>,
        max_tokens: u32,
        temperature: f64,
    ) -> (bool, String, String, String, u32, u32, u32, String) {
        if !check_caller_access(&header).await {
            return (
                false,
                String::new(),
                String::new(),
                String::new(),
                0,
                0,
                0,
                "access denied".into(),
            );
        }
        if model_id.is_empty() {
            return (
                false,
                String::new(),
                String::new(),
                String::new(),
                0,
                0,
                0,
                "model_id is required".into(),
            );
        }
        let (provider_id, real_model) = match route::resolve_alias(&model_id).await {
            Ok(p) => p,
            Err(e) => {
                return (
                    false,
                    String::new(),
                    String::new(),
                    String::new(),
                    0,
                    0,
                    0,
                    e.to_string(),
                );
            }
        };
        let provider = match pconfig::load_provider(&provider_id).await {
            Ok(p) => p,
            Err(e) => {
                return (
                    false,
                    String::new(),
                    String::new(),
                    String::new(),
                    0,
                    0,
                    0,
                    e.to_string(),
                );
            }
        };
        let msgs: Vec<ChatMessage> = messages
            .into_iter()
            .map(|(role, content)| ChatMessage {
                role,
                content,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            })
            .collect();
        let request = CompletionRequest {
            model: real_model,
            messages: msgs,
            max_tokens: if max_tokens == 0 {
                None
            } else {
                Some(max_tokens)
            },
            temperature: if temperature == 0.0 {
                None
            } else {
                Some(temperature)
            },
            stream: false,
            tools: None,
        };
        match provider.complete_dyn("ipc", &request).await {
            Ok(resp) => {
                let choice = resp.choices.first();
                let role = choice.map(|c| c.message.role.clone()).unwrap_or_default();
                let content = choice
                    .map(|c| c.message.content.clone())
                    .unwrap_or_default();
                let finish = choice
                    .and_then(|c| c.finish_reason.clone())
                    .unwrap_or_default();
                let (pt, ct, tt) = resp
                    .usage
                    .map(|u| (u.prompt_tokens, u.completion_tokens, u.total_tokens))
                    .unwrap_or((0, 0, 0));
                (true, role, content, finish, pt, ct, tt, String::new())
            }
            Err(e) => (
                false,
                String::new(),
                String::new(),
                String::new(),
                0,
                0,
                0,
                e.to_string(),
            ),
        }
    }

    // -----------------------------------------------------------------------
    // Capability: streaming
    // -----------------------------------------------------------------------

    /// Returns (success, stream_id, error).
    async fn create_stream(
        &self,
        #[zbus(header)] header: Header<'_>,
        model_id: String,
        temperature: f64,
        max_tokens: u32,
    ) -> (bool, String, String) {
        if !check_caller_access(&header).await {
            return (false, String::new(), "access denied".into());
        }
        if model_id.is_empty() {
            return (false, String::new(), "model_id is required".into());
        }
        let temp = if temperature == 0.0 {
            None
        } else {
            Some(temperature)
        };
        let mt = if max_tokens == 0 {
            None
        } else {
            Some(max_tokens)
        };
        let (provider_id, real_model) = match route::resolve_alias(&model_id).await {
            Ok(p) => p,
            Err(e) => return (false, String::new(), e.to_string()),
        };
        let session = StreamSession::new(provider_id, real_model, temp, mt);
        let stream_id = new_stream_id();
        SESSIONS.lock().await.insert(stream_id.clone(), session);
        (true, stream_id, String::new())
    }

    /// Returns (success, error).
    async fn send_message(
        &self,
        #[zbus(header)] header: Header<'_>,
        stream_id: String,
        role: String,
        content: String,
    ) -> (bool, String) {
        if !check_caller_access(&header).await {
            return (false, "access denied".into());
        }
        if stream_id.is_empty() {
            return (false, "stream_id is required".into());
        }
        let session = {
            let reg = SESSIONS.lock().await;
            match reg.get(&stream_id) {
                Some(s) => Arc::clone(s),
                None => return (false, "stream_id not found".into()),
            }
        };

        let msg = ChatMessage {
            role: if role.is_empty() { "user".into() } else { role },
            content,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        };
        session.messages.lock().await.push(msg);
        session.done.store(false, Ordering::SeqCst);

        let provider_id = session.provider_id.clone();
        let real_model = session.model_id.clone();
        let temperature = session.temperature;
        let max_tokens = session.max_tokens;
        let all_msgs = session.messages.lock().await.clone();
        let all_tools = session.tools.lock().await.clone();
        let sess = Arc::clone(&session);

        let task = tokio::spawn(async move {
            let provider = match pconfig::load_provider(&provider_id).await {
                Ok(p) => p,
                Err(e) => {
                    sess.pending
                        .lock()
                        .await
                        .push_back(StreamChunk::Error(e.to_string()));
                    sess.notify.notify_one();
                    sess.done.store(true, Ordering::SeqCst);
                    return;
                }
            };
            let request = CompletionRequest {
                model: real_model,
                messages: all_msgs,
                max_tokens,
                temperature,
                stream: true,
                tools: if all_tools.is_empty() {
                    None
                } else {
                    Some(all_tools)
                },
            };
            match provider.complete_stream_dyn("ipc-stream", &request).await {
                Ok(stream) => {
                    use tokio_stream::StreamExt as _;
                    let mut tool_buf: Vec<ToolCall> = Vec::new();
                    tokio::pin!(stream);
                    while let Some(event) = stream.next().await {
                        match event {
                            Ok(StreamEvent::Delta { content }) => {
                                sess.pending
                                    .lock()
                                    .await
                                    .push_back(StreamChunk::Delta(content));
                                sess.notify.notify_one();
                            }
                            Ok(StreamEvent::ToolCalls { tool_calls }) => {
                                tool_buf.extend(tool_calls);
                            }
                            Ok(StreamEvent::Done) => {
                                if !tool_buf.is_empty() {
                                    sess.pending.lock().await.push_back(StreamChunk::ToolCalls(
                                        std::mem::take(&mut tool_buf),
                                    ));
                                    sess.notify.notify_one();
                                }
                                sess.pending.lock().await.push_back(StreamChunk::Done {
                                    usage: None,
                                    finish_reason: Some("stop".into()),
                                });
                                sess.notify.notify_one();
                                break;
                            }
                            Ok(StreamEvent::Error { message }) => {
                                sess.pending
                                    .lock()
                                    .await
                                    .push_back(StreamChunk::Error(message));
                                sess.notify.notify_one();
                                break;
                            }
                            Err(e) => {
                                sess.pending
                                    .lock()
                                    .await
                                    .push_back(StreamChunk::Error(e.to_string()));
                                sess.notify.notify_one();
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    sess.pending
                        .lock()
                        .await
                        .push_back(StreamChunk::Error(e.to_string()));
                    sess.notify.notify_one();
                }
            }
            sess.done.store(true, Ordering::SeqCst);
        });

        *session.task.lock().await = Some(task);
        (true, String::new())
    }

    /// Returns (success, chunks: Vec<(chunk_type, data)>, error).
    /// chunk_type: "delta", "tool_calls", "done", "error".
    async fn receive_stream(
        &self,
        #[zbus(header)] header: Header<'_>,
        stream_id: String,
        timeout_ms: u64,
    ) -> (bool, Vec<(String, String)>, String) {
        if !check_caller_access(&header).await {
            return (false, Vec::new(), "access denied".into());
        }
        if stream_id.is_empty() {
            return (false, Vec::new(), "stream_id is required".into());
        }
        let session = {
            let reg = SESSIONS.lock().await;
            match reg.get(&stream_id) {
                Some(s) => Arc::clone(s),
                None => return (false, Vec::new(), "stream_id not found".into()),
            }
        };

        if let Some(chunk) = session.pending.lock().await.pop_front() {
            return (true, vec![encode_chunk(&chunk)], String::new());
        }

        let tm = if timeout_ms == 0 { 1000 } else { timeout_ms };
        let _ = tokio::time::timeout(
            std::time::Duration::from_millis(tm),
            session.notify.notified(),
        )
        .await;

        let mut chunks = Vec::new();
        let mut q = session.pending.lock().await;
        while let Some(chunk) = q.pop_front() {
            chunks.push(encode_chunk(&chunk));
        }
        if chunks.is_empty() && session.done.load(Ordering::SeqCst) {
            chunks.push(("done".into(), String::new()));
        }
        (true, chunks, String::new())
    }

    /// Returns (success, error).
    async fn close_stream(
        &self,
        #[zbus(header)] header: Header<'_>,
        stream_id: String,
    ) -> (bool, String) {
        if !check_caller_access(&header).await {
            return (false, "access denied".into());
        }
        if stream_id.is_empty() {
            return (false, "stream_id is required".into());
        }
        let session = SESSIONS.lock().await.remove(&stream_id);
        if let Some(s) = session {
            // Signal the task to stop gracefully.
            s.done.store(true, Ordering::SeqCst);
            s.notify.notify_one();
            // Wait up to 5 seconds for the task to finish.
            if let Some(task) = s.task.lock().await.take() {
                let _ = tokio::time::timeout(std::time::Duration::from_secs(5), task).await;
            }
        }
        (true, String::new())
    }

    // -----------------------------------------------------------------------
    // Capability: embed
    // -----------------------------------------------------------------------

    /// Returns (success, embeddings: Vec<Vec<f64>>, prompt_tokens, total_tokens, error).
    async fn embed(
        &self,
        #[zbus(header)] header: Header<'_>,
        model_id: String,
        inputs: Vec<String>,
    ) -> (bool, Vec<Vec<f64>>, u32, u32, String) {
        if !check_caller_access(&header).await {
            return (false, Vec::new(), 0, 0, "access denied".into());
        }
        if model_id.is_empty() || inputs.is_empty() {
            return (
                false,
                Vec::new(),
                0,
                0,
                "model_id and inputs are required".into(),
            );
        }
        let (provider_id, real_model) = match route::resolve_alias(&model_id).await {
            Ok(p) => p,
            Err(e) => return (false, Vec::new(), 0, 0, e.to_string()),
        };
        let provider = match pconfig::load_provider(&provider_id).await {
            Ok(p) => p,
            Err(e) => return (false, Vec::new(), 0, 0, e.to_string()),
        };
        let request = EmbeddingRequest {
            model: real_model,
            input: inputs,
            encoding_format: None,
        };
        match provider.embed_dyn("ipc", &request).await {
            Ok(resp) => {
                let embeddings: Vec<Vec<f64>> =
                    resp.data.into_iter().map(|e| e.embedding).collect();
                let (pt, tt) = resp
                    .usage
                    .map(|u| (u.prompt_tokens, u.total_tokens))
                    .unwrap_or((0, 0));
                (true, embeddings, pt, tt, String::new())
            }
            Err(e) => (false, Vec::new(), 0, 0, e.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// Stream chunk encoding helper
// ---------------------------------------------------------------------------

fn encode_chunk(chunk: &StreamChunk) -> (String, String) {
    match chunk {
        StreamChunk::Delta(s) => ("delta".into(), s.clone()),
        StreamChunk::ToolCalls(tcs) => {
            // Encode tool calls as pipe-separated "id|name|args" entries.
            let data: Vec<String> = tcs
                .iter()
                .map(|tc| format!("{}|{}|{}", tc.id, tc.function.name, tc.function.arguments))
                .collect();
            ("tool_calls".into(), data.join("\n"))
        }
        StreamChunk::Done { finish_reason, .. } => {
            ("done".into(), finish_reason.clone().unwrap_or_default())
        }
        StreamChunk::Error(msg) => ("error".into(), msg.clone()),
    }
}
