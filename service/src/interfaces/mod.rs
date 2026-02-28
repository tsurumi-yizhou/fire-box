//! Platform-specific interfaces.

#[cfg(target_os = "macos")]
pub mod capability;
#[cfg(target_os = "macos")]
pub mod control;
#[cfg(target_os = "macos")]
pub mod xpc;

#[cfg(target_os = "linux")]
pub mod dbus;

#[cfg(target_os = "windows")]
pub mod com;

// connections.rs is used by all platform transports.
pub mod connections;

#[cfg(target_os = "macos")]
pub mod codec;
