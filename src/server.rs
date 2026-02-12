/// Gateway servers.
/// Creates 1 Unix socket + 1 TCP port (configured in config file):
/// - Unix socket: supports both /chat/completions and /messages
/// - TCP: supports both /chat/completions and /messages
use crate::config::{Config, ProtocolType};
use crate::filesystem;
use crate::protocol::{StreamEvent, UnifiedRequest};
use crate::protocols::{anthropic, openai};
use crate::provider;
use crate::session::SessionManager;
use axum::Json;
use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use bytes::Bytes;
use std::sync::Arc;
use tokio::net::TcpListener;
#[cfg(unix)]
use tokio::net::UnixListener;
#[cfg(unix)]
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::models::ModelRegistry;

#[derive(Clone)]
pub struct GatewayState {
    pub config: Arc<Config>,
    pub http: reqwest::Client,
    #[allow(dead_code)]
    pub session_manager: SessionManager,
    pub model_registry: Arc<ModelRegistry>,
}

/// Launch gateway servers. Returns join handles.
pub fn launch_all(
    config: Arc<Config>,
    session_manager: SessionManager,
    model_registry: Arc<ModelRegistry>,
) -> anyhow::Result<Vec<tokio::task::JoinHandle<()>>> {
    let http = reqwest::Client::new();
    let mut handles = Vec::new();

    let state = GatewayState {
        config: config.clone(),
        http: http.clone(),
        session_manager: session_manager.clone(),
        model_registry: model_registry.clone(),
    };

    let router = Router::new()
        // OpenAI-compatible endpoints
        .route("/v1/chat/completions", post(handle_openai))
        .route("/v1/models", get(list_models))
        .route("/v1/models/{model_id}", get(get_model))
        .route("/v1/embeddings", post(handle_embeddings))
        // Anthropic-compatible endpoint
        .route("/v1/messages", post(handle_anthropic))
        // File management (OpenAI-compatible)
        .route("/v1/files", get(list_files_handler).post(upload_file))
        .route(
            "/v1/files/{file_id}",
            get(get_file_info).delete(delete_file_handler),
        )
        .route("/v1/files/{file_id}/content", get(get_file_content));

    // 1. Unix socket (both protocols) - Unix only
    #[cfg(unix)]
    {
        let uds_path = PathBuf::from(shellexpand::tilde(&config.service.uds).as_ref());

        // Ensure parent directory exists
        if let Some(parent) = uds_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Remove old socket if exists
        let _ = std::fs::remove_file(&uds_path);

        let uds_state = state.clone();
        let uds_router = router.clone();
        let uds_path_display = uds_path.clone();
        handles.push(tokio::spawn(async move {
            info!(socket = %uds_path_display.display(), "Starting Unix socket");
            match UnixListener::bind(&uds_path) {
                Ok(listener) => {
                    let _ = axum::serve(listener, uds_router.with_state(uds_state).into_make_service()).await;
                }
                Err(e) => {
                    error!(socket = %uds_path_display.display(), error = %e, "Failed to bind Unix socket")
                }
            }
        }));
    }

    #[cfg(not(unix))]
    {
        warn!("Unix Domain Socket is not supported on this platform. Only TCP server will be started.");
    }

    // 2. TCP server (both protocols)
    let tcp_addr = config.service.tcp.clone();
    let tcp_state = state.clone();
    let tcp_router = router.clone();
    handles.push(tokio::spawn(async move {
        info!(addr = %tcp_addr, "Starting TCP server");
        match TcpListener::bind(&tcp_addr).await {
            Ok(listener) => {
                let _ = axum::serve(
                    listener,
                    tcp_router.with_state(tcp_state).into_make_service(),
                )
                .await;
            }
            Err(e) => error!(addr = %tcp_addr, error = %e, "Failed to bind TCP server"),
        }
    }));

    Ok(handles)
}

// ─── OpenAI protocol handler ───────────────────────────────────────────────

