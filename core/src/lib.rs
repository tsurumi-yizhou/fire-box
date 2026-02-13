//! Fire Box Core — stateful LLM gateway with auth, metrics and IPC.
//!
//! This crate implements the core logic:
//! - IPC server for communication with the native layer (Swift / C++)
//! - LLM provider abstraction (OpenAI / Anthropic / DashScope)
//! - App authentication and authorization management
//! - Real-time metrics collection (token usage, requests, connections)
//! - All configuration stored securely in OS keyring

pub mod auth;
pub mod config;
pub mod filesystem;
pub mod ipc;
pub mod keystore;
pub mod metrics;
pub mod models;
pub mod protocol;
pub mod protocols;
pub mod provider;
pub mod session;

use config::Config;
use models::ModelRegistry;
use session::SessionManager;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::watch;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use crate::auth::AuthManager;
use crate::metrics::Metrics;

/// Shared state for the entire core, accessible by the IPC server.
#[derive(Clone)]
pub struct CoreState {
    pub config: Arc<tokio::sync::RwLock<Config>>,
    pub http: reqwest::Client,
    pub session_manager: SessionManager,
    pub model_registry: Arc<ModelRegistry>,
    pub metrics: Arc<Metrics>,
    pub auth_manager: Arc<AuthManager>,
    pub ipc_event_tx: tokio::sync::broadcast::Sender<ipc::IpcEvent>,
}

// ─── Service lifecycle (start / stop / reload) ─────────────────────────────

/// Internal handle to the running service, stored in a global singleton.
struct CoreHandle {
    runtime: tokio::runtime::Runtime,
    shutdown_tx: watch::Sender<bool>,
}

/// Global singleton holding the active service handle.
static CORE_HANDLE: OnceLock<Mutex<Option<CoreHandle>>> = OnceLock::new();

fn handle_mutex() -> &'static Mutex<Option<CoreHandle>> {
    CORE_HANDLE.get_or_init(|| Mutex::new(None))
}

/// Start the Fire Box core service.
///
/// Creates a multi-threaded Tokio runtime, launches the IPC server, and
/// **blocks the calling thread** until [`stop`] is called or the service
/// errors out.
///
/// Returns 0 on success, 1 on error, 2 if already running.
#[unsafe(no_mangle)]
pub extern "C" fn fire_box_start() -> i32 {
    // Build runtime first (before taking the lock) to avoid poisoning
    // the mutex on failure.
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to create tokio runtime: {e}");
            return 1;
        }
    };

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    {
        let mut guard = handle_mutex().lock().expect("core handle mutex poisoned");
        if guard.is_some() {
            eprintln!("core is already running");
            return 2;
        }
        *guard = Some(CoreHandle {
            runtime: rt,
            shutdown_tx,
        });
    }

    // Borrow the runtime from the global handle to enter `block_on`.
    // We keep the lock as short as possible — just grab a reference and
    // enter into `block_on` which will park this thread.
    let result = {
        let guard = handle_mutex().lock().expect("core handle mutex poisoned");
        let handle = guard.as_ref().unwrap();
        let rt_handle = handle.runtime.handle().clone();
        drop(guard); // release lock before blocking

        // Enter the runtime context so we can block_on.
        let _enter = rt_handle.enter();
        rt_handle.block_on(run(shutdown_rx))
    };

    // Service finished — clean up.
    let mut guard = handle_mutex().lock().expect("core handle mutex poisoned");
    *guard = None;

    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("core error: {e}");
            1
        }
    }
}

/// Signal the running core to shut down gracefully.
///
/// Returns immediately. The [`start`] call will unblock shortly after.
/// Returns 0 on success, 1 if not running.
#[unsafe(no_mangle)]
pub extern "C" fn fire_box_stop() -> i32 {
    let guard = handle_mutex().lock().expect("core handle mutex poisoned");
    match guard.as_ref() {
        Some(h) => {
            let _ = h.shutdown_tx.send(true);
            0
        }
        None => {
            eprintln!("core is not running");
            1
        }
    }
}

/// Reload configuration from the OS keyring without restarting.
///
/// The running IPC server picks up the new configuration immediately.
/// Returns 0 on success, 1 if not running.
#[unsafe(no_mangle)]
pub extern "C" fn fire_box_reload() -> i32 {
    let guard = handle_mutex().lock().expect("core handle mutex poisoned");
    match guard.as_ref() {
        Some(h) => {
            h.runtime.handle().block_on(async {
                info!("reloading configuration from keyring");
            });
            // The actual reload is done inside the runtime so we can access
            // the CORE_STATE that was stored during start().
            h.runtime.handle().block_on(reload_inner());
            0
        }
        None => {
            eprintln!("core is not running");
            1
        }
    }
}

