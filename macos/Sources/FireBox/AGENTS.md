# macos/Sources/FireBox/

> ⚠️ **Please update me promptly.**

Main Swift executable target for the macOS menu-bar application. Uses Swift 6 strict concurrency (`@MainActor`, `Sendable`, `@preconcurrency`).

## Modules

| File | Description |
|------|-------------|
| `Bridge.swift` | `@main` SwiftUI app entry point (`FireBoxApp`). Spawns `start()` (UniFFI-generated Swift wrapper) on a detached thread, waits for IPC server readiness, then initializes `CoreClient`. Service is stopped via `stop()` and config reloaded via `reload()` |
| `core.swift` | **Auto-generated** by UniFFI via `core/build.rs`. Swift bindings for the Rust core FFI — do not edit manually |
| `AppDelegate.swift` | `NSApplicationDelegate` — creates the NSStatusBar menu-bar item (flame icon), manages an `NSPopover` hosting `DashboardView`, starts `XPCService` and `FireBoxState` |
| `AppState.swift` | `@MainActor @Observable` centralized app state. Holds metrics, apps, providers, pending approvals, OAuth prompts. Polls IPC every 5s and listens to SSE event stream |
| `CoreClient.swift` | HTTP-over-Unix-domain-socket IPC client using raw POSIX sockets. Methods: `fetchMetrics`, `fetchApps`, `fetchProviders`, `fetchSettings`, `fetchModels`, `sendAuthDecision`, `revokeApp`, `eventStream` (SSE) |
| `Models.swift` | `Codable` Swift mirrors of the Rust IPC JSON types: `MetricsSnapshot`, `EntityMetrics`, `AppInfo`, `AuthDecision`, `ProviderInfo`, `ProviderMapping`, `ServiceSettings`, `IpcEvent` |
| `XPCService.swift` | `NSXPCListener`-based XPC service (`com.firebox.xpc`). Allows local apps to query the gateway via XPC → IPC relay. `XPCHandler` forwards requests to `CoreClient` |

## Subdirectories

| Directory | Description |
|-----------|-------------|
| `Views/` | SwiftUI view components for the menu-bar popover |
