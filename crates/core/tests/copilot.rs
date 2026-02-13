//! GitHub Copilot integration test.
//!
//! This test requires a valid GitHub token with Copilot access.
//! The token should be configured in local config files:
//!
//! - Windows: `%LOCALAPPDATA%\\github-copilot\\hosts.json` or `apps.json`
//! - Linux/macOS: `$XDG_CONFIG_HOME/github-copilot/hosts.json` or `apps.json`
//!
//! Get a GitHub token from: https://github.com/settings/tokens (requires 'read:user' scope)
use fire_box_core::protocol::*;
use fire_box_core::protocols::copilot;
use reqwest::Client;
use tracing::{info, warn};
use tracing_subscriber;

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_test_writer()
        .try_init();
}

#[tokio::test]
#[ignore]
async fn test_copilot_oauth() {
    init_tracing();

    info!("Testing GitHub Copilot token exchange and chat");
    info!("⚠️  This test requires a valid GitHub token (see file header for instructions)");

    let http = Client::new();

    let provider_tag = "Copilot-Test";
    // Clear any cached token from keyring to test fresh token loading
    let _ = fire_box_core::keystore::delete_provider_key(provider_tag);

    let session = copilot::ensure_session(&http, provider_tag)
        .await
        .expect("Copilot token exchange failed. Please configure a valid GitHub token (see test file header)");

    info!("✅ GitHub Copilot token exchange succeeded, session_token acquired");

    let req = UnifiedRequest {
        model: "gpt-4".to_string(),
        messages: vec![UnifiedMessage {
            role: "user".to_string(),
            content: MessageContent::Parts(vec![ContentPart::Text {
                text: "Say 'Hello from Copilot integration test!' in one sentence.".to_string(),
            }]),
        }],
        stream: false,
        max_tokens: None,
        temperature: None,
        files: vec![],
    };

    let body = copilot::encode_request(&req, "gpt-4")
        .await
        .expect("Failed to encode Copilot request");
    let headers = copilot::request_headers(&session);
    let url = "https://api.githubcopilot.com/chat/completions";

    let mut builder = http.post(url);
    for (name, value) in &headers {
        builder = builder.header(*name, value);
    }
    let resp = builder
        .json(&body)
        .send()
        .await
        .expect("Copilot request failed");

    let status = resp.status();
    let bytes = resp.bytes().await.expect("Failed to read Copilot response");

    if !status.is_success() {
        warn!(
            "Copilot request failed ({}): {}",
            status,
            String::from_utf8_lossy(&bytes)
        );
    } else {
        let text = copilot::parse_full_response(&bytes).expect("Failed to parse Copilot response");
        info!("✅ Copilot chat response: {}", text);
        assert!(!text.is_empty(), "Copilot response should not be empty");
    }
}
