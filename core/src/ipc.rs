use crate::CoreState;
/// IPC server for communication with the native layer (Swift / C++).
///
/// Runs an Axum HTTP server over an **interprocess local socket**
/// (named pipe on Windows, UDS on Unix), exposing:
///
/// **Commands (Native → Core):**
/// - `POST /ipc/v1/chat`           — forward an app's chat request
/// - `POST /ipc/v1/auth/decide`    — user approval / denial for an app
/// - `GET  /ipc/v1/metrics`        — current metrics snapshot
/// - `GET  /ipc/v1/apps`           — list registered apps
/// - `POST /ipc/v1/apps/{id}/revoke` — revoke an app's access
/// - `GET  /ipc/v1/providers`      — list all providers
/// - `POST /ipc/v1/providers`      — add a new provider
/// - `PUT  /ipc/v1/providers/{tag}` — update a provider
/// - `DELETE /ipc/v1/providers/{tag}` — delete a provider
/// - `GET  /ipc/v1/models`         — list all model mappings
/// - `POST /ipc/v1/models`         — add a model mapping
/// - `DELETE /ipc/v1/models/{tag}` — delete a model mapping
/// - `GET  /ipc/v1/settings`       — get service settings
/// - `PUT  /ipc/v1/settings`       — update service settings
///
/// **Events (Core → Native, SSE):**
/// - `GET  /ipc/v1/events`         — Server-Sent Events stream:
///     - `auth_required`  — an unknown app wants access, show popup
///     - `metrics_update` — periodic metrics push
///     - `request_log`    — a request was processed
use crate::auth::AppInfo;
use crate::metrics::MetricsSnapshot;
use crate::protocol::UnifiedRequest;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::Stream;
use interprocess::local_socket::ListenerOptions;
// Bring the interprocess Listener trait into scope for `.accept()`.
use interprocess::local_socket::traits::tokio::Listener as _;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tracing::{error, info, warn};

// ─── IPC event types (Core → Native via SSE) ───────────────────────────────

/// Events pushed from Core to the native layer over SSE.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IpcEvent {
    /// An unknown app wants access; native should show an approval popup.
    #[serde(rename = "auth_required")]
    AuthRequired {
        request_id: String,
        app_id: String,
        app_name: String,
        requested_models: Vec<String>,
    },
    /// Periodic metrics update pushed to native GUI.
    #[serde(rename = "metrics_update")]
    MetricsUpdate { metrics: MetricsSnapshot },
    /// A request was completed (for activity log).
    #[serde(rename = "request_log")]
    RequestLog {
        app_id: Option<String>,
        model: String,
        provider: String,
        input_tokens: u64,
        output_tokens: u64,
        success: bool,
    },
    /// An OAuth device-code flow needs user interaction: the native layer
    /// should show a notification with the URL so the user can open it in a
    /// browser to authorise.
    #[serde(rename = "oauth_open_url")]
    OAuthOpenUrl {
        provider: String,
        url: String,
        user_code: String,
    },
}

// ─── IPC request / response bodies ─────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct IpcChatRequest {
    pub app_id: String,
    pub app_name: String,
    pub request: UnifiedRequest,
}

