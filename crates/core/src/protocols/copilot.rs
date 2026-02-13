/// Codec for the GitHub Copilot Chat API.
///
/// Copilot uses an OpenAI-compatible chat/completions endpoint with a
/// multi-step authentication flow:
///
/// 1. **Device Code OAuth** — authenticate via GitHub device flow using
///    the VS Code client ID to obtain a `ghu_` user access token.
/// 2. **Token Exchange** — exchange the `ghu_` token for a short-lived
///    Copilot session token via the internal GitHub API.
/// 3. **Chat Request** — call the Copilot proxy with OpenAI-format
///    bodies, using special VS Code headers (session ID, machine ID,
///    editor version).
///
/// The session token is cached in memory and auto-refreshed when it
/// expires. The long-lived `ghu_` token is persisted in the OS keyring.
use crate::protocol::*;
use anyhow::Context;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{info, warn};

// ─── Constants ──────────────────────────────────────────────────────────────

const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";

/// User-Agent required by the Copilot internal API.
const COPILOT_USER_AGENT: &str = "GitHubCopilotChat/0.1.0";
/// Editor version header value.
const EDITOR_VERSION: &str = "vscode/1.80.0";

/// Buffer before treating the session token as expired (seconds).
const TOKEN_REFRESH_BUFFER_SECS: u64 = 30;

// ─── Copilot Session Token Management ──────────────────────────────────────

/// Response from `GET /copilot_internal/v2/token`.
#[derive(Debug, Deserialize)]
struct CopilotTokenResponse {
    token: String,
    expires_at: i64,
}

struct SessionCache {
    /// The `ghu_` OAuth access token (long-lived).
    github_token: String,
    /// Short-lived Copilot session token.
    session_token: String,
    /// When the session token expires.
    expires_at: Instant,
    /// VS Code session ID (stable per provider session).
    session_id: String,
    /// VS Code machine ID (stable per machine, SHA-256 hex).
    machine_id: String,
}

static SESSION_CACHE: OnceLock<RwLock<std::collections::HashMap<String, SessionCache>>> =
    OnceLock::new();

fn session_cache() -> &'static RwLock<std::collections::HashMap<String, SessionCache>> {
    SESSION_CACHE.get_or_init(|| RwLock::new(std::collections::HashMap::new()))
}

/// Generate a stable machine ID (SHA-256 of hostname).
fn generate_machine_id() -> String {
    // Use COMPUTERNAME (Windows) / HOSTNAME (Unix) env vars as a stable seed.
    let hostname = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "fire-box-unknown".to_string());
    let mut hasher = Sha256::new();
    hasher.update(hostname.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Exchange the `ghu_` token for a short-lived Copilot session token.
async fn exchange_copilot_token(
    http: &reqwest::Client,
    github_token: &str,
) -> anyhow::Result<CopilotTokenResponse> {
    let resp = http
        .get(COPILOT_TOKEN_URL)
        .header("Authorization", format!("Token {github_token}"))
        .header("User-Agent", COPILOT_USER_AGENT)
        .header("Accept", "application/json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .context("Copilot token exchange request failed")?;

    let status = resp.status();
    let body = resp.bytes().await?;
    if !status.is_success() {
        anyhow::bail!(
            "Copilot token exchange failed ({}): {}",
            status,
            String::from_utf8_lossy(&body)
        );
    }

    serde_json::from_slice(&body).context("failed to parse Copilot token response")
}

fn local_config_dir() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg));
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA")
            && !local.is_empty()
        {
            return Some(PathBuf::from(local));
        }
    }

    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
    {
        return Some(PathBuf::from(home).join(".config"));
    }

    None
}

fn load_local_github_token() -> Option<String> {
    // Read GitHub token only from local configuration files.
    // (Do NOT use environment variables here; prefer explicit local config.)
    let config_dir = local_config_dir()?;
    let paths = [
        config_dir.join("github-copilot").join("hosts.json"),
        config_dir.join("github-copilot").join("apps.json"),
    ];

    for path in paths {
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let root: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let obj = match root.as_object() {
            Some(o) => o,
            None => continue,
        };

        for (key, value) in obj {
            if !key.contains("github.com") {
                continue;
            }
            if let Some(token) = value.get("oauth_token").and_then(|v| v.as_str())
                && !token.is_empty()
            {
                return Some(token.to_string());
            }
        }
    }

    None
}

// ─── Public API: ensure_access_token ────────────────────────────────────────