async fn handle_openai(State(state): State<GatewayState>, body: Bytes) -> Response {
    let session_id = Uuid::new_v4();

    // Decode request using a placeholder origin
    let (unified, _oai_req) = match openai::decode_request(&body, "gateway").await {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "Failed to decode OpenAI request");
            return error_response_openai(StatusCode::BAD_REQUEST, &e.to_string());
        }
    };

    // Find model configuration
    let model_config = match state.config.models.get(&unified.model) {
        Some(mappings) => mappings,
        None => {
            error!(model = %unified.model, "Model not configured");
            return error_response_openai(
                StatusCode::NOT_FOUND,
                &format!("Model '{}' not found", unified.model),
            );
        }
    };

    let is_stream = unified.stream;
    let model_tag = &unified.model;
    let request_id = generate_request_id(&session_id);

    // Find model capabilities by trying each configured upstream model_id.
    // Try exact match first, then try the suffix after the last '/' if present
    let metadata = model_config.iter().find_map(|m| {
        // exact
        if let Some(md) = state.model_registry.get(&m.model_id) {
            return Some(md);
        }
        // try suffix after last '/'
        if let Some(pos) = m.model_id.rfind('/') {
            let tail = &m.model_id[pos + 1..];
            if let Some(md) = state.model_registry.get(tail) {
                return Some(md);
            }
        }
        None
    });

    // Log model capabilities if available
    if let Some(metadata) = metadata {
        info!(
            model = %model_tag,
            session = %session_id,
            stream = is_stream,
            providers = model_config.len(),
            upstream_model = %metadata.name,
            tool_call = metadata.capabilities.tool_call,
            reasoning = metadata.capabilities.reasoning,
            input_text = metadata.input.text,
            input_image = metadata.input.image,
            input_pdf = metadata.input.pdf,
            "OpenAI request received"
        );
    } else {
        info!(
            model = %model_tag,
            session = %session_id,
            stream = is_stream,
            providers = model_config.len(),
            "OpenAI request received"
        );
    }

    if is_stream {
        // Streaming response.
        match try_providers_stream(&state, model_config, &unified).await {
            Ok((provider_tag, upstream_model, mut rx)) => {
                info!(model = %model_tag, provider = %provider_tag, upstream_model = %upstream_model, "Streaming from provider");
                let (body_tx, body_rx) = mpsc::channel::<Result<String, std::io::Error>>(256);
                let rid = request_id.clone();
                let model_c = upstream_model.clone();

                tokio::spawn(async move {
                    while let Some(event) = rx.recv().await {
                        let sse = openai::format_stream_event(&event, &model_c, &rid);
                        if body_tx.send(Ok(sse)).await.is_err() {
                            break;
                        }
                        if event.event_type == "done" {
                            break;
                        }
                    }
                });

                let stream = tokio_stream::wrappers::ReceiverStream::new(body_rx);
                let body = Body::from_stream(stream);

                Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "text/event-stream")
                    .header("Cache-Control", "no-cache")
                    .header("Connection", "keep-alive")
                    .body(body)
                    .unwrap()
            }
            Err(e) => {
                error!(error = %e, "All streaming providers failed");
                error_response_openai(StatusCode::BAD_GATEWAY, &e.to_string())
            }
        }
    } else {
        // Non-streaming response.
        match try_providers(&state, model_config, &unified).await {
            Ok((provider_tag, upstream_model, text)) => {
                info!(model = %model_tag, provider = %provider_tag, upstream_model = %upstream_model, "Response from provider");
                let resp = openai::format_full_response(&text, &upstream_model, &request_id);
                axum::Json(resp).into_response()
            }
            Err(e) => {
                error!(error = %e, "All providers failed");
                error_response_openai(StatusCode::BAD_GATEWAY, &e.to_string())
            }
        }
    }
}

// ─── Anthropic protocol handler ────────────────────────────────────────────

