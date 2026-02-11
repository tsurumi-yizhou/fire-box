use crate::config::{ChannelConfig, Config, ProtocolType};
/// Channel servers.
/// Each channel listens on its configured port and serves requests in
/// either OpenAI or Anthropic protocol format.
use crate::protocols::{anthropic, dashscope, openai};
use crate::provider;
use crate::router;
use crate::session::SessionManager;
use axum::Router;
use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use bytes::Bytes;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Clone)]
pub struct ChannelState {
    pub config: Arc<Config>,
    pub channel: ChannelConfig,
    pub http: reqwest::Client,
    pub session_manager: SessionManager,
}

/// Launch all channel servers. Returns join handles.
pub fn launch_all(
    config: Arc<Config>,
    session_manager: SessionManager,
) -> Vec<tokio::task::JoinHandle<()>> {
    let http = reqwest::Client::new();
    let mut handles = Vec::new();

    for channel in &config.channels {
        let state = ChannelState {
            config: config.clone(),
            channel: channel.clone(),
            http: http.clone(),
            session_manager: session_manager.clone(),
        };

        let channel_tag = channel.tag.clone();
        let port = channel.port;
        let channel_type = channel.channel_type;

        let handle = tokio::spawn(async move {
            let app = build_router(channel_type, state);
            let addr = SocketAddr::from(([0, 0, 0, 0], port));
            info!(channel = %channel_tag, port = port, protocol = ?channel_type, "Starting channel");

            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    error!(port = port, error = %e, "Failed to bind channel port");
                    return;
                }
            };
            if let Err(e) = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            {
                error!(channel = %channel_tag, error = %e, "Channel server error");
            }
        });

        handles.push(handle);
    }

    handles
}

fn build_router(channel_type: ProtocolType, state: ChannelState) -> Router {
    match channel_type {
        ProtocolType::OpenAI => Router::new()
            .route("/v1/chat/completions", post(handle_openai))
            .route("/v1/models", get(handle_models))
            .route("/health", get(health))
            .with_state(state),
        ProtocolType::Anthropic => Router::new()
            .route("/v1/messages", post(handle_anthropic))
            .route("/health", get(health))
            .with_state(state),
        ProtocolType::DashScope => Router::new()
            .route(
                "/compatible-mode/v1/chat/completions",
                post(handle_dashscope),
            )
            .route("/v1/chat/completions", post(handle_dashscope))
            .route("/v1/models", get(handle_models))
            .route("/health", get(health))
            .with_state(state),
    }
}

async fn health() -> &'static str {
    "ok"
}

async fn handle_models(State(state): State<ChannelState>) -> impl IntoResponse {
    // Return a minimal models list with just this channel's tag as the model.
    let models = serde_json::json!({
        "object": "list",
        "data": [{
            "id": state.channel.tag,
            "object": "model",
            "created": 0,
            "owned_by": "fire-box",
        }]
    });
    axum::Json(models)
}

// ─── OpenAI channel handler ────────────────────────────────────────────────

async fn handle_openai(
    State(state): State<ChannelState>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Session management.
    let session_id = state.session_manager.get_or_create(remote).await;

    // Validate API key.
    if let Err(resp) = validate_api_key(&headers, &state.channel.api_key, ProtocolType::OpenAI) {
        return *resp;
    }

    // Decode request.
    let (unified, _oai_req) = match openai::decode_request(&body, &state.channel.tag).await {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "Failed to decode OpenAI request");
            return error_response_openai(StatusCode::BAD_REQUEST, &e.to_string());
        }
    };

    let is_stream = unified.stream;
    let channel_tag = &state.channel.tag;
    let request_id = generate_request_id(&session_id);

    // Route.
    let candidates = router::resolve_route(&state.config, channel_tag, &unified);
    if candidates.is_empty() {
        return error_response_openai(
            StatusCode::INTERNAL_SERVER_ERROR,
            "No route matched for request",
        );
    }

    info!(
        channel = %channel_tag,
        session = %session_id,
        stream = is_stream,
        candidates = candidates.len(),
        "Routing request"
    );

    if is_stream {
        // Streaming response.
        match provider::try_candidates_stream(
            &state.http,
            &state.config.providers,
            &candidates,
            &unified,
        )
        .await
        {
            Ok((_prov_tag, model, mut rx)) => {
                let (body_tx, body_rx) = mpsc::channel::<Result<String, std::io::Error>>(256);
                let rid = request_id.clone();
                let model_c = model.clone();

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
                error!(error = %e, "All streaming candidates failed");
                error_response_openai(StatusCode::BAD_GATEWAY, &e.to_string())
            }
        }
    } else {
        // Non-streaming response.
        match provider::try_candidates(&state.http, &state.config.providers, &candidates, &unified)
            .await
        {
            Ok((_prov, model, text)) => {
                let resp = openai::format_full_response(&text, &model, &request_id);
                axum::Json(resp).into_response()
            }
            Err(e) => {
                error!(error = %e, "All candidates failed");
                error_response_openai(StatusCode::BAD_GATEWAY, &e.to_string())
            }
        }
    }
}

// ─── Anthropic channel handler ─────────────────────────────────────────────

