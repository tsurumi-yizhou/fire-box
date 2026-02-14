/// File system manager for storing uploaded files in memory.
/// Files are stored with a unique ID and can be retrieved for provider upload.
use std::collections::HashMap;
use std::sync::OnceLock;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct StoredFile {
    pub filename: String,
    pub content_base64: String,
    pub media_type: String,
}

static FILE_STORE: OnceLock<RwLock<HashMap<String, StoredFile>>> = OnceLock::new();

fn file_store() -> &'static RwLock<HashMap<String, StoredFile>> {
    FILE_STORE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Store a file and return its unique ID.
pub async fn store_file(filename: String, content_base64: String, media_type: String) -> String {
    let file_id = Uuid::new_v4().to_string();
    let file = StoredFile {
        filename,
        content_base64,
        media_type,
    };
    let mut store = file_store().write().await;
    store.insert(file_id.clone(), file);
    file_id
}

/// Retrieve a file by its ID.
pub async fn get_file(file_id: &str) -> Option<StoredFile> {
    let store = file_store().read().await;
    store.get(file_id).cloned()
}

/// List all stored files.
pub async fn list_files() -> Vec<(String, StoredFile)> {
    let store = file_store().read().await;
    store.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// Delete a file by its ID. Returns true if the file existed.
pub async fn delete_file(file_id: &str) -> bool {
    let mut store = file_store().write().await;
    store.remove(file_id).is_some()
}