#[derive(Debug, Serialize)]
pub struct IpcChatResponse {
    pub text: String,
    pub model: String,
    pub provider: String,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct AuthDecisionRequest {
    pub app_id: String,
    pub approved: bool,
    #[serde(default)]
    pub allowed_models: Vec<String>,
}

// ─── Configuration management request / response bodies ────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct AddProviderRequest {
    pub tag: String,
    #[serde(rename = "type")]
    pub provider_type: crate::keystore::ProviderType,
    pub base_url: Option<String>,
    pub oauth_creds_path: Option<String>,
    /// API key or auth token (will be stored in keyring).
    pub credential: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateProviderRequest {
    pub base_url: Option<String>,
    pub oauth_creds_path: Option<String>,
    /// New credential (if provided, will update keyring).
    pub credential: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProviderListResponse {
    pub providers: Vec<crate::keystore::ProviderInfo>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AddModelRequest {
    pub tag: String,
    pub provider_mappings: Vec<crate::keystore::ProviderMapping>,
}

#[derive(Debug, Serialize)]
pub struct ModelListResponse {
    pub models: std::collections::HashMap<String, Vec<crate::keystore::ProviderMapping>>,
}

#[derive(Debug, Serialize)]
pub struct SettingsResponse {
    pub settings: crate::keystore::ServiceSettings,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSettingsRequest {
    pub log_level: Option<String>,
    pub ipc_pipe: Option<String>,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

// ─── Launch IPC server ──────────────────────────────────────────────────────

/// Create the interprocess local socket listener for the given pipe name.
///
/// On Windows: creates a named pipe via `GenericNamespaced`.
/// On Unix: creates a UDS at the specified path via `GenericFilePath`.
fn create_local_listener(
    pipe_name: &str,
) -> anyhow::Result<interprocess::local_socket::tokio::Listener> {
    #[cfg(windows)]
    {
        use interprocess::local_socket::GenericNamespaced;
        use interprocess::local_socket::tokio::prelude::*;
        let name = pipe_name.to_ns_name::<GenericNamespaced>()?;
        Ok(ListenerOptions::new().name(name).create_tokio()?)
    }
    #[cfg(unix)]
    {
        use interprocess::local_socket::GenericFilePath;
        use interprocess::local_socket::tokio::prelude::*;
        let path = shellexpand::tilde(pipe_name).to_string();
        // Remove stale socket file if present.
        let _ = std::fs::remove_file(&path);
        let name = path.to_fs_name::<GenericFilePath>()?;
        Ok(ListenerOptions::new().name(name).create_tokio()?)
    }
}

/// Wrapper that implements [`axum::serve::Listener`] around an interprocess
/// local socket, so we can serve Axum routes over a named pipe / UDS.
struct LocalSocketListener {
    inner: interprocess::local_socket::tokio::Listener,
}

impl axum::serve::Listener for LocalSocketListener {
    type Io = interprocess::local_socket::tokio::Stream;
    type Addr = ();

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match self.inner.accept().await {
                Ok(stream) => return (stream, ()),
                Err(e) => {
                    warn!(error = %e, "IPC local socket accept error, retrying");
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        Ok(())
    }
}

/// Start the IPC Axum server on the configured local socket.
pub fn launch(state: &CoreState) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let pipe_name = tokio::task::block_in_place(|| {
        let config = state.config.blocking_read();
        config.settings.ipc_pipe.clone()
    });
    let state = state.clone();

    let router = Router::new()
        // Chat & auth
        .route("/ipc/v1/chat", post(handle_chat))
        .route("/ipc/v1/auth/decide", post(handle_auth_decide))
        .route("/ipc/v1/metrics", get(handle_metrics))
        .route("/ipc/v1/apps", get(handle_list_apps))
        .route("/ipc/v1/apps/{app_id}/revoke", post(handle_revoke_app))
        .route("/ipc/v1/events", get(handle_events))
        // Configuration management
        .route("/ipc/v1/providers", get(handle_list_providers))
        .route("/ipc/v1/providers", post(handle_add_provider))
        .route(
            "/ipc/v1/providers/{tag}",
            axum::routing::put(handle_update_provider),
        )
        .route(
            "/ipc/v1/providers/{tag}",
            axum::routing::delete(handle_delete_provider),
        )
        .route("/ipc/v1/models", get(handle_list_models))
        .route("/ipc/v1/models", post(handle_add_model))
        .route(
            "/ipc/v1/models/{tag}",
            axum::routing::delete(handle_delete_model),
        )
        .route("/ipc/v1/settings", get(handle_get_settings))
        .route(
            "/ipc/v1/settings",
            axum::routing::put(handle_update_settings),
        )
        .with_state(state);

    let handle = tokio::spawn(async move {
        info!(pipe = %pipe_name, "Starting IPC server on local socket");
        match create_local_listener(&pipe_name) {
            Ok(listener) => {
                let local_listener = LocalSocketListener { inner: listener };
                let _ = axum::serve(local_listener, router.into_make_service()).await;
            }
            Err(e) => error!(pipe = %pipe_name, error = %e, "Failed to create IPC local socket"),
        }
    });

    Ok(handle)
}

// ─── Handler: chat request ──────────────────────────────────────────────────

async fn handle_chat(State(state): State<CoreState>, Json(body): Json<IpcChatRequest>) -> Response {
    let app_id = &body.app_id;
    let app_name = &body.app_name;
    let model = &body.request.model;

    // 1. Check authorization.
    if !state.auth_manager.is_authorized(app_id).await {
        // If the app is completely unknown, register as pending and emit event.
        if !state.auth_manager.is_pending(app_id).await {
            state.auth_manager.register_pending(app_id, app_name).await;
        }

        // Emit auth_required event to native layer.
        let evt = IpcEvent::AuthRequired {
            request_id: uuid::Uuid::new_v4().to_string(),
            app_id: app_id.clone(),
            app_name: app_name.clone(),
            requested_models: vec![model.clone()],
        };
        let _ = state.ipc_event_tx.send(evt);

        return (
            StatusCode::FORBIDDEN,
            Json(ErrorBody {
                error: format!("App '{app_id}' is not authorized. Approval required."),
            }),
        )
            .into_response();
    }

    // 2. Check model-level permission.
    if !state
        .auth_manager
        .is_authorized_for_model(app_id, model)
        .await
    {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorBody {
                error: format!("App '{app_id}' is not allowed to use model '{model}'."),
            }),
        )
            .into_response();
    }

    // 3. Touch usage timestamp.
    state.auth_manager.touch(app_id).await;

    // 4. Find model config.
    let config = state.config.read().await;
    let model_config = match config.models.get(model) {
        Some(m) => m,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorBody {
                    error: format!("Model '{model}' not configured."),
                }),
            )
                .into_response();
        }
    };