async fn handle_anthropic(State(state): State<GatewayState>, body: Bytes) -> Response {
    let session_id = Uuid::new_v4();

    let unified = match anthropic::decode_request(&body, "gateway").await {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "Failed to decode Anthropic request");
            return error_response_anthropic(StatusCode::BAD_REQUEST, &e.to_string());
        }
    };

    // Find model configuration
    let model_config = match state.config.models.get(&unified.model) {
        Some(mappings) => mappings,
        None => {
            error!(model = %unified.model, "Model not configured");
            return error_response_anthropic(
                StatusCode::NOT_FOUND,
                &format!("Model '{}' not found", unified.model),
            );
        }
    };

    let is_stream = unified.stream;
    let model_tag = &unified.model;
    let request_id = generate_request_id(&session_id);

    // Find model capabilities by trying each configured upstream model_id.
    // Try exact match first, then try the suffix after the last '/' if present
    let metadata = model_config.iter().find_map(|m| {
        if let Some(md) = state.model_registry.get(&m.model_id) {
            return Some(md);
        }
        if let Some(pos) = m.model_id.rfind('/') {
            let tail = &m.model_id[pos + 1..];
            if let Some(md) = state.model_registry.get(tail) {
                return Some(md);
            }
        }
        None
    });

    // Log model capabilities if available
    if let Some(metadata) = metadata {
        info!(
            model = %model_tag,
            session = %session_id,
            stream = is_stream,
            providers = model_config.len(),
            upstream_model = %metadata.name,
            tool_call = metadata.capabilities.tool_call,
            reasoning = metadata.capabilities.reasoning,
            input_text = metadata.input.text,
            input_image = metadata.input.image,
            input_pdf = metadata.input.pdf,
            "Anthropic request received"
        );
    } else {
        info!(
            model = %model_tag,
            session = %session_id,
            stream = is_stream,
            providers = model_config.len(),
            "Anthropic request received"
        );
    }

    if is_stream {
        match try_providers_stream(&state, model_config, &unified).await {
            Ok((provider_tag, upstream_model, mut rx)) => {
                info!(model = %model_tag, provider = %provider_tag, upstream_model = %upstream_model, "Streaming from provider");
                let (body_tx, body_rx) = mpsc::channel::<Result<String, std::io::Error>>(256);
                let rid = request_id.clone();
                let model_c = upstream_model.clone();

                // Send message_start first.
                let start = anthropic::format_stream_start(&model_c, &rid);
                let _ = body_tx.send(Ok(start)).await;

                tokio::spawn(async move {
                    while let Some(event) = rx.recv().await {
                        let sse = anthropic::format_stream_event(&event, &model_c, &rid);
                        if body_tx.send(Ok(sse)).await.is_err() {
                            break;
                        }
                        if event.event_type == "done" {
                            break;
                        }
                    }
                });

                let stream = tokio_stream::wrappers::ReceiverStream::new(body_rx);
                let body = Body::from_stream(stream);

                Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "text/event-stream")
                    .header("Cache-Control", "no-cache")
                    .header("Connection", "keep-alive")
                    .body(body)
                    .unwrap()
            }
            Err(e) => {
                error!(error = %e, "All streaming providers failed");
                error_response_anthropic(StatusCode::BAD_GATEWAY, &e.to_string())
            }
        }
    } else {
        match try_providers(&state, model_config, &unified).await {
            Ok((provider_tag, upstream_model, text)) => {
                info!(model = %model_tag, provider = %provider_tag, upstream_model = %upstream_model, "Response from provider");
                let resp = anthropic::format_full_response(&text, &upstream_model, &request_id);
                axum::Json(resp).into_response()
            }
            Err(e) => {
                error!(error = %e, "All providers failed");
                error_response_anthropic(StatusCode::BAD_GATEWAY, &e.to_string())
            }
        }
    }
}

// ─── Provider fallback logic ───────────────────────────────────────────────

/// Try each provider in the model's provider list (with fallback).
/// Returns (provider_tag, upstream_model, result_text).
async fn try_providers(
    state: &GatewayState,
    provider_mappings: &[crate::config::ProviderMapping],
    request: &UnifiedRequest,
) -> anyhow::Result<(String, String, String)> {
    let mut last_err = anyhow::anyhow!("No providers configured for model");

    for mapping in provider_mappings {
        let provider = state
            .config
            .providers
            .iter()
            .find(|p| p.tag == mapping.provider);
        let provider = match provider {
            Some(p) => p,
            None => {
                warn!(provider = %mapping.provider, "Provider not found, skipping");
                continue;
            }
        };

        // Use the configured model_id for this provider
        let upstream_model = &mapping.model_id;

        match provider::send_request(&state.http, provider, upstream_model, request).await {
            Ok(text) => {
                return Ok((mapping.provider.clone(), upstream_model.clone(), text));
            }
            Err(e) => {
                warn!(
                    provider = %mapping.provider,
                    model_id = %mapping.model_id,
                    error = %e,
                    "Provider failed, trying next"
                );
                last_err = e;
            }
        }
    }

    Err(last_err)
}

