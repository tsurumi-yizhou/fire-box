//! CONTROL protocol handlers — provider management, routing, metrics, allowlist.

use std::collections::HashMap;
use std::sync::LazyLock;

use tokio::sync::Mutex;
use xpc_connection_sys::xpc_object_t;

use crate::middleware::access;
use crate::middleware::{config, metrics, route};
use crate::providers::config as pconfig;

use super::codec::*;
use super::connections::ConnectionRegistry;

// ---------------------------------------------------------------------------
// Pending OAuth state (for device-flow providers)
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
// ping
// ---------------------------------------------------------------------------

pub fn handle_ping() -> xpc_object_t {
    unsafe { response_ok(dict_new()) }
}

// ---------------------------------------------------------------------------
// Provider management
// ---------------------------------------------------------------------------

pub async fn handle_add_api_key_provider(req: xpc_object_t) -> xpc_object_t {
    let (name, ptype, api_key, base_url) = unsafe {
        (
            dict_get_str(req, "name").unwrap_or_default(),
            dict_get_str(req, "provider_type").unwrap_or_default(),
            dict_get_str(req, "api_key").unwrap_or_default(),
            dict_get_str(req, "base_url"),
        )
    };

    if name.is_empty() || ptype.is_empty() {
        return unsafe { response_err("name and provider_type are required") };
    }

    let cfg = match ptype.as_str() {
        "openai" => pconfig::ProviderConfig::openai(&api_key, base_url),
        "anthropic" => pconfig::ProviderConfig::anthropic(&api_key, base_url),
        "ollama" => pconfig::ProviderConfig::ollama(base_url),
        other => return unsafe { response_err(&format!("unsupported provider_type: {other}")) },
    };

    let profile_id = name.to_lowercase().replace(' ', "_");

    match pconfig::configure_provider(&profile_id, &cfg).await {
        Ok(_) => {
            if let Err(e) = pconfig::add_to_provider_index(&profile_id).await {
                if let Err(e) = pconfig::remove_provider(&profile_id).await {
                    tracing::warn!("Cleanup: failed to remove provider {profile_id}: {e}");
                }
                return unsafe { response_err(&format!("failed to update provider index: {e}")) };
            }
            let pid = profile_id.clone();
            let nm = name.clone();
            if let Err(e) = config::update_config(move |d| {
                d.display_names.insert(pid, nm);
            })
            .await
            {
                if let Err(e) = pconfig::remove_from_provider_index(&profile_id).await {
                    tracing::warn!(
                        "Cleanup: failed to remove provider {profile_id} from index: {e}"
                    );
                }
                if let Err(e) = pconfig::remove_provider(&profile_id).await {
                    tracing::warn!("Cleanup: failed to remove provider {profile_id}: {e}");
                }
                return unsafe {
                    response_err(&format!("failed to persist provider metadata: {e}"))
                };
            }
            unsafe {
                let body = dict_new();
                dict_set_str(body, "provider_id", &profile_id);
                response_ok(body)
            }
        }
        Err(e) => unsafe { response_err(&e.to_string()) },
    }
}

