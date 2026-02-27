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

// MARK: - XPC Transport

private let xpcServiceName = "com.firebox.service"

/// Send a request to the Rust XPC service and return the "body" reply dict.
private func xpcSend(_ request: [String: Any]) async -> [String: Any]? {
    await withCheckedContinuation { continuation in
        let conn = xpc_connection_create_mach_service(xpcServiceName, nil, 0)
        xpc_connection_set_event_handler(conn) { _ in }
        xpc_connection_resume(conn)

        let msg = xpc_dictionary_create(nil, nil, 0)
        for (key, value) in request {
            switch value {
            case let s as String:
                xpc_dictionary_set_string(msg, key, s)
            case let i as Int64:
                xpc_dictionary_set_int64(msg, key, i)
            case let b as Bool:
                xpc_dictionary_set_bool(msg, key, b)
            case let arr as [[String: String]]:
                let xarr = xpc_array_create(nil, 0)
                for item in arr {
                    let xitem = xpc_dictionary_create(nil, nil, 0)
                    for (k, v) in item { xpc_dictionary_set_string(xitem, k, v) }
                    xpc_array_append_value(xarr, xitem)
                    xpc_release(xitem)
                }
                xpc_dictionary_set_value(msg, key, xarr)
                xpc_release(xarr)
            default:
                break
            }
        }

        xpc_connection_send_message_with_reply(conn, msg, nil) { reply in
            xpc_connection_cancel(conn)
            guard xpc_get_type(reply) == XPC_TYPE_DICTIONARY,
                  let body = xpc_dictionary_get_value(reply, "body"),
                  xpc_get_type(body) == XPC_TYPE_DICTIONARY
            else {
                continuation.resume(returning: nil)
                return
            }
            continuation.resume(returning: xpcDictToSwift(body))
        }
        xpc_release(msg)
    }
}

private func xpcDictToSwift(_ obj: xpc_object_t) -> [String: Any] {
    var result: [String: Any] = [:]
    xpc_dictionary_apply(obj) { key, value in
        let k = String(cString: key)
        let t = xpc_get_type(value)
        if t == XPC_TYPE_STRING {
            if let ptr = xpc_string_get_string_ptr(value) { result[k] = String(cString: ptr) }
        } else if t == XPC_TYPE_INT64 {
            result[k] = xpc_int64_get_value(value)
        } else if t == XPC_TYPE_BOOL {
            result[k] = xpc_bool_get_value(value)
        } else if t == XPC_TYPE_ARRAY {
            var arr: [[String: Any]] = []
            xpc_array_apply(value) { _, item in
                if xpc_get_type(item) == XPC_TYPE_DICTIONARY {
                    arr.append(xpcDictToSwift(item))
                }
                return true
            }
            result[k] = arr
        } else if t == XPC_TYPE_DICTIONARY {
            result[k] = xpcDictToSwift(value)
        }
        return true
    }
    return result
}

// MARK: - Service Client

actor ServiceClient {
    static let shared = ServiceClient()

    private init() {}

    func checkServiceStatus() async -> Bool {
        let reply = await xpcSend(["cmd": "ping"])
        return reply?["success"] as? Bool ?? false
    }

    func getMetricsSnapshot() async -> MetricsSnapshot? {
        guard let r = await xpcSend(["cmd": "get_metrics"]),
              r["success"] as? Bool == true else { return nil }

        let costMicrocents = r["cost_total_microcents"] as? Int64 ?? 0
        return MetricsSnapshot(
            timestampMs: r["window_end_ms"] as? Int64 ?? 0,
            totalRequests: r["requests_total"] as? Int64 ?? 0,
            totalTokensInput: r["prompt_tokens_total"] as? Int64 ?? 0,
            totalTokensOutput: r["completion_tokens_total"] as? Int64 ?? 0,
            totalCost: Double(costMicrocents) / 1_000_000.0,
            activeConnections: 0
        )
    }

    func listConnections() async -> [Connection] {
        guard let r = await xpcSend(["cmd": "list_connections"]),
              r["success"] as? Bool == true,
              let conns = r["connections"] as? [[String: Any]] else { return [] }

        return conns.compactMap { d in
            guard let cid = d["connection_id"] as? String,
                  let name = d["client_name"] as? String else { return nil }
            return Connection(
                connectionId: cid,
                programName: name,
                programPath: "",
                connectedAtMs: 0,
                lastActivityMs: 0,
                requestCount: d["requests_count"] as? Int64 ?? 0
            )
        }
    }

    func listProviders() async -> [Provider] {
        guard let r = await xpcSend(["cmd": "list_providers"]),
              r["success"] as? Bool == true,
              let provs = r["providers"] as? [[String: Any]] else { return [] }

        return provs.compactMap { d in
            guard let pid = d["provider_id"] as? String,
                  let name = d["name"] as? String else { return nil }
            let typeRaw = Int(d["type"] as? Int64 ?? 1)
            let ptype = ProviderType(rawValue: typeRaw) ?? .apiKey
            let baseUrl = d["base_url"] as? String
            return Provider(
                providerId: pid,
                name: name,
                type: ptype,
                baseUrl: baseUrl?.isEmpty == false ? baseUrl : nil,
                localPath: nil
            )
        }
    }

    func listModels() async -> [Model] {
        // Models are derived from providers; no dedicated XPC command yet.
        return []
    }

    func listRouteRules() async -> [RouteRule] {
        guard let r = await xpcSend(["cmd": "list_route_rules"]),
              r["success"] as? Bool == true,
              let rules = r["rules"] as? [[String: Any]] else { return [] }

        return rules.compactMap { d in
            guard let vmid = d["virtual_model_id"] as? String else { return nil }
            let targets = (d["targets"] as? [[String: Any]] ?? []).compactMap { t -> RouteTarget? in
                guard let pid = t["provider_id"] as? String,
                      let mid = t["model_id"] as? String else { return nil }
                return RouteTarget(providerId: pid, modelId: mid)
            }
            return RouteRule(alias: vmid, targets: targets)
        }
    }

    func addProvider(_ provider: Provider) async -> Bool {
        var req: [String: Any] = [
            "cmd": "add_api_key_provider",
            "name": provider.name,
            "provider_type": providerTypeSlug(provider.type),
        ]
        if let url = provider.baseUrl { req["base_url"] = url }
        let r = await xpcSend(req)
        return r?["success"] as? Bool ?? false
    }

    func removeProvider(providerId: String) async -> Bool {
        let r = await xpcSend(["cmd": "delete_provider", "provider_id": providerId])
        return r?["success"] as? Bool ?? false
    }

    func addRouteRule(_ rule: RouteRule) async -> Bool {
        let targets = rule.targets.map { ["provider_id": $0.providerId, "model_id": $0.modelId] }
        let r = await xpcSend([
            "cmd": "set_route_rule",
            "virtual_model_id": rule.alias,
            "display_name": rule.alias,
            "targets": targets,
        ])
        return r?["success"] as? Bool ?? false
    }

    func removeRouteRule(alias: String) async -> Bool {
        let r = await xpcSend(["cmd": "delete_route_rule", "virtual_model_id": alias])
        return r?["success"] as? Bool ?? false
    }

    func updateRouteRule(_ rule: RouteRule) async -> Bool {
        return await addRouteRule(rule)
    }

    private func providerTypeSlug(_ type: ProviderType) -> String {
        switch type {
        case .apiKey: return "openai"
        case .oauth:  return "copilot"
        case .local:  return "llamacpp"
        }
    }
}
