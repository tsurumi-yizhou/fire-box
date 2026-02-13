# macos/Sources/FireBox/Views/

> ⚠️ **Please update me promptly.**

SwiftUI view components for the macOS menu-bar popover. All views observe `FireBoxState` (injected via `@Environment`).

## Views

| File | Description |
|------|-------------|
| `DashboardView.swift` | Main popover view — header with status dot, segmented tab picker (overview / apps / providers), footer with quit button. Hosts `MetricsView`, `AppsView`, and `ProvidersView` as tabs. Presents `ApprovalView` and `OAuthPromptView` as modal sheets |
| `MetricsView.swift` | Overview tab — global counter grid (total requests, tokens, active connections, uptime) + per-provider and per-model metrics breakdowns |
| `AppsView.swift` | Apps tab — list of registered client applications with authorize/revoke actions |
| `ProvidersView.swift` | Providers tab — configured LLM providers with type icons, endpoint info, and per-provider metrics |
| `ApprovalView.swift` | Modal sheet presented on `auth_required` SSE event — shows app details and approve/deny buttons |
| `OAuthPromptView.swift` | Modal sheet presented on `oauth_open_url` SSE event — shows OAuth URL and "Open in Browser" button |
