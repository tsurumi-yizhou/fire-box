//! Connection tracking registry.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Information about one active XPC client connection.
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub connection_id: String,
    pub client_name: String,
    pub app_path: String,
    pub connected_at_ms: u64,
    pub requests_count: u64,
}

/// Thread-safe registry of active connections.
#[derive(Clone, Default)]
pub struct ConnectionRegistry {
    inner: Arc<Mutex<HashMap<String, ConnectionInfo>>>,
}

impl ConnectionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&self, info: ConnectionInfo) {
        // If the mutex is poisoned another thread panicked while holding it;
        // recover by accepting the potentially-inconsistent inner data.
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.insert(info.connection_id.clone(), info);
    }

    pub fn remove(&self, id: &str) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.remove(id);
    }

    pub fn increment(&self, id: &str) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(info) = guard.get_mut(id) {
            info.requests_count += 1;
        }
    }

    pub fn list(&self) -> Vec<ConnectionInfo> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.values().cloned().collect()
    }
}
