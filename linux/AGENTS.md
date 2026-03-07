# Agents.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

FireBox is an AI gateway service for Linux that routes requests from local applications to AI providers (OpenAI, Anthropic, etc.) via D-Bus IPC. It consists of a backend daemon, a GTK4 frontend, and a client SDK library.

## Build System

Meson build system with C++23. All build files are under `linux/`.

```bash
# Configure (from linux/)
meson setup builddir

# Build
meson compile -C linux/builddir

# Run all tests
meson test -C linux/builddir

# Run a single test (by name from test/meson.build)
meson test -C linux/builddir storage
meson test -C linux/builddir coroutine
meson test -C linux/builddir router
meson test -C linux/builddir client
meson test -C linux/builddir dbus-ping
```

**Build targets:** `firebox-backend` (daemon), `firebox` (GUI), `libfirebox-client.a` (SDK), `libfirebox-common.a` (shared lib).

**Key dependencies:** sdbus-c++ (D-Bus), gtkmm-4.0 + libadwaita-1 (GUI), libsoup-3.0 (HTTP), sqlite3 (storage), nlohmann_json, spdlog, polkit-gobject-1, boost-ut (testing).

**Code generation:** `sdbus-c++-xml2cpp` generates D-Bus adaptor/proxy headers from XML files in `linux/data/dbus/`.

## Architecture

```
Third-party apps ──► Client SDK ──► D-Bus ──► Backend Service ──► AI Provider APIs
                                      ▲
                              Frontend GUI (Control interface)
```

### Components (`linux/src/`)

- **backend/**: D-Bus service daemon. `Service` is the central orchestrator owning Storage, D-Bus connection, and adaptors. Two D-Bus interfaces: `org.firebox.Capability` (for client apps) and `org.firebox.Control` (for the frontend).
- **frontend/**: GTK4/Libadwaita GUI. `Application` → `MainWindow` → page stack (Dashboard, Settings, Route, Allowlist, Connections). Uses D-Bus Control proxy to communicate with backend.
- **client/**: SDK library (`FireBoxClient` + `ChatStream`). Wraps D-Bus Capability proxy. Factory method `connect()` with auto-discovery.
- **common/**: Shared types and utilities. `Storage` (SQLite RAII wrapper), `coroutine.hpp` (C++23 coroutine awaitables for GLib main loop), `dbus_types.hpp` (shared D-Bus structs), `error.hpp` (structured error codes with retryability), `credential.hpp` (systemd-creds encryption).

### Key Patterns

- **C++23 coroutines** integrated with GLib event loop: `Task<T>`, `GAsyncAwaitable`, `spawn()` for fire-and-forget scheduling. All coroutine resumptions happen on the GLib main thread. Fire-and-forget tasks self-destroy via `final_suspend` — `spawn()` releases handle ownership and the coroutine frame is destroyed in `FinalAwaiter::await_suspend` when there is no continuation.
- **D-Bus method signatures** return `std::tuple<bool success, std::string message, ...>`.
- **Virtual model routing**: clients reference virtual model IDs; the `Router` resolves to physical provider/model targets using Failover or Random strategies. `Service::resolve_route()` delegates to `Router::resolve()`.
- **TOFU authorization**: Trust On First Use via Polkit — unknown callers trigger a user approval dialog, then get added to the allowlist. All `CapabilityAdaptor` methods call `check_caller_auth()` which resolves the caller PID via D-Bus `GetConnectionUnixProcessID`, reads `/proc/PID/exe`, and checks `storage.is_allowed()`. Non-allowlisted callers get prompted via TOFU.
- **Control interface caller verification**: All `ControlAdaptor` methods call `verify_frontend_caller()` which resolves the caller's process exe path and verifies it is the firebox frontend binary.
- **Stream state tracking**: `CapabilityAdaptor` maintains a `streams_` map (protected by `streams_mutex_`) mapping stream IDs to `StreamState` structs. `CreateStream` → `SendMessage` → `ReceiveStream` → `CloseStream` lifecycle is enforced.
- **JSON over D-Bus**: complex nested data (messages, tools, tool calls) is serialized as JSON strings rather than D-Bus structured types. `ControlAdaptor` uses `nlohmann::json` for serialization.
- **Storage thread safety**: All `Storage` public methods acquire `mutex_` before accessing SQLite. `safe_column_text()` prevents NULL pointer dereference from `sqlite3_column_text()`.
- **Credential security**: `credential.hpp/cpp` uses `fork()`/`execvp()` to invoke `systemd-creds` — never `system()` or `popen()`. Credential names are validated with `is_valid_credential_name()` (alphanumeric + dash/underscore).
- **Async subprocess execution**: Frontend uses `GSubprocess` + `g_subprocess_wait_async()` for systemctl operations to avoid blocking the GTK main loop.

### Data Files (`linux/data/`)

- `dbus/`: D-Bus interface XML definitions (Capability, Control) — source of truth for generated adaptor/proxy headers
- `systemd/`: User service unit (Type=dbus, BusName=org.firebox)
- `gsettings/`: GUI persistence schema (window geometry, metrics refresh interval)
- `polkit/`: Authorization policies (authorize-client, manage-service)

### Documentation (`docs/`)

- `backend/`: Protocol specs for Capability, Control, and TOFU interfaces
- `client/`: SDK design docs (connection, chat, embeddings, errors)
- `frontend/`: GUI feature specs (dashboard, settings, routes, allowlist, connections, tray)

## Runtime & Agent Notes

- **Frontend startup**: The GUI (`firebox`) requires a running X11/Wayland display and a valid `DISPLAY` environment variable. When launching manually from a remote or WSL session ensure an X server (or WSLg) is available and `DISPLAY` is set (for example `:0`). The frontend is also packaged as a systemd user service for easy start/stop.
- **DiscoverModels**: The backend exposes a `DiscoverModels` method on the `org.firebox.Control` D-Bus interface which fetches available models live from provider APIs (OpenAI, Anthropic, Gemini). All built-in hardcoded model lists have been removed — providers must be queried at runtime.
- **Provider subtype**: Providers now include a `subtype` (e.g. `openai`, `anthropic`, `gemini`) which determines API endpoints and parsing logic; existing DB entries created before this change may have an empty subtype and should be re-added.
- **Blocking behavior**: `DiscoverModels` currently performs synchronous HTTP requests from the D-Bus handler thread via a private `GMainContext` (`HttpClient::get_sync`). This means frontend calls to `DiscoverModels` are blocking until the provider responds; use a valid API key to avoid long timeouts, or consider making the call asynchronous in the future.
