//! Anthropic integration test (single-target).
use core::protocol::*;
use core::protocols::anthropic;
use reqwest::Client;
use std::env;
use tracing::info;
use tracing_subscriber;

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_test_writer()
        .try_init();
}

#[tokio::test]
#[ignore]
async fn test_anthropic() {
    init_tracing();

    let auth_token = env::var("ANTHROPIC_AUTH_TOKEN")
        .expect("ANTHROPIC_AUTH_TOKEN not set. Run: export ANTHROPIC_AUTH_TOKEN=sk-ant-...");
    let base_url =
        env::var("ANTHROPIC_BASE_URL").unwrap_or_else(|_| "https://api.anthropic.com".to_string());

    info!("Testing Anthropic protocol at {}", base_url);

    let http = Client::new();
    let req = UnifiedRequest {
        model: "claude-3-haiku-20240307".to_string(),
        messages: vec![UnifiedMessage {
            role: "user".to_string(),
            content: MessageContent::Parts(vec![ContentPart::Text {
                text: "Say 'Hello from Anthropic integration test!' in one sentence.".to_string(),
            }]),
        }],
        stream: false,
        max_tokens: None,
        temperature: None,
        files: vec![],
    };

    let body = anthropic::encode_request(&req, "claude-3-haiku-20240307")
        .await
        .expect("Failed to encode Anthropic request");

    info!("Encoded request: {}", serde_json::to_string(&body).unwrap());

    let headers = anthropic::request_headers(&auth_token);
    let url = format!("{}/v1/messages", base_url.trim_end_matches('/'));

    let mut builder = http.post(&url);
    for (name, value) in &headers {
        builder = builder.header(*name, value);
    }
    let resp = builder
        .json(&body)
        .send()
        .await
        .expect("Anthropic request failed");

    let status = resp.status();
    let bytes = resp
        .bytes()
        .await
        .expect("Failed to read Anthropic response");

    if !status.is_success() {
        panic!(
            "Anthropic request failed ({}): {}",
            status,
            String::from_utf8_lossy(&bytes)
        );
    }

    let text = anthropic::parse_full_response(&bytes).expect("Failed to parse Anthropic response");
    info!("✅ Anthropic response: {}", text);

    assert!(!text.is_empty(), "Anthropic response should not be empty");
}
