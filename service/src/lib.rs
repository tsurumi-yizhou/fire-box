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
