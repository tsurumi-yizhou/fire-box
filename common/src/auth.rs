/// App authentication and authorization management.
///
/// Manages which external applications are allowed to call the LLM gateway.
/// When an unknown app makes a request, the core emits an `AuthorizationRequired`
/// IPC event so the native GUI can show an approval popup. Once the user
/// approves, the app is registered and may call freely until revoked.
///
/// Authorizations are persisted to the OS keyring (via the `keyring` crate)
/// so they survive service restarts.
use crate::keystore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::warn;

// ─── Types ──────────────────────────────────────────────────────────────────

/// Persisted authorization record for a single app.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppAuthorization {
    pub app_id: String,
    pub app_name: String,
    /// Whether the user has granted access.
    pub authorized: bool,
    /// If non-empty, the app may only use these models.
    /// Empty list means all models are allowed.
    #[serde(default)]
    pub allowed_models: Vec<String>,
    /// Unix-epoch seconds when the authorization was created.
    pub created_at: u64,
    /// Unix-epoch seconds of the most recent request.
    pub last_used: u64,
    /// Cumulative request count for this app.
    pub total_requests: u64,
}

/// Info returned to the native layer for listing apps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub app_id: String,
    pub app_name: String,
    pub authorized: bool,
    pub allowed_models: Vec<String>,
    pub created_at: u64,
    pub last_used: u64,
    pub total_requests: u64,
}

impl From<&AppAuthorization> for AppInfo {
    fn from(a: &AppAuthorization) -> Self {
        Self {
            app_id: a.app_id.clone(),
            app_name: a.app_name.clone(),
            authorized: a.authorized,
            allowed_models: a.allowed_models.clone(),
            created_at: a.created_at,
            last_used: a.last_used,
            total_requests: a.total_requests,
        }
    }
}

// ─── Manager ────────────────────────────────────────────────────────────────

/// Attempt to load persisted app authorizations from the OS keyring.
/// Returns an empty map on any failure (missing data, parse error, etc.).
fn load_from_keyring() -> HashMap<String, AppAuthorization> {
    if let Some(json) = keystore::load_app_authorizations() {
        match serde_json::from_str::<HashMap<String, AppAuthorization>>(&json) {
            Ok(apps) => return apps,
            Err(e) => {
                warn!(error = %e, "Failed to parse app authorizations from keyring, starting fresh");
            }
        }
    }
    HashMap::new()
}

/// Thread-safe manager for app authorizations.
#[derive(Debug)]
pub struct AuthManager {
    apps: RwLock<HashMap<String, AppAuthorization>>,
}

impl Default for AuthManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthManager {
    pub fn new() -> Self {
        let initial = load_from_keyring();
        Self {
            apps: RwLock::new(initial),
        }
    }

    /// Create an empty manager (no keyring loading). Used by tests.
    #[cfg(test)]
    fn new_empty() -> Self {
        Self {
            apps: RwLock::new(HashMap::new()),
        }
    }

    // ── Keyring persistence ─────────────────────────────────────────────

    /// Persist the current authorization map to the OS keyring.
    /// Called after every mutation (approve / revoke / register).
    async fn persist_to_keyring(&self) {
        let json = {
            let apps = self.apps.read().await;
            match serde_json::to_string(&*apps) {
                Ok(j) => j,
                Err(e) => {
                    warn!(error = %e, "Failed to serialize app authorizations");
                    return;
                }
            }
        };
        // keyring I/O is blocking — offload to a blocking thread.
        tokio::task::spawn_blocking(move || {
            if let Err(e) = keystore::save_app_authorizations(&json) {
                warn!(error = %e, "Failed to persist app authorizations to keyring");
            }
        });
    }

    // ── Query ───────────────────────────────────────────────────────────

    /// Check if an app is authorized.
    pub async fn is_authorized(&self, app_id: &str) -> bool {
        let apps = self.apps.read().await;
        apps.get(app_id).is_some_and(|a| a.authorized)
    }

    /// Check if an app is authorized for a specific model.
    pub async fn is_authorized_for_model(&self, app_id: &str, model: &str) -> bool {
        let apps = self.apps.read().await;
        match apps.get(app_id) {
            Some(a) if a.authorized => {
                a.allowed_models.is_empty() || a.allowed_models.iter().any(|m| m == model)
            }
            _ => false,
        }
    }

