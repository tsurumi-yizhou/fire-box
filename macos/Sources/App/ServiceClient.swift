import Foundation

// MARK: - Data Models

struct MetricsSnapshot: Codable {
    let timestampMs: Int64
    let totalRequests: Int64
    let totalTokensInput: Int64
    let totalTokensOutput: Int64
    let totalCost: Double
    let activeConnections: Int32
}

struct Connection: Codable, Identifiable {
    let connectionId: String
    let programName: String
    let programPath: String
    let connectedAtMs: Int64
    let lastActivityMs: Int64
    let requestCount: Int64

    var id: String { connectionId }
}

enum ProviderType: Int, Codable {
    case apiKey = 1
    case oauth = 2
    case local = 3
}

struct Provider: Codable, Identifiable {
    let providerId: String
    let name: String
    let type: ProviderType
    let baseUrl: String?
    let localPath: String?

    var id: String { providerId }
}

struct Model: Codable, Identifiable {
    let modelId: String
    let providerId: String?
    let contextWindow: Int32?
    let enabled: Bool
    let capabilityChat: Bool?
    let capabilityTools: Bool?
    let capabilityVision: Bool?
    let capabilityEmbeddings: Bool?
    let capabilityStreaming: Bool?
    let costInputPerMillionTokens: Double?
    let costOutputPerMillionTokens: Double?
    let costCacheReadPerMillionTokens: Double?
    let costCacheWritePerMillionTokens: Double?

    var id: String { modelId }
}

struct RouteTarget: Codable {
    let providerId: String
    let modelId: String
}

struct RouteRule: Codable, Identifiable {
    let alias: String
    let targets: [RouteTarget]

    var id: String { alias }
}

// MARK: - Service Client

actor ServiceClient {
    static let shared = ServiceClient()

    private init() {}

    // TODO: Implement actual XPC connection to the backend service
    // For now, return mock data

    func checkServiceStatus() async -> Bool {
        // Mock implementation
        return true
    }

    func getMetricsSnapshot() async -> MetricsSnapshot? {
        // Mock implementation
        return MetricsSnapshot(
            timestampMs: Int64(Date().timeIntervalSince1970 * 1000),
            totalRequests: 1234,
            totalTokensInput: 567890,
            totalTokensOutput: 234567,
            totalCost: 12.34,
            activeConnections: 3
        )
    }

    func listConnections() async -> [Connection] {
        // Mock implementation
        return [
            Connection(
                connectionId: "conn-1",
                programName: "VSCode",
                programPath: "/Applications/Visual Studio Code.app",
                connectedAtMs: Int64(Date().timeIntervalSince1970 * 1000) - 3600000,
                lastActivityMs: Int64(Date().timeIntervalSince1970 * 1000),
                requestCount: 42
            )
        ]
    }

    func listProviders() async -> [Provider] {
        // Mock implementation
        return [
            Provider(
                providerId: "openai",
                name: "OpenAI",
                type: .apiKey,
                baseUrl: "https://api.openai.com/v1",
                localPath: nil
            ),
            Provider(
                providerId: "anthropic",
                name: "Anthropic",
                type: .apiKey,
                baseUrl: "https://api.anthropic.com",
                localPath: nil
            )
        ]
    }

    func listModels() async -> [Model] {
        // Mock implementation
        return [
            Model(
                modelId: "gpt-4",
                providerId: "openai",
                contextWindow: 8192,
                enabled: true,
                capabilityChat: true,
                capabilityTools: true,
                capabilityVision: false,
                capabilityEmbeddings: false,
                capabilityStreaming: true,
                costInputPerMillionTokens: 30.0,
                costOutputPerMillionTokens: 60.0,
                costCacheReadPerMillionTokens: nil,
                costCacheWritePerMillionTokens: nil
            )
        ]
    }

    func listRouteRules() async -> [RouteRule] {
        // Mock implementation
        return [
            RouteRule(
                alias: "default",
                targets: [
                    RouteTarget(providerId: "openai", modelId: "gpt-4")
                ]
            )
        ]
    }

    func addProvider(_ provider: Provider) async -> Bool {
        // TODO: Implement XPC call
        return true
    }

    func removeProvider(providerId: String) async -> Bool {
        // TODO: Implement XPC call
        return true
    }

    func addRouteRule(_ rule: RouteRule) async -> Bool {
        // TODO: Implement XPC call
        return true
    }

    func removeRouteRule(alias: String) async -> Bool {
        // TODO: Implement XPC call
        return true
    }

    func updateRouteRule(_ rule: RouteRule) async -> Bool {
        // TODO: Implement XPC call
        return true
    }
}