async fn handle_anthropic(
    State(state): State<ChannelState>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Session management.
    let session_id = state.session_manager.get_or_create(remote).await;

    if let Err(resp) = validate_api_key(&headers, &state.channel.api_key, ProtocolType::Anthropic) {
        return *resp;
    }

    let unified = match anthropic::decode_request(&body, &state.channel.tag).await {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "Failed to decode Anthropic request");
            return error_response_anthropic(StatusCode::BAD_REQUEST, &e.to_string());
        }
    };

    let is_stream = unified.stream;
    let channel_tag = &state.channel.tag;
    let request_id = generate_request_id(&session_id);

    let candidates = router::resolve_route(&state.config, channel_tag, &unified);
    if candidates.is_empty() {
        return error_response_anthropic(
            StatusCode::INTERNAL_SERVER_ERROR,
            "No route matched for request",
        );
    }

    info!(
        channel = %channel_tag,
        session = %session_id,
        stream = is_stream,
        candidates = candidates.len(),
        "Routing Anthropic request"
    );

    if is_stream {
        match provider::try_candidates_stream(
            &state.http,
            &state.config.providers,
            &candidates,
            &unified,
        )
        .await
        {
            Ok((_prov_tag, model, mut rx)) => {
                let (body_tx, body_rx) = mpsc::channel::<Result<String, std::io::Error>>(256);
                let rid = request_id.clone();
                let model_c = model.clone();

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
                error!(error = %e, "All streaming candidates failed");
                error_response_anthropic(StatusCode::BAD_GATEWAY, &e.to_string())
            }
        }
    } else {
        match provider::try_candidates(&state.http, &state.config.providers, &candidates, &unified)
            .await
        {
            Ok((_prov, model, text)) => {
                let resp = anthropic::format_full_response(&text, &model, &request_id);
                axum::Json(resp).into_response()
            }
            Err(e) => {
                error!(error = %e, "All candidates failed");
                error_response_anthropic(StatusCode::BAD_GATEWAY, &e.to_string())
            }
        }
    }
}

// ─── Helpers ───────────────────────────────────────────────────────────────

// ─── DashScope channel handler ─────────────────────────────────────────────

async fn handle_dashscope(
    State(state): State<ChannelState>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let session_id = state.session_manager.get_or_create(remote).await;

    if let Err(resp) = validate_api_key(&headers, &state.channel.api_key, ProtocolType::DashScope) {
        return *resp;
    }

    let unified = match dashscope::decode_request(&body, &state.channel.tag).await {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "Failed to decode DashScope request");
            return error_response_openai(StatusCode::BAD_REQUEST, &e.to_string());
        }
    };

    let is_stream = unified.stream;
    let channel_tag = &state.channel.tag;
    let request_id = generate_request_id(&session_id);

    let candidates = router::resolve_route(&state.config, channel_tag, &unified);
    if candidates.is_empty() {
        return error_response_openai(
            StatusCode::INTERNAL_SERVER_ERROR,
            "No route matched for request",
        );
    }

    info!(
        channel = %channel_tag,
        session = %session_id,
        stream = is_stream,
        candidates = candidates.len(),
        "Routing DashScope request"
    );

    if is_stream {
        match provider::try_candidates_stream(
            &state.http,
            &state.config.providers,
            &candidates,
            &unified,
        )
        .await
        {
            Ok((_prov_tag, model, mut rx)) => {
                let (body_tx, body_rx) = mpsc::channel::<Result<String, std::io::Error>>(256);
                let rid = request_id.clone();
                let model_c = model.clone();

                tokio::spawn(async move {
                    while let Some(event) = rx.recv().await {
                        let sse = dashscope::format_stream_event(&event, &model_c, &rid);
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
                error!(error = %e, "All streaming candidates failed");
                error_response_openai(StatusCode::BAD_GATEWAY, &e.to_string())
            }
        }
    } else {
        match provider::try_candidates(&state.http, &state.config.providers, &candidates, &unified)
            .await
        {
            Ok((_prov, model, text)) => {
                let resp = dashscope::format_full_response(&text, &model, &request_id);
                axum::Json(resp).into_response()
            }
            Err(e) => {
                error!(error = %e, "All candidates failed");
                error_response_openai(StatusCode::BAD_GATEWAY, &e.to_string())
            }
        }
    }
}

// ─── Helpers ───────────────────────────────────────────────────────────────

fn validate_api_key(
    headers: &HeaderMap,
    expected: &str,
    protocol: ProtocolType,
) -> Result<(), Box<Response>> {
    let key = match protocol {
        ProtocolType::OpenAI | ProtocolType::DashScope => headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .unwrap_or(""),
        ProtocolType::Anthropic => headers
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .unwrap_or(""),
    };

    if key != expected {
        warn!("Invalid API key");
        let resp = match protocol {
            ProtocolType::OpenAI | ProtocolType::DashScope => {
                error_response_openai(StatusCode::UNAUTHORIZED, "Invalid API key")
            }
            ProtocolType::Anthropic => {
                error_response_anthropic(StatusCode::UNAUTHORIZED, "Invalid API key")
            }
        };
        return Err(Box::new(resp));
    }
    Ok(())
}

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
    // Use the session UUID prefix + a fresh v7 UUID for uniqueness.
    let req_uuid = Uuid::new_v4();
    format!(
        "chatcmpl-{}-{}",
        &session_id.to_string()[..8],
        &req_uuid.to_string()[..8]
    )
}