    // 5. Try providers with fallback (non-streaming only via IPC for now).
    let mut last_err = String::from("no providers configured");
    for mapping in model_config {
        let provider = match config.providers.iter().find(|p| p.tag == mapping.provider) {
            Some(p) => p,
            None => {
                warn!(provider = %mapping.provider, "Provider not found, skipping");
                continue;
            }
        };

        state
            .metrics
            .record_request(model, &mapping.provider, Some(app_id))
            .await;

        match crate::provider::send_request(
            &state.http,
            provider,
            &mapping.model_id,
            &body.request,
            Some(&state.ipc_event_tx),
        )
        .await
        {
            Ok(text) => {
                // TODO: extract actual token counts from response when available
                state
                    .metrics
                    .record_tokens(model, &mapping.provider, Some(app_id), 0, 0)
                    .await;

                // Emit request log event.
                let _ = state.ipc_event_tx.send(IpcEvent::RequestLog {
                    app_id: Some(app_id.clone()),
                    model: model.clone(),
                    provider: mapping.provider.clone(),
                    input_tokens: 0,
                    output_tokens: 0,
                    success: true,
                });

                return Json(IpcChatResponse {
                    text,
                    model: model.clone(),
                    provider: mapping.provider.clone(),
                    input_tokens: None,
                    output_tokens: None,
                })
                .into_response();
            }
            Err(e) => {
                state
                    .metrics
                    .record_error(model, &mapping.provider, Some(app_id))
                    .await;
                warn!(provider = %mapping.provider, error = %e, "Provider failed, trying next");
                last_err = e.to_string();
            }
        }
    }

    // All providers failed.
    let _ = state.ipc_event_tx.send(IpcEvent::RequestLog {
        app_id: Some(app_id.clone()),
        model: model.clone(),
        provider: String::new(),
        input_tokens: 0,
        output_tokens: 0,
        success: false,
    });

    (
        StatusCode::BAD_GATEWAY,
        Json(ErrorBody {
            error: format!("All providers failed. Last error: {last_err}"),
        }),
    )
        .into_response()
}

// ─── Handler: auth decision ─────────────────────────────────────────────────

async fn handle_auth_decide(
    State(state): State<CoreState>,
    Json(body): Json<AuthDecisionRequest>,
) -> Response {
    if body.approved {
        if state
            .auth_manager
            .approve(&body.app_id, body.allowed_models)
            .await
        {
            info!(app_id = %body.app_id, "App authorized by user");
            StatusCode::OK.into_response()
        } else {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorBody {
                    error: format!("App '{}' not found.", body.app_id),
                }),
            )
                .into_response()
        }
    } else {
        state.auth_manager.revoke(&body.app_id).await;
        info!(app_id = %body.app_id, "App denied by user");
        StatusCode::OK.into_response()
    }
}