/// Try each provider in the model's provider list for streaming.
/// Returns (provider_tag, upstream_model, event_receiver).
async fn try_providers_stream(
    state: &GatewayState,
    provider_mappings: &[crate::config::ProviderMapping],
    request: &UnifiedRequest,
) -> anyhow::Result<(String, String, mpsc::Receiver<StreamEvent>)> {
    let mut last_err = anyhow::anyhow!("No providers configured for model");

    for mapping in provider_mappings {
        let provider = state
            .config
            .providers
            .iter()
            .find(|p| p.tag == mapping.provider);
        let provider = match provider {
            Some(p) => p,
            None => {
                warn!(provider = %mapping.provider, "Provider not found, skipping");
                continue;
            }
        };

        let upstream_model = &mapping.model_id;

        match provider::send_stream_request(&state.http, provider, upstream_model, request).await {
            Ok(rx) => {
                return Ok((mapping.provider.clone(), upstream_model.clone(), rx));
            }
            Err(e) => {
                warn!(
                    provider = %mapping.provider,
                    model_id = %mapping.model_id,
                    error = %e,
                    "Streaming provider failed, trying next"
                );
                last_err = e;
            }
        }
    }

    Err(last_err)
}

// ─── Helper functions ──────────────────────────────────────────────────────

fn error_response_openai(status: StatusCode, message: &str) -> Response {
    let body = serde_json::json!({
        "error": {
            "message": message,
            "type": "error",
            "code": status.as_u16(),
        }
    });
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap_or_default()))
        .unwrap()
}

fn error_response_anthropic(status: StatusCode, message: &str) -> Response {
    let body = serde_json::json!({
        "type": "error",
        "error": {
            "type": "api_error",
            "message": message,
        }
    });
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap_or_default()))
        .unwrap()
}

fn generate_request_id(session_id: &Uuid) -> String {
    let req_uuid = Uuid::new_v4();
    format!(
        "chatcmpl-{}-{}",
        &session_id.to_string()[..8],
        &req_uuid.to_string()[..8]
    )
}

// ─── OpenAI-compatible model listing ────────────────────────────────────────

/// GET /v1/models — list all configured models (OpenAI-compatible format).
async fn list_models(State(state): State<GatewayState>) -> Json<serde_json::Value> {
    let mut data: Vec<serde_json::Value> = state
        .config
        .models
        .keys()
        .map(|tag| {
            serde_json::json!({
                "id": tag,
                "object": "model",
                "created": 0,
                "owned_by": "fire-box"
            })
        })
        .collect();
    data.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Json(serde_json::json!({
        "object": "list",
        "data": data
    }))
}

/// GET /v1/models/{model_id} — retrieve a single model.
async fn get_model(Path(model_id): Path<String>, State(state): State<GatewayState>) -> Response {
    if state.config.models.contains_key(&model_id) {
        Json(serde_json::json!({
            "id": model_id,
            "object": "model",
            "created": 0,
            "owned_by": "fire-box"
        }))
        .into_response()
    } else {
        error_response_openai(
            StatusCode::NOT_FOUND,
            &format!("Model '{}' not found", model_id),
        )
    }
}

// ─── Embeddings proxy ──────────────────────────────────────────────────────

/// POST /v1/embeddings — proxy to the upstream OpenAI-compatible provider.
async fn handle_embeddings(State(state): State<GatewayState>, body: Bytes) -> Response {
    let body_value: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return error_response_openai(StatusCode::BAD_REQUEST, &e.to_string());
        }
    };

    let model = match body_value.get("model").and_then(|m| m.as_str()) {
        Some(m) => m.to_string(),
        None => {
            return error_response_openai(StatusCode::BAD_REQUEST, "Missing 'model' field");
        }
    };

    let model_config = match state.config.models.get(&model) {
        Some(c) => c,
        None => {
            return error_response_openai(
                StatusCode::NOT_FOUND,
                &format!("Model '{}' not found", model),
            );
        }
    };

    // Try each provider (only OpenAI-type supports /embeddings)
    for mapping in model_config {
        let provider = match state
            .config
            .providers
            .iter()
            .find(|p| p.tag == mapping.provider)
        {
            Some(p) => p,
            None => continue,
        };

        if provider.provider_type != ProtocolType::OpenAI {
            continue;
        }

        let base = provider
            .base_url
            .as_deref()
            .unwrap_or("")
            .trim_end_matches('/');
        let url = format!("{}/embeddings", base);

        let mut upstream_body = body_value.clone();
        upstream_body["model"] = serde_json::Value::String(mapping.model_id.clone());

        let headers = openai::request_headers(provider.api_key.as_deref().unwrap_or(""));
        let mut req = state
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .body(serde_json::to_vec(&upstream_body).unwrap_or_default());

        for (name, value) in &headers {
            req = req.header(*name, value);
        }

        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                match resp.bytes().await {
                    Ok(bytes) => {
                        return Response::builder()
                            .status(status.as_u16())
                            .header("Content-Type", "application/json")
                            .body(Body::from(bytes))
                            .unwrap();
                    }
                    Err(e) => {
                        warn!(provider = %mapping.provider, error = %e, "Embeddings response error");
                        continue;
                    }
                }
            }
            Err(e) => {
                warn!(provider = %mapping.provider, error = %e, "Embeddings provider failed");
                continue;
            }
        }
    }

    error_response_openai(
        StatusCode::BAD_GATEWAY,
        "All providers failed for embeddings",
    )
}