/// Resolved Copilot session token with all data needed for a chat request.
pub struct ResolvedSession {
    pub session_token: String,
    pub session_id: String,
    pub machine_id: String,
}

/// Ensure a valid Copilot session token is available.
///
/// Flow:
/// 1. Check in-memory cache for a valid session token.
/// 2. If the session token is expired but we have a `ghu_` token → re-exchange.
/// 3. If no `ghu_` token → try loading from local config or keyring.
/// 4. If still none → return error with instructions.
pub async fn ensure_session(
    http: &reqwest::Client,
    provider_tag: &str,
) -> anyhow::Result<ResolvedSession> {
    // Fast path: read lock — check if cached session is still valid.
    {
        let guard = session_cache().read().await;
        if let Some(cached) = guard.get(provider_tag)
            && cached.expires_at > Instant::now()
        {
            return Ok(ResolvedSession {
                session_token: cached.session_token.clone(),
                session_id: cached.session_id.clone(),
                machine_id: cached.machine_id.clone(),
            });
        }
    }

    // Slow path: write lock.
    let mut guard = session_cache().write().await;

    // Double-check after lock upgrade.
    if let Some(cached) = guard.get(provider_tag)
        && cached.expires_at > Instant::now()
    {
        return Ok(ResolvedSession {
            session_token: cached.session_token.clone(),
            session_id: cached.session_id.clone(),
            machine_id: cached.machine_id.clone(),
        });
    }

    // Try to reuse existing `ghu_` token: from local config → keyring → error.
    let github_token = if let Some(cached) = guard.get(provider_tag) {
        cached.github_token.clone()
    } else if let Some(local) = load_local_github_token() {
        // Persist to keyring for next restart.
        if let Err(e) = crate::keystore::store_provider_key(provider_tag, &local) {
            warn!(error = %e, "Failed to persist local GitHub token to keyring");
        }
        local
    } else if let Some(stored) = crate::keystore::get_provider_key(provider_tag) {
        stored
    } else {
        // No token found — return error with instructions.
        anyhow::bail!(
            "No GitHub token found for Copilot. Please:\n\
             1. Set CODESPACES=true and GITHUB_TOKEN=<your-token> environment variables, OR\n\
             2. Add token to $LOCALAPPDATA/github-copilot/hosts.json or apps.json (Windows), OR\n\
             3. Add token to $XDG_CONFIG_HOME/github-copilot/hosts.json or apps.json (Linux/macOS)\n\
             \n\
             Get a GitHub token from: https://github.com/settings/tokens (requires 'read:user' scope)"
        );
    };

    // Exchange for Copilot session token.
    let copilot_resp = match exchange_copilot_token(http, &github_token).await {
        Ok(r) => r,
        Err(e) => {
            // The stored token may be revoked — try loading fresh token from local config.
            if let Some(local) = load_local_github_token()
                && local != github_token
            {
                match exchange_copilot_token(http, &local).await {
                    Ok(resp) => {
                        if let Err(e) = crate::keystore::store_provider_key(provider_tag, &local) {
                            warn!(error = %e, "Failed to persist local GitHub token to keyring");
                        }
                        let session_id = uuid::Uuid::new_v4().to_string();
                        let machine_id = generate_machine_id();
                        let buffer = Duration::from_secs(TOKEN_REFRESH_BUFFER_SECS);
                        let expires_at = Instant::now()
                            + Duration::from_secs(
                                (resp.expires_at - chrono_like_now()).max(0) as u64
                            )
                            .saturating_sub(buffer);

                        guard.insert(
                            provider_tag.to_string(),
                            SessionCache {
                                github_token: local,
                                session_token: resp.token.clone(),
                                expires_at,
                                session_id: session_id.clone(),
                                machine_id: machine_id.clone(),
                            },
                        );

                        return Ok(ResolvedSession {
                            session_token: resp.token,
                           session_id,
                            machine_id,
                        });
                    }
                    Err(_) => {
                        // Both tokens failed — return original error with instructions.
                        return Err(e).context(
                            "Copilot token exchange failed. Your GitHub token may be revoked.\n\
                             Please update your token in environment variables or config files."
                        );
                    }
                }
            }

            // No alternative token available — return error.
            return Err(e).context(
                "Copilot token exchange failed. Your GitHub token may be revoked.\n\
                 Please update your token in environment variables or config files."
            );
        }
    };

    let session_id = guard
        .get(provider_tag)
        .map(|c| c.session_id.clone())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let machine_id = guard
        .get(provider_tag)
        .map(|c| c.machine_id.clone())
        .unwrap_or_else(generate_machine_id);

    let buffer = Duration::from_secs(TOKEN_REFRESH_BUFFER_SECS);
    let ttl_secs = (copilot_resp.expires_at - chrono_like_now()).max(0) as u64;
    let expires_at = Instant::now() + Duration::from_secs(ttl_secs).saturating_sub(buffer);

    info!(
        provider = provider_tag,
        ttl_secs, "Copilot session token acquired"
    );

    guard.insert(
        provider_tag.to_string(),
        SessionCache {
            github_token,
            session_token: copilot_resp.token.clone(),
            expires_at,
            session_id: session_id.clone(),
            machine_id: machine_id.clone(),
        },
    );

    Ok(ResolvedSession {
        session_token: copilot_resp.token,
        session_id,
        machine_id,
    })
}

