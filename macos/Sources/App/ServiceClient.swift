import Foundation

// MARK: - Data Models

struct MetricsSnapshot {
    let windowStartMs: Int64
    let windowEndMs: Int64
    let requestsTotal: Int64
    let requestsFailed: Int64
    let promptTokensTotal: Int64
    let completionTokensTotal: Int64
    let latencyAvgMs: Int64
    let costTotal: Double
}

struct Connection: Identifiable {
    let connectionId: String
    let clientName: String
    let appPath: String
    let requestsCount: Int64
    let connectedAtMs: Int64

    var id: String { connectionId }
}

struct Provider: Identifiable {
    let providerId: String
    let displayName: String
    let providerType: String

    var id: String { providerId }
}

struct RouteTarget {
    let providerId: String
    let modelId: String
}

struct RouteRule: Identifiable {
    let virtualModelId: String
    let displayName: String
    let strategy: String
    let targets: [RouteTarget]

    var id: String { virtualModelId }
}

struct AllowlistEntry: Identifiable {
    let appPath: String
    let displayName: String

    var id: String { appPath }
}

struct OAuthChallenge {
    let deviceCode: String
    let userCode: String
    let verificationUri: String
    let expiresIn: Int64
    let interval: Int64
}

// MARK: - XPC Transport

private let xpcServiceName = "com.firebox.service"

/// Send a request to the Rust XPC service and return the "body" sub-dict on success.
///
/// The Rust service wraps every response as `{ "success": bool, "body": { ... } }`.
/// On failure (success=false or transport error), returns nil.
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
            case let arr as [[String: Any]]:
                let xpcArr = xpc_array_create(nil, 0)
                for dict in arr {
                    let xpcDict = xpc_dictionary_create(nil, nil, 0)
                    for (k, v) in dict {
                        if let s = v as? String {
                            xpc_dictionary_set_string(xpcDict, k, s)
                        }
                    }
                    xpc_array_append_value(xpcArr, xpcDict)
                }
                xpc_dictionary_set_value(msg, key, xpcArr)
            default:
                break
            }
        }

        xpc_connection_send_message_with_reply(conn, msg, nil) { reply in
            defer {
                xpc_connection_cancel(conn)
                xpc_release(conn)
            }

            guard xpc_get_type(reply) == XPC_TYPE_DICTIONARY else {
                continuation.resume(returning: nil)
                return
            }

            let success = xpc_dictionary_get_bool(reply, "success")
            guard success else {
                continuation.resume(returning: nil)
                return
            }

            // Extract the "body" sub-dictionary.
            guard let bodyObj = xpc_dictionary_get_value(reply, "body"),
                  xpc_get_type(bodyObj) == XPC_TYPE_DICTIONARY else {
                // Some commands (e.g. ping) return an empty body dict.
                continuation.resume(returning: [:])
                return
            }

            let body = xpcDictToSwift(bodyObj)
            continuation.resume(returning: body)
        }
    }
}

/// Recursively convert an XPC dictionary to a Swift dictionary.
private func xpcDictToSwift(_ obj: xpc_object_t) -> [String: Any] {
    var result: [String: Any] = [:]
    xpc_dictionary_apply(obj) { key, value in
        let k = String(cString: key)
        let t = xpc_get_type(value)
        if t == XPC_TYPE_STRING {
            result[k] = String(cString: xpc_string_get_string_ptr(value))
        } else if t == XPC_TYPE_INT64 {
            result[k] = xpc_int64_get_value(value)
        } else if t == XPC_TYPE_DOUBLE {
            result[k] = xpc_double_get_value(value)
        } else if t == XPC_TYPE_BOOL {
            result[k] = xpc_bool_get_value(value)
        } else if t == XPC_TYPE_ARRAY {
            result[k] = xpcArrayToSwift(value)
        } else if t == XPC_TYPE_DICTIONARY {
            result[k] = xpcDictToSwift(value)
        }
        return true
    }
    return result
}

/// Convert an XPC array to a Swift array of dictionaries.
private func xpcArrayToSwift(_ arr: xpc_object_t) -> [[String: Any]] {
    var result: [[String: Any]] = []
    let count = xpc_array_get_count(arr)
    for i in 0..<count {
        let elem = xpc_array_get_value(arr, i)
        if xpc_get_type(elem) == XPC_TYPE_DICTIONARY {
            result.append(xpcDictToSwift(elem))
        }
    }
    return result
}

/// Send a request and only check if it succeeded (ignoring body).
private func xpcSendOk(_ request: [String: Any]) async -> Bool {
    await xpcSend(request) != nil
}

// MARK: - Service Client

final class ServiceClient: Sendable {
    static let shared = ServiceClient()
    private init() {}

    // MARK: - Status

    func checkServiceStatus() async -> Bool {
        await xpcSendOk(["cmd": "ping"])
    }

    // MARK: - Metrics

