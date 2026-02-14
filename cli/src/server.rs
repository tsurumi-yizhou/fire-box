//! HTTP server for Fire Box CLI
//!
//! Provides an OpenAI/Anthropic-compatible HTTP API for LLM requests.

use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use common::{protocol::UnifiedRequest, provider, CoreState};
use serde_json::json;
use std::sync::Arc;
use tracing::info;

pub async fn start_server(host: &str, port: u16) -> Result<()> {
    info!("Starting Fire Box HTTP server on {}:{}", host, port);

    let state = Arc::new(CoreState::new().await?);

    let app = Router::new()
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/completions", post(chat_completions))
        .route("/health", post(health_check))
        .with_state(state);

    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!("Fire Box HTTP server listening on {}", addr);
    info!("OpenAI-compatible endpoint: http://{}/v1/chat/completions", addr);
    info!("Anthropic-compatible endpoint: http://{}/v1/completions", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "service": "fire-box"
    }))
}

async fn chat_completions(
    State(state): State<Arc<CoreState>>,
    Json(request): Json<UnifiedRequest>,
) -> Response {
    // Get model from request
    let model = request.model.clone();

    // Lookup model mapping
    let config = state.config.read().await;
    let mappings = match config.models.get(&model) {
        Some(m) => m,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": {
                        "message": format!("Model '{}' not configured", model),
                        "type": "invalid_request_error"
                    }
                })),
            )
                .into_response();
        }
    };

    if mappings.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": {
                    "message": format!("Model '{}' has no provider mappings", model),
                    "type": "invalid_request_error"
                }
            })),
        )
            .into_response();
    }

    // Use first mapping
    let mapping = &mappings[0];

    // Get provider
    let provider_config = match config.providers.iter().find(|p| p.tag == mapping.provider) {
        Some(p) => p,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": {
                        "message": format!("Provider '{}' not found", mapping.provider),
                        "type": "server_error"
                    }
                })),
            )
                .into_response();
        }
    };

    // Make request
    let mut req = request;
    req.model = mapping.model_id.clone();

    match provider::send_request(&state.http, provider_config, &req.model, &req).await {
        Ok(response) => Json(json!({"content": response})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": {
                    "message": e.to_string(),
                    "type": "server_error"
                }
            })),
        )
            .into_response(),
    }
}
