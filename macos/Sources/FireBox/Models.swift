// SPDX-License-Identifier: Apache-2.0
// Fire Box — macOS Native Layer — Data Models
//
// Mirror the JSON structures used by the Rust core's IPC API so that
// Swift can decode them with Codable.

import Foundation

// MARK: - Metrics

struct EntityMetrics: Codable, Identifiable {
    var id: String { name }
    let name: String
    let requests: UInt64
    let inputTokens: UInt64
    let outputTokens: UInt64
    let errors: UInt64

    enum CodingKeys: String, CodingKey {
        case name
        case requests
        case inputTokens = "input_tokens"
        case outputTokens = "output_tokens"
        case errors
    }
}

struct MetricsSnapshot: Codable {
    let totalRequests: UInt64
    let activeConnections: UInt64
    let totalInputTokens: UInt64
    let totalOutputTokens: UInt64
    let totalErrors: UInt64
    let perModel: [String: EntityMetricsRaw]
    let perProvider: [String: EntityMetricsRaw]
    let perApp: [String: EntityMetricsRaw]

    enum CodingKeys: String, CodingKey {
        case totalRequests = "total_requests"
        case activeConnections = "active_connections"
        case totalInputTokens = "total_input_tokens"
        case totalOutputTokens = "total_output_tokens"
        case totalErrors = "total_errors"
        case perModel = "per_model"
        case perProvider = "per_provider"
        case perApp = "per_app"
    }

    /// Convert raw dictionaries into identified arrays for SwiftUI lists.
    func modelMetrics() -> [EntityMetrics] {
        perModel.map {
            EntityMetrics(
                name: $0.key, requests: $0.value.requests,
                inputTokens: $0.value.inputTokens,
                outputTokens: $0.value.outputTokens,
                errors: $0.value.errors)
        }
        .sorted { $0.requests > $1.requests }
    }

    func providerMetrics() -> [EntityMetrics] {
        perProvider.map {
            EntityMetrics(
                name: $0.key, requests: $0.value.requests,
                inputTokens: $0.value.inputTokens,
                outputTokens: $0.value.outputTokens,
                errors: $0.value.errors)
        }
        .sorted { $0.requests > $1.requests }
    }

    func appMetrics() -> [EntityMetrics] {
        perApp.map {
            EntityMetrics(
                name: $0.key, requests: $0.value.requests,
                inputTokens: $0.value.inputTokens,
                outputTokens: $0.value.outputTokens,
                errors: $0.value.errors)
        }
        .sorted { $0.requests > $1.requests }
    }

    static let empty = MetricsSnapshot(
        totalRequests: 0, activeConnections: 0,
        totalInputTokens: 0, totalOutputTokens: 0, totalErrors: 0,
        perModel: [:], perProvider: [:], perApp: [:]
    )
}

struct EntityMetricsRaw: Codable {
    let requests: UInt64
    let inputTokens: UInt64
    let outputTokens: UInt64
    let errors: UInt64

    enum CodingKeys: String, CodingKey {
        case requests
        case inputTokens = "input_tokens"
        case outputTokens = "output_tokens"
        case errors
    }
}

// MARK: - Apps / Auth

struct AppInfo: Codable, Identifiable {
    var id: String { appId }
    let appId: String
    let appName: String
    let authorized: Bool
    let allowedModels: [String]
    let createdAt: UInt64
    let lastUsed: UInt64
    let totalRequests: UInt64

    enum CodingKeys: String, CodingKey {
        case appId = "app_id"
        case appName = "app_name"
        case authorized
        case allowedModels = "allowed_models"
        case createdAt = "created_at"
        case lastUsed = "last_used"
        case totalRequests = "total_requests"
    }
}

struct AuthDecision: Codable {
    let appId: String
    let approved: Bool
    let allowedModels: [String]

    enum CodingKeys: String, CodingKey {
        case appId = "app_id"
        case approved
        case allowedModels = "allowed_models"
    }
}

// MARK: - Providers

struct ProviderInfo: Codable, Identifiable {
    var id: String { tag }
    let tag: String
    let type: String
    let baseUrl: String?
    let oauthCredsPath: String?

    enum CodingKeys: String, CodingKey {
        case tag
        case type
        case baseUrl = "base_url"
        case oauthCredsPath = "oauth_creds_path"
    }
}

struct ProviderListResponse: Codable {
    let providers: [ProviderInfo]
}

// MARK: - Models

struct ModelListResponse: Codable {
    let models: [String: [ProviderMapping]]
}

struct ProviderMapping: Codable {
    let provider: String
    let modelId: String

    enum CodingKeys: String, CodingKey {
        case provider
        case modelId = "model_id"
    }
}

// MARK: - Settings

struct ServiceSettings: Codable {
    let logLevel: String
    let ipcPipe: String

    enum CodingKeys: String, CodingKey {
        case logLevel = "log_level"
        case ipcPipe = "ipc_pipe"
    }
}

struct SettingsResponse: Codable {
    let settings: ServiceSettings
}

// MARK: - IPC Events (SSE)

enum IpcEvent {
    case authRequired(requestId: String, appId: String, appName: String, requestedModels: [String])
    case metricsUpdate(MetricsSnapshot)
    case requestLog(
        appId: String?, model: String, provider: String,
        inputTokens: UInt64, outputTokens: UInt64, success: Bool)
    case oauthOpenUrl(provider: String, url: String, userCode: String)

    /// Parse a Server-Sent Event (type + JSON data) into a typed event.
    static func parse(type: String, data: Data) -> IpcEvent? {
        let decoder = JSONDecoder()
        switch type {
        case "auth_required":
            struct Payload: Codable {
                let request_id: String
                let app_id: String
                let app_name: String
                let requested_models: [String]
            }
            guard let p = try? decoder.decode(Payload.self, from: data) else { return nil }
            return .authRequired(
                requestId: p.request_id, appId: p.app_id,
                appName: p.app_name, requestedModels: p.requested_models)

        case "metrics_update":
            struct Payload: Codable { let metrics: MetricsSnapshot }
            guard let p = try? decoder.decode(Payload.self, from: data) else { return nil }
            return .metricsUpdate(p.metrics)

        case "request_log":
            struct Payload: Codable {
                let app_id: String?
                let model: String
                let provider: String
                let input_tokens: UInt64
                let output_tokens: UInt64
                let success: Bool
            }
            guard let p = try? decoder.decode(Payload.self, from: data) else { return nil }
            return .requestLog(
                appId: p.app_id, model: p.model, provider: p.provider,
                inputTokens: p.input_tokens, outputTokens: p.output_tokens,
                success: p.success)

        case "oauth_open_url":
            struct Payload: Codable {
                let provider: String
                let url: String
                let user_code: String
            }
            guard let p = try? decoder.decode(Payload.self, from: data) else { return nil }
            return .oauthOpenUrl(provider: p.provider, url: p.url, userCode: p.user_code)

        default:
            return nil
        }
    }
}