    func getMetricsSnapshot() async -> MetricsSnapshot? {
        guard let b = await xpcSend(["cmd": "get_metrics_snapshot"]) else { return nil }
        return MetricsSnapshot(
            windowStartMs: b["window_start_ms"] as? Int64 ?? 0,
            windowEndMs: b["window_end_ms"] as? Int64 ?? 0,
            requestsTotal: b["requests_total"] as? Int64 ?? 0,
            requestsFailed: b["requests_failed"] as? Int64 ?? 0,
            promptTokensTotal: b["prompt_tokens_total"] as? Int64 ?? 0,
            completionTokensTotal: b["completion_tokens_total"] as? Int64 ?? 0,
            latencyAvgMs: b["latency_avg_ms"] as? Int64 ?? 0,
            costTotal: b["cost_total"] as? Double ?? 0.0
        )
    }

    // MARK: - Connections

    func listConnections() async -> [Connection] {
        guard let b = await xpcSend(["cmd": "list_connections"]),
              let arr = b["connections"] as? [[String: Any]] else { return [] }
        return arr.compactMap { d in
            guard let cid = d["connection_id"] as? String else { return nil }
            return Connection(
                connectionId: cid,
                clientName: d["client_name"] as? String ?? "",
                appPath: d["app_path"] as? String ?? "",
                requestsCount: d["requests_count"] as? Int64 ?? 0,
                connectedAtMs: d["connected_at_ms"] as? Int64 ?? 0
            )
        }
    }

    // MARK: - Providers

    func listProviders() async -> [Provider] {
        guard let b = await xpcSend(["cmd": "list_providers"]),
              let arr = b["providers"] as? [[String: Any]] else { return [] }
        return arr.compactMap { d in
            guard let pid = d["provider_id"] as? String else { return nil }
            return Provider(
                providerId: pid,
                displayName: d["display_name"] as? String ?? pid,
                providerType: d["provider_type"] as? String ?? "unknown"
            )
        }
    }

    func addApiKeyProvider(name: String, providerType: String, apiKey: String, baseUrl: String? = nil) async -> Bool {
        var req: [String: Any] = [
            "cmd": "add_api_key_provider",
            "name": name,
            "provider_type": providerType,
            "api_key": apiKey,
        ]
        if let url = baseUrl, !url.isEmpty { req["base_url"] = url }
        return await xpcSendOk(req)
    }

    func addOAuthProvider(name: String, providerType: String) async -> OAuthChallenge? {
        guard let b = await xpcSend(["cmd": "add_oauth_provider", "name": name, "provider_type": providerType]) else {
            return nil
        }
        guard let dc = b["device_code"] as? String,
              let uc = b["user_code"] as? String,
              let uri = b["verification_uri"] as? String else { return nil }
        return OAuthChallenge(
            deviceCode: dc,
            userCode: uc,
            verificationUri: uri,
            expiresIn: b["expires_in"] as? Int64 ?? 300,
            interval: b["interval"] as? Int64 ?? 5
        )
    }

    func completeOAuth(providerType: String, deviceCode: String) async -> Bool {
        await xpcSendOk(["cmd": "complete_oauth", "provider_type": providerType, "device_code": deviceCode])
    }

    func addLocalProvider(name: String, modelPath: String) async -> Bool {
        await xpcSendOk(["cmd": "add_local_provider", "name": name, "model_path": modelPath])
    }

    func removeProvider(providerId: String) async -> Bool {
        await xpcSendOk(["cmd": "delete_provider", "provider_id": providerId])
    }

    // MARK: - Routes

    func listRouteRules() async -> [RouteRule] {
        guard let b = await xpcSend(["cmd": "get_route_rules"]),
              let rules = b["rules"] as? [[String: Any]] else { return [] }
        return rules.compactMap { d in
            guard let vmid = d["virtual_model_id"] as? String else { return nil }
            let targets = (d["targets"] as? [[String: Any]] ?? []).compactMap { t -> RouteTarget? in
                guard let pid = t["provider_id"] as? String,
                      let mid = t["model_id"] as? String else { return nil }
                return RouteTarget(providerId: pid, modelId: mid)
            }
            return RouteRule(
                virtualModelId: vmid,
                displayName: d["display_name"] as? String ?? vmid,
                strategy: d["strategy"] as? String ?? "failover",
                targets: targets
            )
        }
    }

    func addRouteRule(_ rule: RouteRule) async -> Bool {
        let targets = rule.targets.map { ["provider_id": $0.providerId, "model_id": $0.modelId] }
        return await xpcSendOk([
            "cmd": "set_route_rules",
            "virtual_model_id": rule.virtualModelId,
            "display_name": rule.displayName,
            "strategy": rule.strategy,
            "targets": targets,
        ])
    }

    func removeRouteRule(virtualModelId: String) async -> Bool {
        await xpcSendOk(["cmd": "delete_route", "virtual_model_id": virtualModelId])
    }

    func updateRouteRule(_ rule: RouteRule) async -> Bool {
        await addRouteRule(rule)
    }

    // MARK: - Allowlist

    func getAllowlist() async -> [AllowlistEntry] {
        guard let b = await xpcSend(["cmd": "get_allowlist"]),
              let apps = b["apps"] as? [[String: Any]] else { return [] }
        return apps.compactMap { d in
            guard let path = d["app_path"] as? String else { return nil }
            return AllowlistEntry(
                appPath: path,
                displayName: d["display_name"] as? String ?? path
            )
        }
    }

    func removeFromAllowlist(appPath: String) async -> Bool {
        await xpcSendOk(["cmd": "remove_from_allowlist", "app_path": appPath])
    }
}