// ─── File management (OpenAI-compatible) ───────────────────────────────────

/// POST /v1/files — upload a file (JSON body with base64 content).
async fn upload_file(body: Bytes) -> Response {
    let v: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return error_response_openai(StatusCode::BAD_REQUEST, &e.to_string());
        }
    };

    let filename = v
        .get("filename")
        .and_then(|f| f.as_str())
        .unwrap_or("unknown")
        .to_string();
    let content = match v.get("content").and_then(|c| c.as_str()) {
        Some(c) => c.to_string(),
        None => {
            return error_response_openai(
                StatusCode::BAD_REQUEST,
                "Missing 'content' field (base64)",
            );
        }
    };
    let media_type = v
        .get("media_type")
        .and_then(|m| m.as_str())
        .unwrap_or("application/octet-stream")
        .to_string();
    let purpose = v
        .get("purpose")
        .and_then(|p| p.as_str())
        .unwrap_or("assistants");

    let bytes_len = content.len() * 3 / 4;
    let file_id = filesystem::store_file(filename.clone(), content, media_type).await;

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Json(serde_json::json!({
        "id": file_id,
        "object": "file",
        "bytes": bytes_len,
        "created_at": ts,
        "filename": filename,
        "purpose": purpose
    }))
    .into_response()
}

/// GET /v1/files — list uploaded files.
async fn list_files_handler() -> Json<serde_json::Value> {
    let files = filesystem::list_files().await;
    let data: Vec<serde_json::Value> = files
        .iter()
        .map(|(id, f)| {
            serde_json::json!({
                "id": id,
                "object": "file",
                "bytes": f.content_base64.len() * 3 / 4,
                "created_at": 0,
                "filename": f.filename,
                "purpose": "assistants"
            })
        })
        .collect();
    Json(serde_json::json!({
        "object": "list",
        "data": data
    }))
}

/// GET /v1/files/{file_id} — retrieve file metadata.
async fn get_file_info(Path(file_id): Path<String>) -> Response {
    match filesystem::get_file(&file_id).await {
        Some(f) => Json(serde_json::json!({
            "id": file_id,
            "object": "file",
            "bytes": f.content_base64.len() * 3 / 4,
            "created_at": 0,
            "filename": f.filename,
            "purpose": "assistants"
        }))
        .into_response(),
        None => error_response_openai(
            StatusCode::NOT_FOUND,
            &format!("File '{}' not found", file_id),
        ),
    }
}

/// DELETE /v1/files/{file_id} — delete an uploaded file.
async fn delete_file_handler(Path(file_id): Path<String>) -> Json<serde_json::Value> {
    let deleted = filesystem::delete_file(&file_id).await;
    Json(serde_json::json!({
        "id": file_id,
        "object": "file",
        "deleted": deleted
    }))
}

/// GET /v1/files/{file_id}/content — download file content.
async fn get_file_content(Path(file_id): Path<String>) -> Response {
    match filesystem::get_file(&file_id).await {
        Some(f) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", &f.media_type)
            .body(Body::from(f.content_base64))
            .unwrap(),
        None => error_response_openai(
            StatusCode::NOT_FOUND,
            &format!("File '{}' not found", file_id),
        ),
    }
}