pub async fn handle_add_oauth_provider(req: xpc_object_t) -> xpc_object_t {
    let (name, ptype) = unsafe {
        (
            dict_get_str(req, "name").unwrap_or_default(),
            dict_get_str(req, "provider_type").unwrap_or_default(),
        )
    };

    if name.is_empty() || ptype.is_empty() {
        return unsafe { response_err("name and provider_type are required") };
    }

    match ptype.as_str() {
        "copilot" => {
            use crate::providers::copilot::CopilotProvider;
            match CopilotProvider::start_device_flow(None).await {
                Ok(dc) => {
                    let profile_id = name.to_lowercase().replace(' ', "_");
                    let cfg = pconfig::ProviderConfig::copilot_pending(None);
                    if let Err(e) = pconfig::configure_provider(&profile_id, &cfg).await {
                        return unsafe {
                            response_err(&format!("failed to persist pending provider config: {e}"))
                        };
                    }
                    if let Err(e) = pconfig::add_to_provider_index(&profile_id).await {
                        if let Err(e) = pconfig::remove_provider(&profile_id).await {
                            tracing::warn!("Cleanup: failed to remove provider {profile_id}: {e}");
                        }
                        return unsafe {
                            response_err(&format!("failed to update provider index: {e}"))
                        };
                    }
                    let pid = profile_id.clone();
                    let nm = name.clone();
                    if let Err(e) = config::update_config(move |d| {
                        d.display_names.insert(pid, nm);
                    })
                    .await
                    {
                        if let Err(e) = pconfig::remove_from_provider_index(&profile_id).await {
                            tracing::warn!(
                                "Cleanup: failed to remove provider {profile_id} from index: {e}"
                            );
                        }
                        if let Err(e) = pconfig::remove_provider(&profile_id).await {
                            tracing::warn!("Cleanup: failed to remove provider {profile_id}: {e}");
                        }
                        return unsafe {
                            response_err(&format!("failed to persist provider metadata: {e}"))
                        };
                    }

                    PENDING_OAUTH.lock().await.insert(
                        profile_id.clone(),
                        PendingOAuth::Copilot {
                            device_code: dc.device_code.clone(),
                            interval: dc.interval,
                            expires_in: dc.expires_in,
                        },
                    );

                    unsafe {
                        let body = dict_new();
                        let challenge = dict_new();
                        dict_set_str(body, "provider_id", &profile_id);
                        dict_set_str(challenge, "user_code", &dc.user_code);
                        dict_set_str(challenge, "verification_uri", &dc.verification_uri);
                        dict_set_str(challenge, "device_code", &dc.device_code);
                        dict_set_i64(challenge, "expires_in", dc.expires_in as i64);
                        dict_set_i64(challenge, "interval", dc.interval as i64);
                        dict_set_obj(body, "challenge", challenge);
                        response_ok(body)
                    }
                }
                Err(e) => unsafe { response_err(&e.to_string()) },
            }
        }
        "dashscope" => {
            use crate::providers::dashscope::QwenOAuthFlow;
            match QwenOAuthFlow::start(None).await {
                Ok(flow) => {
                    let dc = flow.device_code_response().clone();
                    let profile_id = name.to_lowercase().replace(' ', "_");

                    let cfg = pconfig::ProviderConfig::DashScope(pconfig::DashScopeConfig {
                        access_token: None,
                        refresh_token: None,
                        resource_url: None,
                        expiry_date: None,
                        base_url: None,
                    });
                    if let Err(e) = pconfig::configure_provider(&profile_id, &cfg).await {
                        return unsafe {
                            response_err(&format!("failed to persist pending provider config: {e}"))
                        };
                    }
                    if let Err(e) = pconfig::add_to_provider_index(&profile_id).await {
                        if let Err(e) = pconfig::remove_provider(&profile_id).await {
                            tracing::warn!("Cleanup: failed to remove provider {profile_id}: {e}");
                        }
                        return unsafe {
                            response_err(&format!("failed to update provider index: {e}"))
                        };
                    }
                    let pid = profile_id.clone();
                    let nm = name.clone();
                    if let Err(e) = config::update_config(move |d| {
                        d.display_names.insert(pid, nm);
                    })
                    .await
                    {
                        if let Err(e) = pconfig::remove_from_provider_index(&profile_id).await {
                            tracing::warn!(
                                "Cleanup: failed to remove provider {profile_id} from index: {e}"
                            );
                        }
                        if let Err(e) = pconfig::remove_provider(&profile_id).await {
                            tracing::warn!("Cleanup: failed to remove provider {profile_id}: {e}");
                        }
                        return unsafe {
                            response_err(&format!("failed to persist provider metadata: {e}"))
                        };
                    }

                    PENDING_OAUTH
                        .lock()
                        .await
                        .insert(profile_id.clone(), PendingOAuth::DashScope { flow });

                    unsafe {
                        let body = dict_new();
                        let challenge = dict_new();
                        dict_set_str(body, "provider_id", &profile_id);
                        dict_set_str(challenge, "user_code", &dc.user_code);
                        dict_set_str(challenge, "verification_uri", &dc.verification_uri);
                        dict_set_str(
                            challenge,
                            "verification_uri_complete",
                            &dc.verification_uri_complete,
                        );
                        dict_set_str(challenge, "device_code", &dc.device_code);
                        dict_set_i64(challenge, "expires_in", dc.expires_in as i64);
                        dict_set_i64(challenge, "interval", dc.interval as i64);
                        dict_set_obj(body, "challenge", challenge);
                        response_ok(body)
                    }
                }
                Err(e) => unsafe { response_err(&e.to_string()) },
            }
        }
        other => unsafe { response_err(&format!("unsupported OAuth provider_type: {other}")) },
    }
}