/// Current Unix timestamp in seconds (substitute for chrono dependency).
fn chrono_like_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Pre-flight check for Copilot: ensures we have a valid session token.
pub async fn preflight_check(
    http: &reqwest::Client,
    provider_tag: &str,
) -> anyhow::Result<()> {
    ensure_session(http, provider_tag).await?;
    Ok(())
}

// ─── Request encoding (Unified → Copilot / OpenAI format) ──────────────────

/// Copilot uses OpenAI-compatible request format.
pub async fn encode_request(req: &UnifiedRequest, model: &str) -> anyhow::Result<Value> {
    // Reuse the OpenAI encoder — Copilot API is OpenAI-compatible.
    crate::protocols::openai::encode_request(req, model).await
}

// ─── HTTP headers for Copilot chat requests ────────────────────────────────

/// API endpoint path (unused for Copilot since we use a fixed URL, but needed
/// for the protocol interface).
pub fn endpoint_path() -> &'static str {
    "/v1/chat/completions"
}

/// Build request headers for a Copilot chat completion call.
pub fn request_headers(session: &ResolvedSession) -> Vec<(&'static str, String)> {
    vec![
        ("Authorization", format!("Bearer {}", session.session_token)),
        ("Content-Type", "application/json".into()),
        ("User-Agent", COPILOT_USER_AGENT.into()),
        ("Editor-Version", EDITOR_VERSION.into()),
        ("Vscode-Sessionid", session.session_id.clone()),
        ("Vscode-Machineid", session.machine_id.clone()),
        ("Copilot-Integration-Id", "vscode-chat".into()),
        ("Openai-Organization", "github-copilot".into()),
        ("Openai-Intent", "conversation-panel".into()),
    ]
}

// ─── Response parsing ──────────────────────────────────────────────────────

/// Parse a non-streaming Copilot response (OpenAI format).
pub fn parse_full_response(body: &[u8]) -> anyhow::Result<String> {
    // OpenAI-compatible response format.
    crate::protocols::openai::parse_full_response(body)
}

/// Parse a single SSE `data:` line from a Copilot stream (OpenAI format).
pub fn parse_stream_line(line: &str) -> Option<StreamEvent> {
    crate::protocols::openai::parse_stream_line(line)
}

// ─── Streaming / non-streaming format helpers ──────────────────────────────

#[allow(dead_code)]
pub fn format_stream_event(event: &StreamEvent, model: &str, request_id: &str) -> String {
    crate::protocols::openai::format_stream_event(event, model, request_id)
}

#[allow(dead_code)]
pub fn format_full_response(text: &str, model: &str, request_id: &str) -> Value {
    crate::protocols::openai::format_full_response(text, model, request_id)
}

// ─── Decode inbound request (for completeness, reuses OpenAI) ──────────────

#[allow(dead_code)]
pub async fn decode_request(
    body: &bytes::Bytes,
    origin_provider: &str,
) -> anyhow::Result<UnifiedRequest> {
    let (unified, _) = crate::protocols::openai::decode_request(body, origin_provider).await?;
    Ok(unified)
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_machine_id_deterministic() {
        let id1 = generate_machine_id();
        let id2 = generate_machine_id();
        assert_eq!(id1, id2, "machine ID should be deterministic");
        assert_eq!(id1.len(), 64, "SHA-256 hex should be 64 chars");
    }

    #[test]
    fn test_chrono_like_now() {
        let ts = chrono_like_now();
        assert!(ts > 1_700_000_000, "timestamp should be recent");
    }
}