/// Stored reference to the live `CoreState` so `reload()` can reach it.
static CORE_STATE: OnceLock<CoreState> = OnceLock::new();

async fn reload_inner() {
    if let Some(state) = CORE_STATE.get() {
        let new_config = Config::load_from_keyring();
        info!(
            providers = new_config.providers.len(),
            models = new_config.models.len(),
            "Configuration reloaded from keyring"
        );
        let mut cfg = state.config.write().await;
        *cfg = new_config;
    }
}

/// Legacy entry point — calls [`fire_box_start`].
pub fn run_from_args() -> i32 {
    fire_box_start()
}

/// Start the core service (async inner).
async fn run(mut shutdown_rx: watch::Receiver<bool>) -> anyhow::Result<()> {
    // Load configuration from OS keyring.
    let config = Config::load_from_keyring();

    // Init logging (ignore error on re-init after restart).
    let filter =
        EnvFilter::try_new(&config.settings.log_level).unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();

    info!("fire-box core starting (keyring-based configuration)");
    info!(
        providers = config.providers.len(),
        models = config.models.len(),
        "Configuration loaded from keyring"
    );

    for p in &config.providers {
        info!(tag = %p.tag, r#type = ?p.provider_type, base_url = %p.base_url.as_deref().unwrap_or("-"), "Provider configured");
    }
    for (tag, mappings) in &config.models {
        info!(tag = %tag, providers = mappings.len(), "Model configured");
    }

    // Load model metadata from models.dev
    info!("Loading model metadata from models.dev...");
    let model_registry = match ModelRegistry::load_from_models_dev().await {
        Ok(registry) => {
            info!(
                models_loaded = registry.len(),
                "Model metadata loaded successfully"
            );
            Arc::new(registry)
        }
        Err(e) => {
            warn!(error = %e, "Failed to load model metadata from models.dev, using empty registry");
            Arc::new(ModelRegistry::new())
        }
    };

    // Pre-flight check: verify DashScope OAuth credentials at startup.
    let http = reqwest::Client::new();

    // Create the IPC event channel early so we can use it during pre-flight.
    // 64-slot buffer; native layer receives via SSE.
    let (ipc_event_tx, _) = tokio::sync::broadcast::channel::<ipc::IpcEvent>(64);

    for p in &config.providers {
        if p.provider_type == config::ProtocolType::DashScope
            && let Some(creds_path) = &p.oauth_creds_path
        {
            info!(provider = %p.tag, "Checking DashScope OAuth credentials...");
            if let Err(e) = protocols::dashscope::preflight_check(
                &http,
                &p.tag,
                creds_path,
                Some(&ipc_event_tx),
            )
            .await
            {
                warn!(provider = %p.tag, error = %e, "DashScope OAuth pre-flight check failed, provider will be unavailable");
            }
        }

        if p.provider_type == config::ProtocolType::Copilot {
            info!(provider = %p.tag, "Checking GitHub Copilot credentials...");
            if let Err(e) = protocols::copilot::preflight_check(&http, &p.tag).await {
                warn!(provider = %p.tag, error = %e, "GitHub Copilot pre-flight check failed, provider will be unavailable");
            }
        }
    }

    let config = Arc::new(tokio::sync::RwLock::new(config));
    let session_manager = SessionManager::new();
    let metrics = Arc::new(Metrics::new());
    let auth_manager = Arc::new(AuthManager::new());

    let core_state = CoreState {
        config: config.clone(),
        http: http.clone(),
        session_manager: session_manager.clone(),
        model_registry: model_registry.clone(),
        metrics: metrics.clone(),
        auth_manager: auth_manager.clone(),
        ipc_event_tx: ipc_event_tx.clone(),
    };

    // Store a reference for reload().
    let _ = CORE_STATE.set(core_state.clone());

    // Launch IPC server only (no HTTP gateway).
    let ipc_handle = ipc::launch(&core_state)?;

    info!("IPC server started, ready to accept requests from native layer");

    // Wait for either the shutdown signal or the IPC server to exit.
    tokio::select! {
        _ = shutdown_rx.wait_for(|&v| v) => {
            info!("shutdown signal received, stopping core");
        }
        _ = ipc_handle => {
            info!("IPC server exited");
        }
    }

    Ok(())
}