pub async fn handle_complete_oauth(req: xpc_object_t) -> xpc_object_t {
    let provider_id = unsafe { dict_get_str(req, "provider_id").unwrap_or_default() };
    if provider_id.is_empty() {
        return unsafe { response_err("provider_id is required") };
    }

    let pending = PENDING_OAUTH.lock().await.remove(&provider_id);
    let Some(state) = pending else {
        return unsafe { response_err("no pending OAuth flow for this provider_id") };
    };

    match state {
        PendingOAuth::Copilot {
            device_code,
            interval,
            expires_in,
        } => {
            use crate::providers::copilot::CopilotProvider;
            match CopilotProvider::poll_for_token(None, &device_code, interval, expires_in).await {
                Ok(token) => {
                    let cfg = pconfig::ProviderConfig::copilot(&token, None);
                    if let Err(e) = pconfig::configure_provider(&provider_id, &cfg).await {
                        return unsafe {
                            response_err(&format!("failed to persist OAuth credentials: {e}"))
                        };
                    }
                    unsafe {
                        let body = dict_new();
                        let creds = dict_new();
                        dict_set_str(creds, "token_type", "bearer");
                        dict_set_obj(body, "credentials", creds);
                        response_ok(body)
                    }
                }
                Err(e) => unsafe { response_err(&e.to_string()) },
            }
        }
        PendingOAuth::DashScope { flow } => {
            match tokio::time::timeout(
                std::time::Duration::from_secs(
                    crate::providers::consts::OAUTH_DEVICE_FLOW_TIMEOUT_SECS,
                ),
                flow.wait_for_token(),
            )
            .await
            {
                Ok(Ok(creds)) => {
                    let cfg = pconfig::ProviderConfig::dashscope_oauth(
                        creds.access_token.clone(),
                        creds.refresh_token.clone().unwrap_or_default(),
                        creds.expiry_date.unwrap_or(0),
                        creds.resource_url.clone(),
                    );
                    if let Err(e) = pconfig::configure_provider(&provider_id, &cfg).await {
                        return unsafe {
                            response_err(&format!("failed to persist OAuth credentials: {e}"))
                        };
                    }
                    unsafe {
                        let body = dict_new();
                        let cr = dict_new();
                        dict_set_str(cr, "access_token", &creds.access_token);
                        dict_set_obj(body, "credentials", cr);
                        response_ok(body)
                    }
                }
                Ok(Err(e)) => unsafe { response_err(&e.to_string()) },
                Err(_timeout) => unsafe { response_err("OAuth flow timed out") },
            }
        }
    }
}

pub async fn handle_add_local_provider(req: xpc_object_t) -> xpc_object_t {
    let (name, model_path) = unsafe {
        (
            dict_get_str(req, "name").unwrap_or_default(),
            dict_get_str(req, "model_path").unwrap_or_default(),
        )
    };

    if name.is_empty() || model_path.is_empty() {
        return unsafe { response_err("name and model_path are required") };
    }

    let profile_id = name.to_lowercase().replace(' ', "_");
    let cfg = pconfig::ProviderConfig::llamacpp(&model_path);

    match pconfig::configure_provider(&profile_id, &cfg).await {
        Ok(_) => {
            if let Err(e) = pconfig::add_to_provider_index(&profile_id).await {
                if let Err(e) = pconfig::remove_provider(&profile_id).await {
                    tracing::warn!("Cleanup: failed to remove provider {profile_id}: {e}");
                }
                return unsafe { response_err(&format!("failed to update provider index: {e}")) };
            }
            let pid = profile_id.clone();
            let nm = name.clone();
            if let Err(e) = config::update_config(move |d| {
                d.display_names.insert(pid, nm);
            })
            .await
            {
                if let Err(e) = pconfig::remove_from_provider_index(&profile_id).await {
                    tracing::warn!(
                        "Cleanup: failed to remove provider {profile_id} from index: {e}"
                    );
                }
                if let Err(e) = pconfig::remove_provider(&profile_id).await {
                    tracing::warn!("Cleanup: failed to remove provider {profile_id}: {e}");
                }
                return unsafe {
                    response_err(&format!("failed to persist provider metadata: {e}"))
                };
            }
            unsafe {
                let body = dict_new();
                dict_set_str(body, "provider_id", &profile_id);
                response_ok(body)
            }
        }
        Err(e) => unsafe { response_err(&e.to_string()) },
    }
}