    /// Check if an app is known (registered) but not yet authorized.
    pub async fn is_pending(&self, app_id: &str) -> bool {
        let apps = self.apps.read().await;
        apps.get(app_id).is_some_and(|a| !a.authorized)
    }

    /// Get info for a single app.
    pub async fn get_app(&self, app_id: &str) -> Option<AppInfo> {
        let apps = self.apps.read().await;
        apps.get(app_id).map(AppInfo::from)
    }

    /// List all known apps.
    pub async fn list_apps(&self) -> Vec<AppInfo> {
        let apps = self.apps.read().await;
        apps.values().map(AppInfo::from).collect()
    }

    // ── Mutation ─────────────────────────────────────────────────────────

    /// Register an app as pending (not yet authorized).
    /// Returns `true` if the app was newly created; `false` if it already existed.
    pub async fn register_pending(&self, app_id: &str, app_name: &str) -> bool {
        let created = {
            let mut apps = self.apps.write().await;
            if apps.contains_key(app_id) {
                return false;
            }
            apps.insert(
                app_id.to_owned(),
                AppAuthorization {
                    app_id: app_id.to_owned(),
                    app_name: app_name.to_owned(),
                    authorized: false,
                    allowed_models: Vec::new(),
                    created_at: now_epoch(),
                    last_used: 0,
                    total_requests: 0,
                },
            );
            true
        };
        if created {
            self.persist_to_keyring().await;
        }
        created
    }

    /// Approve an app, optionally restricting to specific models.
    pub async fn approve(&self, app_id: &str, allowed_models: Vec<String>) -> bool {
        let ok = {
            let mut apps = self.apps.write().await;
            if let Some(app) = apps.get_mut(app_id) {
                app.authorized = true;
                app.allowed_models = allowed_models;
                true
            } else {
                false
            }
        };
        if ok {
            self.persist_to_keyring().await;
        }
        ok
    }

    /// Deny / revoke an app's access.
    pub async fn revoke(&self, app_id: &str) -> bool {
        let ok = {
            let mut apps = self.apps.write().await;
            if let Some(app) = apps.get_mut(app_id) {
                app.authorized = false;
                true
            } else {
                false
            }
        };
        if ok {
            self.persist_to_keyring().await;
        }
        ok
    }

    /// Record that an app made a request (updates `last_used` and counter).
    pub async fn touch(&self, app_id: &str) {
        let mut apps = self.apps.write().await;
        if let Some(app) = apps.get_mut(app_id) {
            app.last_used = now_epoch();
            app.total_requests += 1;
        }
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_auth_flow() {
        let mgr = AuthManager::new_empty();

        // Unknown app is not authorized.
        assert!(!mgr.is_authorized("com.test.app").await);

        // Register as pending.
        assert!(mgr.register_pending("com.test.app", "Test App").await);
        assert!(!mgr.is_authorized("com.test.app").await);
        assert!(mgr.is_pending("com.test.app").await);

        // Approve.
        assert!(mgr.approve("com.test.app", vec![]).await);
        assert!(mgr.is_authorized("com.test.app").await);
        assert!(mgr.is_authorized_for_model("com.test.app", "gpt-5").await);

        // Revoke.
        assert!(mgr.revoke("com.test.app").await);
        assert!(!mgr.is_authorized("com.test.app").await);
    }

    #[tokio::test]
    async fn test_model_restriction() {
        let mgr = AuthManager::new_empty();
        mgr.register_pending("app1", "App 1").await;
        mgr.approve("app1", vec!["gpt-5".into()]).await;

        assert!(mgr.is_authorized_for_model("app1", "gpt-5").await);
        assert!(!mgr.is_authorized_for_model("app1", "claude-4").await);
    }

    #[tokio::test]
    async fn test_list_and_touch() {
        let mgr = AuthManager::new_empty();
        mgr.register_pending("a", "A").await;
        mgr.register_pending("b", "B").await;
        mgr.approve("a", vec![]).await;

        let list = mgr.list_apps().await;
        assert_eq!(list.len(), 2);

        mgr.touch("a").await;
        let info = mgr.get_app("a").await.unwrap();
        assert_eq!(info.total_requests, 1);
        assert!(info.last_used > 0);
    }
}
