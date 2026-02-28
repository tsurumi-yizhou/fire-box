//! Trust-On-First-Use (TOFU) access control middleware.
//!
//! When an unknown client connects:
//!  - If the app_path is in the allowlist → permit immediately.
//!  - If there is an unexpired deny entry → reject immediately.
//!  - Otherwise → launch the Helper prompt; on approval write `Allow`,
//!    on rejection write `Deny` with a 24-hour expiry.
//!
//! Deny TTL: 24 hours (86400 seconds).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex as StdMutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::middleware::config::{self, AccessEntry, AccessStatus};

/// Deny entry TTL: 24 hours in milliseconds.
const DENY_TTL_MS: u64 = 86_400_000;

/// Maximum TOFU prompt failures before rate-limiting kicks in.
const TOFU_RATE_LIMIT_MAX_FAILURES: usize = 5;

/// Window (in seconds) over which TOFU failures are counted.
const TOFU_RATE_LIMIT_WINDOW_SECS: u64 = 60;

// ---------------------------------------------------------------------------
// TOFU rate limiter
// ---------------------------------------------------------------------------

/// In-memory rate limiter: tracks recent TOFU denial timestamps per app_path.
static TOFU_FAILURES: LazyLock<StdMutex<HashMap<String, Vec<u64>>>> =
    LazyLock::new(|| StdMutex::new(HashMap::new()));

/// Returns `true` if the app has exceeded the TOFU prompt rate limit.
pub fn is_tofu_rate_limited(app_path: &str) -> bool {
    let now = now_ms();
    let window_ms = TOFU_RATE_LIMIT_WINDOW_SECS * 1000;
    let cutoff = now.saturating_sub(window_ms);

    let guard = TOFU_FAILURES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(timestamps) = guard.get(app_path) {
        let recent = timestamps.iter().filter(|&&t| t >= cutoff).count();
        recent >= TOFU_RATE_LIMIT_MAX_FAILURES
    } else {
        false
    }
}

/// Record a TOFU prompt failure for rate-limiting purposes.
pub fn record_tofu_failure(app_path: &str) {
    let now = now_ms();
    let window_ms = TOFU_RATE_LIMIT_WINDOW_SECS * 1000;
    let cutoff = now.saturating_sub(window_ms);

    let mut guard = TOFU_FAILURES.lock().unwrap_or_else(|e| e.into_inner());
    let timestamps = guard.entry(app_path.to_string()).or_default();
    // Prune old entries.
    timestamps.retain(|&t| t >= cutoff);
    timestamps.push(now);
}

/// Decision returned by `check_access`.
#[derive(Debug, Clone, PartialEq)]
pub enum AccessDecision {
    /// The app is in the allowlist.
    Allow,
    /// The app has an unexpired deny entry.
    Deny,
    /// No entry exists (or expired deny) — caller must trigger Helper prompt.
    Unknown,
}

/// An entry returned by `get_allowlist` (allow-only entries).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowlistEntry {
    pub app_path: String,
    pub display_name: String,
}

/// Check the access status for the given app.
///
/// Returns `Allow`, `Deny` (unexpired), or `Unknown` (no entry / expired deny).
pub async fn check_access(app_path: &str) -> AccessDecision {
    let data = match config::load_config().await {
        Ok(d) => d,
        Err(_) => return AccessDecision::Unknown,
    };

    let Some(entry) = data.access_list.get(app_path) else {
        return AccessDecision::Unknown;
    };

    match entry.status {
        AccessStatus::Allow => AccessDecision::Allow,
        AccessStatus::Deny => {
            // Check whether the deny entry has expired.
            if let Some(expires_at) = entry.expires_at_ms {
                let now_ms = now_ms();
                if now_ms >= expires_at {
                    // Expired: treat as unknown so the Helper is re-shown.
                    return AccessDecision::Unknown;
                }
            }
            AccessDecision::Deny
        }
    }
}

/// Write an `Allow` entry for the given app.
pub async fn grant_access(app_path: &str, display_name: &str) -> Result<()> {
    let app_path = app_path.to_string();
    let display_name = display_name.to_string();
    config::update_config(move |d| {
        d.access_list.insert(
            app_path.clone(),
            AccessEntry {
                status: AccessStatus::Allow,
                display_name: display_name.clone(),
                expires_at_ms: None,
            },
        );
    })
    .await
}

/// Write a `Deny` entry with a 24-hour TTL for the given app.
pub async fn deny_access(app_path: &str, display_name: &str) -> Result<()> {
    let app_path = app_path.to_string();
    let display_name = display_name.to_string();
    let expires = now_ms() + DENY_TTL_MS;
    config::update_config(move |d| {
        d.access_list.insert(
            app_path.clone(),
            AccessEntry {
                status: AccessStatus::Deny,
                display_name: display_name.clone(),
                expires_at_ms: Some(expires),
            },
        );
    })
    .await
}

/// Remove an entry from the access list (allow or deny).
pub async fn remove_from_allowlist(app_path: &str) -> Result<()> {
    let app_path = app_path.to_string();
    config::update_config(move |d| {
        d.access_list.remove(&app_path);
    })
    .await
}

/// Return all `Allow` entries (the "allowlist" shown to the user).
pub async fn get_allowlist() -> Vec<AllowlistEntry> {
    let data = match config::load_config().await {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    data.access_list
        .iter()
        .filter(|(_, e)| e.status == AccessStatus::Allow)
        .map(|(path, e)| AllowlistEntry {
            app_path: path.clone(),
            display_name: e.display_name.clone(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// macOS: resolve PID → executable path
// ---------------------------------------------------------------------------

/// On macOS, resolve a process PID to its executable path and display name.
/// Returns `(exe_path, display_name)`.
#[cfg(target_os = "macos")]
pub fn resolve_pid(pid: i32) -> (String, String) {
    use std::ffi::CStr;

    // PROC_PIDPATHINFO_MAXSIZE = 4096
    const MAXSIZE: usize = 4096;
    let mut buf = vec![0u8; MAXSIZE];

    let ret =
        unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr() as *mut libc::c_void, MAXSIZE as u32) };

    if ret <= 0 {
        let fallback = format!("pid:{}", pid);
        return (fallback.clone(), fallback);
    }

    let exe_path = unsafe { CStr::from_ptr(buf.as_ptr() as *const libc::c_char) }
        .to_string_lossy()
        .into_owned();

    // Use the last path component as display name.
    let display = exe_path
        .split('/')
        .next_back()
        .unwrap_or(&exe_path)
        .to_string();

    (exe_path, display)
}

#[cfg(not(target_os = "macos"))]
pub fn resolve_pid(pid: i32) -> (String, String) {
    let fallback = format!("pid:{}", pid);
    (fallback.clone(), fallback)
}

// ---------------------------------------------------------------------------
// Helper: current time in milliseconds
// ---------------------------------------------------------------------------

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_decision_eq() {
        assert_eq!(AccessDecision::Allow, AccessDecision::Allow);
        assert_ne!(AccessDecision::Allow, AccessDecision::Deny);
    }
}