pub async fn handle_list_providers() -> xpc_object_t {
    let index = pconfig::load_provider_index().await;
    let cfg_data = match config::load_config().await {
        Ok(v) => v,
        Err(e) => return unsafe { response_err(&format!("failed to load config: {e}")) },
    };

    unsafe {
        let body = dict_new();
        let arr = array_new();

        for profile_id in &index {
            let json_val: Option<serde_json::Value> = cfg_data
                .providers
                .get(profile_id)
                .and_then(|j| serde_json::from_str(j).ok());

            let type_slug = json_val
                .as_ref()
                .and_then(|v| v.get("provider").and_then(|p| p.as_str()))
                .unwrap_or("unknown")
                .to_string();

            let display_name = cfg_data
                .display_names
                .get(profile_id)
                .cloned()
                .unwrap_or_else(|| profile_id.clone());

            let base_url = json_val
                .as_ref()
                .and_then(|v| v.get("base_url").and_then(|u| u.as_str()))
                .unwrap_or("")
                .to_string();

            let provider_type: i64 = match type_slug.as_str() {
                "copilot" | "dashscope" | "dash_scope" => 2,
                "llama_cpp" | "llamacpp" => 3,
                "openai" | "open_ai" | "anthropic" | "ollama" | "vllm" => 1,
                _ => 1,
            };

            let entry = dict_new();
            dict_set_str(entry, "provider_id", profile_id);
            dict_set_str(entry, "name", &display_name);
            dict_set_i64(entry, "type", provider_type);
            dict_set_str(entry, "base_url", &base_url);
            array_append(arr, entry);
        }

        dict_set_obj(body, "providers", arr);
        response_ok(body)
    }
}

pub async fn handle_delete_provider(req: xpc_object_t) -> xpc_object_t {
    let provider_id = unsafe { dict_get_str(req, "provider_id").unwrap_or_default() };
    if provider_id.is_empty() {
        return unsafe { response_err("provider_id is required") };
    }
    if let Err(e) = pconfig::remove_provider(&provider_id).await {
        return unsafe { response_err(&format!("failed to remove provider config: {e}")) };
    }
    if let Err(e) = pconfig::remove_from_provider_index(&provider_id).await {
        return unsafe { response_err(&format!("failed to remove provider from index: {e}")) };
    }
    unsafe { response_ok(dict_new()) }
}

// ---------------------------------------------------------------------------
// Model management
// ---------------------------------------------------------------------------

pub async fn handle_get_all_models(req: xpc_object_t) -> xpc_object_t {
    let filter_provider = unsafe { dict_get_str(req, "provider_id") };

    let index = pconfig::load_provider_index().await;
    let mut models_out: Vec<(String, String, String, bool, Option<u32>)> = Vec::new();

    for profile_id in &index {
        if let Some(ref fp) = filter_provider {
            if profile_id != fp {
                continue;
            }
        }
        let provider = match pconfig::load_provider(profile_id).await {
            Ok(p) => p,
            Err(_) => continue,
        };
        let models = provider.list_models_dyn().await.unwrap_or_default();
        for m in models {
            let enabled = route::is_model_enabled(profile_id, &m.id).await;
            models_out.push((profile_id.clone(), m.id, m.owner, enabled, m.context_window));
        }
    }

    unsafe {
        let body = dict_new();
        let arr = array_new();
        for (pid, mid, owner, enabled, cw) in &models_out {
            let entry = dict_new();
            dict_set_str(entry, "provider_id", pid);
            dict_set_str(entry, "model_id", mid);
            dict_set_str(entry, "owner", owner);
            dict_set_bool(entry, "enabled", *enabled);
            if let Some(c) = cw {
                dict_set_i64(entry, "context_window", *c as i64);
            }
            array_append(arr, entry);
        }
        dict_set_obj(body, "models", arr);
        response_ok(body)
    }
}

pub async fn handle_set_model_enabled(req: xpc_object_t) -> xpc_object_t {
    let (provider_id, model_id, enabled) = unsafe {
        (
            dict_get_str(req, "provider_id").unwrap_or_default(),
            dict_get_str(req, "model_id").unwrap_or_default(),
            dict_get_bool(req, "enabled").unwrap_or(true),
        )
    };

    if provider_id.is_empty() || model_id.is_empty() {
        return unsafe { response_err("provider_id and model_id are required") };
    }

    let provider = match pconfig::load_provider(&provider_id).await {
        Ok(p) => p,
        Err(e) => return unsafe { response_err(&e.to_string()) },
    };
    let all_models: Vec<String> = provider
        .list_models_dyn()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|m| m.id)
        .collect();

    match route::toggle_model(&provider_id, &model_id, enabled, &all_models).await {
        Ok(_) => unsafe { response_ok(dict_new()) },
        Err(e) => unsafe { response_err(&e.to_string()) },
    }
}

