//! DashScope integration test (single-target, OAuth device flow).
use fire_box_core::protocol::*;
use fire_box_core::protocols::dashscope;
use reqwest::Client;
use tokio::sync::broadcast;
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
async fn test_dashscope_oauth() {
    init_tracing();

    info!("Testing DashScope OAuth device code flow from zero authorization");
    info!("⚠️  This test requires manual browser interaction!");

    let http = Client::new();
    let (event_tx, mut event_rx) = broadcast::channel(16);

    tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            match event {
                fire_box_core::ipc::IpcEvent::OAuthOpenUrl {
                    provider,
                    url,
                    user_code,
                } => {
                    println!("\n");
                    println!("╔════════════════════════════════════════════════════════════════╗");
                    println!("║ DashScope OAuth Authorization Required                        ║");
                    println!("╠════════════════════════════════════════════════════════════════╣");
                    println!(
                        "║ Provider:   {}                                        ║",
                        provider
                    );
                    println!("║ URL:        {}                  ║", url);
                    println!(
                        "║ User Code:  {}                                  ║",
                        user_code
                    );
                    println!("╚════════════════════════════════════════════════════════════════╝");
                    println!("\n👉 Copy the URL above to your browser and enter the user code.\n");
                }
                _ => {}
            }
        }
    });

    let provider_tag = "DashScope-Test";
    let creds_path = ".dashscope_oauth_test.json";

    let resolved = dashscope::ensure_access_token(&http, provider_tag, creds_path, Some(&event_tx))
        .await
        .expect("DashScope OAuth failed");

    info!("✅ DashScope OAuth succeeded, access_token acquired");

    let req = UnifiedRequest {
        model: "coder-model".to_string(),
        messages: vec![UnifiedMessage {
            role: "user".to_string(),
            content: MessageContent::Parts(vec![ContentPart::Text {
                text: "Hello, please reply in one sentence.".to_string(),
            }]),
        }],
        stream: false,
        max_tokens: None,
        temperature: None,
        files: vec![],
    };

    let body = dashscope::encode_request(&req, "coder-model")
        .await
        .expect("Failed to encode DashScope request");
    let headers = dashscope::request_headers(&resolved.access_token);
    let url = format!(
        "{}{}",
        resolved.base_url.trim_end_matches('/'),
        dashscope::endpoint_path()
    );

    let mut builder = http.post(&url);
    for (name, value) in &headers {
        builder = builder.header(*name, value);
    }
    let resp = builder
        .json(&body)
        .send()
        .await
        .expect("DashScope request failed");

    let status = resp.status();
    let bytes = resp
        .bytes()
        .await
        .expect("Failed to read DashScope response");

    if !status.is_success() {
        warn!(
            "DashScope request failed ({}): {}",
            status,
            String::from_utf8_lossy(&bytes)
        );
    } else {
        let text =
            dashscope::parse_full_response(&bytes).expect("Failed to parse DashScope response");
        info!("✅ DashScope chat response: {}", text);
        assert!(!text.is_empty(), "DashScope response should not be empty");
    }

    let _ = std::fs::remove_file(creds_path);
}