// ─── Handler: metrics ───────────────────────────────────────────────────────

async fn handle_metrics(State(state): State<CoreState>) -> Json<MetricsSnapshot> {
    Json(state.metrics.snapshot().await)
}

// ─── Handler: list apps ─────────────────────────────────────────────────────

async fn handle_list_apps(State(state): State<CoreState>) -> Json<Vec<AppInfo>> {
    Json(state.auth_manager.list_apps().await)
}

// ─── Handler: revoke app ────────────────────────────────────────────────────

async fn handle_revoke_app(
    State(state): State<CoreState>,
    axum::extract::Path(app_id): axum::extract::Path<String>,
) -> Response {
    if state.auth_manager.revoke(&app_id).await {
        info!(app_id = %app_id, "App revoked");
        StatusCode::OK.into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorBody {
                error: format!("App '{app_id}' not found."),
            }),
        )
            .into_response()
    }
}

// ─── Handler: SSE events ────────────────────────────────────────────────────

async fn handle_events(
    State(state): State<CoreState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.ipc_event_tx.subscribe();

    // Convert broadcast::Receiver into a Stream via `unfold`.
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(evt) => {
                    let json = match serde_json::to_string(&evt) {
                        Ok(j) => j,
                        Err(_) => continue,
                    };
                    let event_type = match &evt {
                        IpcEvent::AuthRequired { .. } => "auth_required",
                        IpcEvent::MetricsUpdate { .. } => "metrics_update",
                        IpcEvent::RequestLog { .. } => "request_log",
                        IpcEvent::OAuthOpenUrl { .. } => "oauth_open_url",
                    };
                    let event: Result<Event, Infallible> =
                        Ok(Event::default().event(event_type).data(json));
                    return Some((event, rx));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(missed = n, "SSE subscriber lagged, some events dropped");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    return None;
                }
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ─── Configuration Management Handlers ──────────────────────────────────────

async fn handle_list_providers(State(state): State<CoreState>) -> Json<ProviderListResponse> {
    let config = state.config.read().await;
    Json(ProviderListResponse {
        providers: config.providers.clone(),
    })
}

async fn handle_add_provider(
    State(state): State<CoreState>,
    Json(body): Json<AddProviderRequest>,
) -> Response {
    // Store credential in keyring if provided.
    if let Some(cred) = &body.credential {
        let result = match body.provider_type {
            crate::keystore::ProviderType::OpenAI
            | crate::keystore::ProviderType::DashScope
            | crate::keystore::ProviderType::Copilot => {
                crate::keystore::store_provider_key(&body.tag, cred)
            }
            crate::keystore::ProviderType::Anthropic => {
                crate::keystore::store_auth_token(&body.tag, cred)
            }
        };
        if let Err(e) = result {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorBody {
                    error: format!("Failed to store credential: {e}"),
                }),
            )
                .into_response();
        }
    }

    // Add provider to config.
    let provider_info = crate::keystore::ProviderInfo {
        tag: body.tag.clone(),
        provider_type: body.provider_type,
        base_url: body.base_url,
        oauth_creds_path: body.oauth_creds_path,
    };

    let mut config = state.config.write().await;
    config.providers.push(provider_info);

    // Persist to keyring.
    if let Err(e) = config.save_to_keyring() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorBody {
                error: format!("Failed to persist config: {e}"),
            }),
        )
            .into_response();
    }

    info!(tag = %body.tag, "Provider added");
    StatusCode::CREATED.into_response()
}

