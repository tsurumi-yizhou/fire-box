/// Session management.
///
/// Sessions are identified by the client's source port (i.e. the ephemeral TCP
/// port of the incoming connection). Each unique source port is assigned a
/// stable UUID session ID for the lifetime of that connection.
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Manages the mapping from client source port to session ID.
#[derive(Debug, Clone)]
pub struct SessionManager {
    /// Map from source port → session UUID.
    sessions: Arc<RwLock<HashMap<u16, Uuid>>>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get or create a session ID for the given remote address.
    /// The source port of the remote address is used as the session key.
    #[allow(dead_code)]
    pub async fn get_or_create(&self, remote: SocketAddr) -> Uuid {
        let port = remote.port();
        // Fast path: read lock.
        {
            let sessions = self.sessions.read().await;
            if let Some(&id) = sessions.get(&port) {
                return id;
            }
        }
        // Slow path: write lock.
        let mut sessions = self.sessions.write().await;
        *sessions.entry(port).or_insert_with(Uuid::new_v4)
    }

    /// Remove a session (e.g. when a connection is closed).
    #[allow(dead_code)]
    pub async fn remove(&self, remote: SocketAddr) {
        let port = remote.port();
        let mut sessions = self.sessions.write().await;
        sessions.remove(&port);
    }

    /// Get the session ID for a given remote address, if it exists.
    #[allow(dead_code)]
    pub async fn get(&self, remote: SocketAddr) -> Option<Uuid> {
        let port = remote.port();
        let sessions = self.sessions.read().await;
        sessions.get(&port).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[tokio::test]
    async fn test_session_creation() {
        let mgr = SessionManager::new();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345);
        let id1 = mgr.get_or_create(addr).await;
        let id2 = mgr.get_or_create(addr).await;
        assert_eq!(id1, id2, "Same port should yield same session ID");
    }

    #[tokio::test]
    async fn test_different_ports() {
        let mgr = SessionManager::new();
        let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345);
        let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12346);
        let id1 = mgr.get_or_create(addr1).await;
        let id2 = mgr.get_or_create(addr2).await;
        assert_ne!(id1, id2, "Different ports should yield different sessions");
    }

    #[tokio::test]
    async fn test_session_removal() {
        let mgr = SessionManager::new();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345);
        let _id = mgr.get_or_create(addr).await;
        assert!(mgr.get(addr).await.is_some());
        mgr.remove(addr).await;
        assert!(mgr.get(addr).await.is_none());
    }
}
