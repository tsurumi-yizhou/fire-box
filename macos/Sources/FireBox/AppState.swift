// SPDX-License-Identifier: Apache-2.0
// Fire Box — macOS Native Layer — Observable Application State
//
// Centralised @Observable state object injected into the SwiftUI
// environment.  Polls the Rust core for metrics and app lists,
// and listens to the SSE event stream for real-time updates.

import Foundation
import SwiftUI

@MainActor @Observable
final class FireBoxState {
    // MARK: Published state

    var metrics: MetricsSnapshot = .empty
    var apps: [AppInfo] = []
    var providers: [ProviderInfo] = []
    var pendingApproval: PendingApproval?
    var oauthPrompt: OAuthPrompt?
    var isConnected = false
    var lastError: String?

    // MARK: Pending user interactions

    struct PendingApproval: Identifiable {
        let id: String  // request_id
        let appId: String
        let appName: String
        let requestedModels: [String]
    }

    struct OAuthPrompt: Identifiable {
        var id: String { provider }
        let provider: String
        let url: String
        let userCode: String
    }

    // MARK: Internal

    private var eventTask: Task<Void, Never>?
    private var pollTask: Task<Void, Never>?

    /// Start all background activities (SSE + periodic polling).
    func start() {
        startEventStream()
        startPolling()
    }

    func stop() {
        eventTask?.cancel()
        pollTask?.cancel()
    }

    // MARK: - Service control

    /// Start the Rust core on a background thread. Non-blocking.
    func startService() {
        Thread.detachNewThread {
            let rc = fire_box_start()
            if rc != 0 && rc != 2 {
                fputs("core exited with code \(rc)\n", stderr)
            }
        }
        // Kick off a refresh shortly so the UI updates
        Task { @MainActor in
            try? await Task.sleep(nanoseconds: 1_000_000_000)
            await refresh()
        }
    }

    /// Stop the Rust core. Returns the exit code.
    @discardableResult
    func stopService() -> Int32 {
        let rc = fire_box_stop()
        Task { @MainActor in
            try? await Task.sleep(nanoseconds: 500_000_000)
            await refresh()
        }
        return rc
    }

    // MARK: - SSE

    private func startEventStream() {
        eventTask?.cancel()
        eventTask = Task { [weak self] in
            for await event in CoreClient.shared.eventStream() {
                guard let self else { return }
                switch event {
                case .authRequired(let rid, let appId, let appName, let models):
                    self.pendingApproval = PendingApproval(
                        id: rid, appId: appId, appName: appName,
                        requestedModels: models)

                case .metricsUpdate(let snapshot):
                    self.metrics = snapshot

                case .requestLog:
                    // Trigger a light refresh — the dashboard view reacts to `metrics`.
                    break

                case .oauthOpenUrl(let provider, let url, let userCode):
                    self.oauthPrompt = OAuthPrompt(
                        provider: provider, url: url, userCode: userCode)
                }
            }
        }
    }

    // MARK: - Periodic polling

    private func startPolling() {
        pollTask?.cancel()
        pollTask = Task { [weak self] in
            while !Task.isCancelled {
                await self?.refresh()
                try? await Task.sleep(nanoseconds: 5_000_000_000)  // 5 s
            }
        }
    }

    @MainActor
    func refresh() async {
        do {
            let m = try await CoreClient.shared.fetchMetrics()
            metrics = m
            isConnected = true

            let a = try await CoreClient.shared.fetchApps()
            apps = a

            let p = try await CoreClient.shared.fetchProviders()
            providers = p

            lastError = nil
        } catch {
            isConnected = false
            lastError = error.localizedDescription
        }
    }

    // MARK: - Actions

    @MainActor
    func approve(requestId: String, appId: String, models: [String]) async {
        do {
            try await CoreClient.shared.sendAuthDecision(
                AuthDecision(appId: appId, approved: true, allowedModels: models))
            pendingApproval = nil
            await refresh()
        } catch {
            lastError = error.localizedDescription
        }
    }

    @MainActor
    func deny(requestId: String, appId: String) async {
        do {
            try await CoreClient.shared.sendAuthDecision(
                AuthDecision(appId: appId, approved: false, allowedModels: []))
            pendingApproval = nil
        } catch {
            lastError = error.localizedDescription
        }
    }

    @MainActor
    func revokeApp(id: String) async {
        do {
            try await CoreClient.shared.revokeApp(id: id)
            await refresh()
        } catch {
            lastError = error.localizedDescription
        }
    }
}