async fn handle_update_provider(
    State(state): State<CoreState>,
    axum::extract::Path(tag): axum::extract::Path<String>,
    Json(body): Json<UpdateProviderRequest>,
) -> Response {
    // Update credential in keyring if provided.
    if let Some(cred) = &body.credential {
        let config = state.config.read().await;
        if let Some(provider) = config.providers.iter().find(|p| p.tag == tag) {
            let result = match provider.provider_type {
                crate::keystore::ProviderType::OpenAI
                | crate::keystore::ProviderType::DashScope
                | crate::keystore::ProviderType::Copilot => {
                    crate::keystore::store_provider_key(&tag, cred)
                }
                crate::keystore::ProviderType::Anthropic => {
                    crate::keystore::store_auth_token(&tag, cred)
                }
            };
            if let Err(e) = result {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorBody {
                        error: format!("Failed to update credential: {e}"),
                    }),
                )
                    .into_response();
            }
        } else {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorBody {
                    error: format!("Provider '{tag}' not found."),
                }),
            )
                .into_response();
        }
    }

    // Update provider config.
    let mut config = state.config.write().await;
    if let Some(provider) = config.providers.iter_mut().find(|p| p.tag == tag) {
        if let Some(base_url) = body.base_url {
            provider.base_url = Some(base_url);
        }
        if let Some(oauth_path) = body.oauth_creds_path {
            provider.oauth_creds_path = Some(oauth_path);
        }

        if let Err(e) = config.save_to_keyring() {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorBody {
                    error: format!("Failed to persist config: {e}"),
                }),
            )
                .into_response();
        }

        info!(tag = %tag, "Provider updated");
        StatusCode::OK.into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorBody {
                error: format!("Provider '{tag}' not found."),
            }),
        )
            .into_response()
    }
}

async fn handle_delete_provider(
    State(state): State<CoreState>,
    axum::extract::Path(tag): axum::extract::Path<String>,
) -> Response {
    let mut config = state.config.write().await;
    let initial_len = config.providers.len();
    config.providers.retain(|p| p.tag != tag);

    if config.providers.len() == initial_len {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorBody {
                error: format!("Provider '{tag}' not found."),
            }),
        )
            .into_response();
    }

    if let Err(e) = config.save_to_keyring() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorBody {
                error: format!("Failed to persist config: {e}"),
            }),
        )
            .into_response();
    }

    info!(tag = %tag, "Provider deleted");
    StatusCode::OK.into_response()
}

async fn handle_list_models(State(state): State<CoreState>) -> Json<ModelListResponse> {
    let config = state.config.read().await;
    Json(ModelListResponse {
        models: config.models.clone(),
    })
}

async fn handle_add_model(
    State(state): State<CoreState>,
    Json(body): Json<AddModelRequest>,
) -> Response {
    let mut config = state.config.write().await;
    config
        .models
        .insert(body.tag.clone(), body.provider_mappings);

    if let Err(e) = config.save_to_keyring() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorBody {
                error: format!("Failed to persist config: {e}"),
            }),
        )
            .into_response();
    }

    info!(tag = %body.tag, "Model mapping added");
    StatusCode::CREATED.into_response()
}

async fn handle_delete_model(
    State(state): State<CoreState>,
    axum::extract::Path(tag): axum::extract::Path<String>,
) -> Response {
    let mut config = state.config.write().await;
    if config.models.remove(&tag).is_some() {
        if let Err(e) = config.save_to_keyring() {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorBody {
                    error: format!("Failed to persist config: {e}"),
                }),
            )
                .into_response();
        }

        info!(tag = %tag, "Model mapping deleted");
        StatusCode::OK.into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorBody {
                error: format!("Model '{tag}' not found."),
            }),
        )
            .into_response()
    }
}

async fn handle_get_settings(State(state): State<CoreState>) -> Json<SettingsResponse> {
    let config = state.config.read().await;
    Json(SettingsResponse {
        settings: config.settings.clone(),
    })
}

async fn handle_update_settings(
    State(state): State<CoreState>,
    Json(body): Json<UpdateSettingsRequest>,
) -> Response {
    let mut config = state.config.write().await;

    if let Some(log_level) = body.log_level {
        config.settings.log_level = log_level;
    }
    if let Some(ipc_pipe) = body.ipc_pipe {
        config.settings.ipc_pipe = ipc_pipe;
    }

    if let Err(e) = config.save_to_keyring() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorBody {
                error: format!("Failed to persist settings: {e}"),
            }),
        )
            .into_response();
    }

    info!("Service settings updated");
    StatusCode::OK.into_response()
}