// ---------------------------------------------------------------------------
// Route rules
// ---------------------------------------------------------------------------

pub async fn handle_set_route_rules(req: xpc_object_t) -> xpc_object_t {
    let (virtual_model_id, display_name, strategy_str) = unsafe {
        let vmid = dict_get_str(req, "virtual_model_id").unwrap_or_default();
        let dn = dict_get_str(req, "display_name").unwrap_or_else(|| vmid.clone());
        let strat = dict_get_str(req, "strategy").unwrap_or_default();
        (vmid, dn, strat)
    };

    if virtual_model_id.is_empty() {
        return unsafe { response_err("virtual_model_id is required") };
    }

    let strategy = match strategy_str.as_str() {
        "random" => route::RouteStrategy::Random,
        _ => route::RouteStrategy::Failover,
    };

    let (capabilities, metadata, targets) = unsafe {
        let caps = if let Some(co) = dict_get_obj(req, "capabilities") {
            route::RouteCapabilities {
                chat: dict_get_bool(co, "chat").unwrap_or(true),
                streaming: dict_get_bool(co, "streaming").unwrap_or(true),
                embeddings: dict_get_bool(co, "embeddings").unwrap_or(false),
                vision: dict_get_bool(co, "vision").unwrap_or(false),
                tool_calling: dict_get_bool(co, "tool_calling").unwrap_or(false),
            }
        } else {
            route::RouteCapabilities::default()
        };

        let meta = if let Some(mo) = dict_get_obj(req, "metadata") {
            route::RouteMetadata {
                context_window: dict_get_i64(mo, "context_window").map(|v| v as u32),
                pricing_tier: dict_get_str(mo, "pricing_tier"),
                strengths: Vec::new(),
                description: dict_get_str(mo, "description"),
            }
        } else {
            route::RouteMetadata::default()
        };

        let mut tgts = Vec::new();
        if let Some(arr) = dict_get_obj(req, "targets") {
            let count = array_len(arr);
            for i in 0..count {
                if let Some(entry) = array_get(arr, i) {
                    let pid = dict_get_str(entry, "provider_id").unwrap_or_default();
                    let mid = dict_get_str(entry, "model_id").unwrap_or_default();
                    if !pid.is_empty() && !mid.is_empty() {
                        tgts.push(route::RouteTarget {
                            provider_id: pid,
                            model_id: mid,
                        });
                    }
                }
            }
        }
        (caps, meta, tgts)
    };

    match route::set_route_rules_with_options(
        &virtual_model_id,
        &display_name,
        capabilities,
        metadata,
        targets,
        strategy,
    )
    .await
    {
        Ok(_) => unsafe { response_ok(dict_new()) },
        Err(e) => unsafe { response_err(&e.to_string()) },
    }
}

pub async fn handle_get_route_rules(req: xpc_object_t) -> xpc_object_t {
    let virtual_model_id = unsafe { dict_get_str(req, "virtual_model_id") };

    match virtual_model_id {
        Some(ref id) if !id.is_empty() => match route::get_route_rules(id).await {
            Ok(Some(rule)) => unsafe {
                let body = dict_new();
                let rule_obj = encode_route_rule(&rule);
                dict_set_obj(body, "rule", rule_obj);
                response_ok(body)
            },
            Ok(None) => unsafe { response_err(&format!("route '{}' not found", id)) },
            Err(e) => unsafe { response_err(&e.to_string()) },
        },
        _ => match route::get_all_rules().await {
            Ok(rules) => unsafe {
                let body = dict_new();
                let arr = array_new();
                for rule in &rules {
                    array_append(arr, encode_route_rule(rule));
                }
                dict_set_obj(body, "rules", arr);
                response_ok(body)
            },
            Err(e) => unsafe { response_err(&e.to_string()) },
        },
    }
}

