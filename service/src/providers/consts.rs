//! Centralised constants for the FireBox service.
//!
//! All magic numbers (timeouts, buffer sizes, API versions, well-known URLs)
//! live here so that they are changed in exactly one place.

use std::time::Duration;

// ---------------------------------------------------------------------------
// HTTP client timeouts
// ---------------------------------------------------------------------------

/// Default timeout for individual API calls (including streaming bodies).
pub const HTTP_TIMEOUT: Duration = Duration::from_secs(120);

/// TCP connection establishment timeout.
pub const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// How long idle keep-alive connections remain in the pool before eviction.
pub const HTTP_POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(90);

// ---------------------------------------------------------------------------
// Retry / exponential backoff
// ---------------------------------------------------------------------------

/// Maximum number of retry attempts before giving up.
pub const RETRY_MAX_RETRIES: u32 = 3;

/// Backoff duration before the first retry.
pub const RETRY_INITIAL_BACKOFF: Duration = Duration::from_millis(100);

/// Upper bound on per-retry backoff duration.
pub const RETRY_MAX_BACKOFF: Duration = Duration::from_secs(10);

/// Multiplier applied to the backoff after each failed attempt.
pub const RETRY_MULTIPLIER: f64 = 2.0;

// ---------------------------------------------------------------------------
// OAuth / device-code flows
// ---------------------------------------------------------------------------

/// Seconds before token expiry at which a cached token is considered stale
/// and should be proactively refreshed.
pub const OAUTH_TOKEN_EXPIRY_BUFFER_SECS: u64 = 60;

/// Total time (seconds) allowed for the user to complete device-code
/// authorisation before the flow is considered expired.
pub const OAUTH_DEVICE_FLOW_TIMEOUT_SECS: u64 = 300;

/// Extra seconds added to the poll interval when the server replies with
/// `slow_down`.
pub const OAUTH_SLOW_DOWN_INCREMENT_SECS: u64 = 5;

// ---------------------------------------------------------------------------
// Local model (llama.cpp) defaults
// ---------------------------------------------------------------------------

/// Default context window size (in tokens) when none is specified in config.
pub const LLAMACPP_DEFAULT_CONTEXT_SIZE: u32 = 4096;

// ---------------------------------------------------------------------------
// Anthropic API
// ---------------------------------------------------------------------------

/// `anthropic-version` header value required by all Anthropic API requests.
pub const ANTHROPIC_API_VERSION: &str = "2023-06-01";

/// Fallback `max_tokens` value when the caller does not specify one.
/// Anthropic requires this field; this acts as a conservative default.
pub const ANTHROPIC_DEFAULT_MAX_TOKENS: u32 = 4096;

// ---------------------------------------------------------------------------
// Well-known provider base URLs
//
// Used exclusively by the configuration layer (config.rs) to fill in the
// default when the user omits `base_url`.  Provider structs themselves must
// NOT hard-code these; they always receive a URL from the caller.
// ---------------------------------------------------------------------------

/// Official OpenAI REST API base URL.
pub const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

/// Official Anthropic REST API base URL.
pub const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com/v1";

// ---------------------------------------------------------------------------
// IPC listener / service loop
// ---------------------------------------------------------------------------

/// Maximum time allowed for the user to respond to a TOFU (Trust On First
/// Use) authorization prompt before the request is automatically denied.
pub const TOFU_PROMPT_TIMEOUT: Duration = Duration::from_secs(30);

/// How long (in seconds) a platform IPC listener thread sleeps between
/// keep-alive iterations.  The listener itself is event-driven; this only
/// controls the watchdog tick.
pub const IPC_LISTENER_SLEEP_SECS: u64 = 3600;

/// Interval (in seconds) between shutdown-flag polls in the service main loop.
pub const SERVICE_HEARTBEAT_SECS: u64 = 1;
