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

pub mod interfaces;
pub mod middleware;
pub mod providers;

use anyhow::Result;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let fmt_layer = tracing_subscriber::fmt::layer();

    #[cfg(target_os = "linux")]
    {
        // Try systemd journal first, fall back to console-only tracing.
        if let Ok(journal_layer) = systemd_journal_logger::JournalLog::new() {
            tracing_subscriber::registry()
                .with(filter)
                .with(journal_layer)
                .init();
            tracing::info!("Logging initialized (Journal)");
        } else {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .init();
            tracing::warn!("Failed to initialize systemd journal logger. Falling back to console.");
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .init();
    }

    tracing::info!("FireBox Service starting...");
}

/// Main service logic
async fn run_service(shutdown: ShutdownHandle) -> Result<()> {
    tracing::info!("FireBox Service running");

    #[cfg(target_os = "macos")]
    {
        // Run the XPC listener on a dedicated OS thread so it can block forever.
        std::thread::spawn(|| {
            crate::interfaces::xpc::run_listener();
        });
        tracing::info!("XPC listener started");
    }

    #[cfg(target_os = "linux")]
    {
        tokio::spawn(async {
            if let Err(e) = crate::interfaces::dbus::run_listener().await {
                tracing::error!(error = %e, "Linux IPC listener failed");
            }
        });
        tracing::info!("Linux IPC listener started");
    }

    #[cfg(target_os = "windows")]
    {
        // Run the COM listener on a dedicated OS thread so it can block forever,
        // mirroring the macOS XPC pattern.
        std::thread::spawn(|| {
            crate::interfaces::com::run_listener();
        });
        tracing::info!("Windows COM listener started");
    }

    // Wait for shutdown signal
    while !shutdown.is_requested() {
        tokio::time::sleep(tokio::time::Duration::from_secs(
            crate::providers::consts::SERVICE_HEARTBEAT_SECS,
        ))
        .await;
    }

    tracing::info!("FireBox Service shutting down");
    Ok(())
}

/// Platform-specific service entry points
#[cfg(target_os = "windows")]
mod windows_service {
    use super::*;
    use ::windows_service::{
        define_windows_service,
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher,
    };
    use std::ffi::OsString;
    use std::time::Duration;

    const SERVICE_NAME: &str = "FireBoxService";

    define_windows_service!(ffi_service_entry, ffi_service_main);

    pub fn windows_main() -> Result<()> {
        service_dispatcher::start(SERVICE_NAME, ffi_service_entry)?;
        Ok(())
    }

    fn ffi_service_main(_arguments: Vec<OsString>) {
        if let Err(e) = run_windows_service() {
            tracing::error!("Service error: {}", e);
        }
    }

    fn run_windows_service() -> Result<()> {
        init_logging();

        let shutdown = Shutdown::new();
        let shutdown_handle = shutdown.handle();
        let shutdown_for_handler = shutdown.handle();

        let status_handle = service_control_handler::register(
            SERVICE_NAME,
            move |control_event| match control_event {
                ServiceControl::Stop => {
                    tracing::info!("Service stop requested");
                    shutdown_for_handler.request();
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                _ => ServiceControlHandlerResult::NotImplemented,
            },
        )?;

        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(run_service(shutdown_handle))?;

        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

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

    tracing::info!("FireBox Service stopped");

    Ok(())
}