unsafe fn encode_route_rule(rule: &route::RouteRule) -> xpc_object_t {
    let entry = dict_new();
    dict_set_str(entry, "virtual_model_id", &rule.virtual_model_id);
    dict_set_str(entry, "display_name", &rule.display_name);
    dict_set_str(
        entry,
        "strategy",
        match rule.strategy {
            route::RouteStrategy::Random => "random",
            route::RouteStrategy::Failover => "failover",
        },
    );

    let targets_arr = array_new();
    for t in &rule.targets {
        let te = dict_new();
        dict_set_str(te, "provider_id", &t.provider_id);
        dict_set_str(te, "model_id", &t.model_id);
        array_append(targets_arr, te);
    }
    dict_set_obj(entry, "targets", targets_arr);

    let caps = dict_new();
    dict_set_bool(caps, "chat", rule.capabilities.chat);
    dict_set_bool(caps, "streaming", rule.capabilities.streaming);
    dict_set_bool(caps, "embeddings", rule.capabilities.embeddings);
    dict_set_bool(caps, "vision", rule.capabilities.vision);
    dict_set_bool(caps, "tool_calling", rule.capabilities.tool_calling);
    dict_set_obj(entry, "capabilities", caps);

    entry
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

pub fn handle_get_metrics_snapshot() -> xpc_object_t {
    let snap = metrics::get_snapshot();
    unsafe {
        let body = dict_new();
        let snap_obj = dict_new();
        dict_set_i64(snap_obj, "window_start_ms", snap.window_start_ms as i64);
        dict_set_i64(snap_obj, "window_end_ms", snap.window_end_ms as i64);
        dict_set_i64(snap_obj, "requests_total", snap.requests_total as i64);
        dict_set_i64(snap_obj, "requests_failed", snap.requests_failed as i64);
        dict_set_i64(
            snap_obj,
            "prompt_tokens_total",
            snap.prompt_tokens_total as i64,
        );
        dict_set_i64(
            snap_obj,
            "completion_tokens_total",
            snap.completion_tokens_total as i64,
        );
        dict_set_f64(snap_obj, "cost_total", snap.cost_total);
        dict_set_obj(body, "snapshot", snap_obj);
        response_ok(body)
    }
}

pub async fn handle_get_metrics_range(req: xpc_object_t) -> xpc_object_t {
    let (start_ms, end_ms) = unsafe {
        let s = dict_get_i64(req, "start_ms").unwrap_or(0) as u64;
        let e = dict_get_i64(req, "end_ms").unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64
        }) as u64;
        (s, e)
    };

    let buckets = metrics::get_metrics_range(start_ms, end_ms).await;
    unsafe {
        let body = dict_new();
        let arr = array_new();
        for b in &buckets {
            let bobj = dict_new();
            dict_set_i64(bobj, "timestamp_ms", b.timestamp_ms as i64);
            dict_set_i64(bobj, "requests_total", b.requests_total as i64);
            dict_set_i64(bobj, "requests_failed", b.requests_failed as i64);
            dict_set_i64(bobj, "prompt_tokens", b.prompt_tokens as i64);
            dict_set_i64(bobj, "completion_tokens", b.completion_tokens as i64);
            dict_set_f64(bobj, "cost_total", b.cost_total);
            array_append(arr, bobj);
        }
        dict_set_obj(body, "snapshots", arr);
        response_ok(body)
    }
}

// ---------------------------------------------------------------------------
// Connections + allowlist
// ---------------------------------------------------------------------------

pub fn handle_list_connections(registry: &ConnectionRegistry) -> xpc_object_t {
    let conns = registry.list();
    unsafe {
        let body = dict_new();
        let arr = array_new();
        for c in &conns {
            let entry = dict_new();
            dict_set_str(entry, "connection_id", &c.connection_id);
            dict_set_str(entry, "client_name", &c.client_name);
            dict_set_str(entry, "app_path", &c.app_path);
            dict_set_i64(entry, "requests_count", c.requests_count as i64);
            dict_set_i64(entry, "connected_at_ms", c.connected_at_ms as i64);
            array_append(arr, entry);
        }
        dict_set_obj(body, "connections", arr);
        response_ok(body)
    }
}

pub async fn handle_get_allowlist() -> xpc_object_t {
    let entries = access::get_allowlist().await;
    unsafe {
        let body = dict_new();
        let arr = array_new();
        for e in &entries {
            let obj = dict_new();
            dict_set_str(obj, "app_path", &e.app_path);
            dict_set_str(obj, "display_name", &e.display_name);
            array_append(arr, obj);
        }
        dict_set_obj(body, "apps", arr);
        response_ok(body)
    }
}

pub async fn handle_remove_from_allowlist(req: xpc_object_t) -> xpc_object_t {
    let app_path = unsafe { dict_get_str(req, "app_path").unwrap_or_default() };
    if app_path.is_empty() {
        return unsafe { response_err("app_path is required") };
    }
    match access::remove_from_allowlist(&app_path).await {
        Ok(_) => unsafe { response_ok(dict_new()) },
        Err(e) => unsafe { response_err(&e.to_string()) },
    }
}
