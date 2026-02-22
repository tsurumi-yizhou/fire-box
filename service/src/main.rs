//! FireBox Service - Cross-platform Local AI Capability Backend
//!
//! Runs as a system service:
//! - Linux: systemd service
//! - macOS: launchd daemon
//! - Windows: Windows Service
//!
//! Provides IPC interface for:
//! - Provider management (OpenAI, Anthropic, Ollama, vLLM, etc.)
//! - Model routing and failover
//! - Metrics collection and health monitoring
//!
//! Note: This is a background service with no CLI interface.
//! All interaction is done through platform-specific IPC.

pub mod middleware;
pub mod providers;

use anyhow::Result;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Service state
static RUNNING: AtomicBool = AtomicBool::new(false);

/// Shutdown flag for graceful termination
struct Shutdown {
    flag: Arc<AtomicBool>,
}

impl Shutdown {
    fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
        }
    }

    #[allow(dead_code)]
    fn request(&self) {
        self.flag.store(true, Ordering::Relaxed);
    }

    fn handle(&self) -> ShutdownHandle {
        ShutdownHandle {
            flag: Arc::clone(&self.flag),
        }
    }
}

#[derive(Clone)]
struct ShutdownHandle {
    flag: Arc<AtomicBool>,
}

impl ShutdownHandle {
    fn is_requested(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }

    fn request(&self) {
        self.flag.store(true, Ordering::Relaxed);
    }
}

/// Initialize structured logging with tracing
fn init_logging() {
    use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

    #[cfg(target_os = "linux")]
    {
        // Try systemd journal first, fall back to tracing
        if systemd_journal_logger::JournalLog::new()
            .unwrap()
            .install()
            .is_err()
        {
            tracing_subscriber::registry()
                .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
                .with(tracing_subscriber::fmt::layer())
                .init();
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        tracing_subscriber::registry()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    tracing::info!("FireBox Service starting...");
}

/// Main service logic
async fn run_service(shutdown: ShutdownHandle) -> Result<()> {
    tracing::info!("FireBox Service running");

    // TODO: Initialize IPC server (XPC on macOS, D-Bus on Linux, Named Pipes on Windows)
    // TODO: Start listening for incoming requests

    // Wait for shutdown signal
    while !shutdown.is_requested() {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    tracing::info!("FireBox Service shutting down");
    Ok(())
}

/// Platform-specific service entry points
#[cfg(target_os = "windows")]
mod windows_service {
    use super::*;
    use std::ffi::OsString;
    use std::time::Duration;
    use ::windows_service::{
        define_windows_service,
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher,
    };

    const SERVICE_NAME: &str = "FireBoxService";
    const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

    define_windows_service!(ffi_service_entry, ffi_service_main);

    pub fn windows_main() -> Result<()> {
        // Register the service dispatcher
        service_dispatcher::start(SERVICE_NAME, ffi_service_entry)?;
        Ok(())
    }

    fn ffi_service_main(_arguments: Vec<OsString>) {
        if let Err(e) = run_windows_service() {
            log::error!("Service error: {}", e);
        }
    }

    fn run_windows_service() -> Result<()> {
        // Initialize logging (Windows Event Log would be better)
        let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .try_init();

        log::info!("FireBox Service starting...");

        // Create shutdown channel
        let (tx, rx) = std::sync::mpsc::channel();

        // Register service control handler
        let status_handle = service_control_handler::register(
            SERVICE_NAME,
            move |control_event| match control_event {
                ServiceControl::Stop => {
                    log::info!("Service stop requested");
                    let _ = tx.send(());
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                _ => ServiceControlHandlerResult::NotImplemented,
            },
        )?;

        // Report running status
        let mut next_status = ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        };
        status_handle.set_service_status(next_status.clone())?;

        // Run service logic
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            // Wait for stop signal
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    // Service running
                    loop {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        if rx.try_recv().is_ok() {
                            break;
                        }
                    }
                }
            }
        });

        // Report stopped status
        next_status.current_state = ServiceState::Stopped;
        next_status.controls_accepted = ServiceControlAccept::empty();
        status_handle.set_service_status(next_status)?;

        log::info!("FireBox Service stopped");
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn main() -> Result<()> {
    windows_service::windows_main()
}

#[cfg(not(target_os = "windows"))]
#[tokio::main]
async fn main() -> Result<()> {
    init_logging();

    // Set up shutdown handling
    let shutdown = Shutdown::new();
    let shutdown_handle = shutdown.handle();

    // Set global running flag
    RUNNING.store(true, Ordering::Relaxed);

    // Set up signal handlers
    let shutdown_clone = shutdown.handle();
    tokio::spawn(async move {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                tracing::info!("Received Ctrl+C, shutting down...");
                shutdown_clone.request();
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to set up Ctrl+C handler");
            }
        }
    });

    // Handle SIGTERM on Unix
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let shutdown_clone = shutdown.handle();
        tokio::spawn(async move {
            let mut term = signal(SignalKind::terminate())?;
            term.recv().await;
            tracing::info!("Received SIGTERM, shutting down...");
            shutdown_clone.request();
            anyhow::Ok(())
        });
    }

    // Run the service
    run_service(shutdown_handle).await?;

    RUNNING.store(false, Ordering::Relaxed);
    tracing::info!("FireBox Service stopped");

    Ok(())
}
