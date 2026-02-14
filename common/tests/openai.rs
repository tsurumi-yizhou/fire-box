//! OpenAI integration test (single-target).
use core::protocol::*;
use core::protocols::openai;
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
async fn test_openai() {
    init_tracing();

    let api_key = env::var("OPENAI_API_KEY")
        .expect("OPENAI_API_KEY not set. Run: export OPENAI_API_KEY=sk-...");
    let base_url =
        env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".to_string());

    info!("Testing OpenAI protocol at {}", base_url);

    let http = Client::new();
    let req = UnifiedRequest {
        model: "gpt-3.5-turbo".to_string(),
        messages: vec![UnifiedMessage {
            role: "user".to_string(),
            content: MessageContent::Parts(vec![ContentPart::Text {
                text: "Say 'Hello from OpenAI integration test!' in one sentence.".to_string(),
            }]),
        }],
        stream: false,
        max_tokens: None,
        temperature: None,
        files: vec![],
    };

    let body = openai::encode_request(&req, "gpt-3.5-turbo")
        .await
        .expect("Failed to encode OpenAI request");

    info!("Encoded request: {}", serde_json::to_string(&body).unwrap());

    let headers = openai::request_headers(&api_key);
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

    let mut builder = http.post(&url);
    for (name, value) in &headers {
        builder = builder.header(*name, value);
    }
    let resp = builder
        .json(&body)
        .send()
        .await
        .expect("OpenAI request failed");

    let status = resp.status();
    let bytes = resp.bytes().await.expect("Failed to read OpenAI response");

    if !status.is_success() {
        panic!(
            "OpenAI request failed ({}): {}",
            status,
            String::from_utf8_lossy(&bytes)
        );
    }

    let text = openai::parse_full_response(&bytes).expect("Failed to parse OpenAI response");
    info!("✅ OpenAI response: {}", text);

    assert!(!text.is_empty(), "OpenAI response should not be empty");
}
