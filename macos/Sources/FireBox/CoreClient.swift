// SPDX-License-Identifier: Apache-2.0
// Fire Box — macOS Native Layer — HTTP-over-UDS Client
//
// Talks to the Rust core's IPC server (Axum on a Unix domain socket)
// using raw POSIX sockets and hand-crafted HTTP/1.1 requests.
// This avoids pulling in a heavy networking dependency and works
// well for the small, local request/response pattern.

import Foundation

// MARK: - Errors

enum CoreClientError: Error, LocalizedError {
    case socketCreationFailed
    case connectionFailed(Int32)
    case writeFailed
    case invalidResponse
    case httpError(Int, String)

    var errorDescription: String? {
        switch self {
        case .socketCreationFailed: return "Failed to create Unix socket"
        case .connectionFailed(let code): return "UDS connect failed (errno \(code))"
        case .writeFailed: return "Failed to write to socket"
        case .invalidResponse: return "Invalid HTTP response from core"
        case .httpError(let status, let body): return "HTTP \(status): \(body)"
        }
    }
}

// MARK: - Client

final class CoreClient: @unchecked Sendable {
    static let shared = CoreClient()

    private var socketPath: String = "/tmp/fire-box-ipc.sock"
    private let decoder = JSONDecoder()
    private let encoder = JSONEncoder()

    func configure(socketPath path: String) {
        socketPath = path
    }

    // MARK: Typed API helpers

    func fetchMetrics() async throws -> MetricsSnapshot {
        let data = try httpRequest(method: "GET", path: "/ipc/v1/metrics")
        return try decoder.decode(MetricsSnapshot.self, from: data)
    }

    func fetchApps() async throws -> [AppInfo] {
        let data = try httpRequest(method: "GET", path: "/ipc/v1/apps")
        return try decoder.decode([AppInfo].self, from: data)
    }

    func fetchProviders() async throws -> [ProviderInfo] {
        let data = try httpRequest(method: "GET", path: "/ipc/v1/providers")
        let resp = try decoder.decode(ProviderListResponse.self, from: data)
        return resp.providers
    }

    func fetchSettings() async throws -> ServiceSettings {
        let data = try httpRequest(method: "GET", path: "/ipc/v1/settings")
        let resp = try decoder.decode(SettingsResponse.self, from: data)
        return resp.settings
    }

    func fetchModels() async throws -> [String: [ProviderMapping]] {
        let data = try httpRequest(method: "GET", path: "/ipc/v1/models")
        let resp = try decoder.decode(ModelListResponse.self, from: data)
        return resp.models
    }

    func sendAuthDecision(_ decision: AuthDecision) async throws {
        let body = try encoder.encode(decision)
        _ = try httpRequest(method: "POST", path: "/ipc/v1/auth/decide", body: body)
    }

    func revokeApp(id: String) async throws {
        _ = try httpRequest(method: "POST", path: "/ipc/v1/apps/\(id)/revoke")
    }

    // MARK: SSE event stream

    /// Opens a persistent connection to the SSE endpoint and yields events.
    /// Runs indefinitely; call from a Task and cancel when done.
    func eventStream() -> AsyncStream<IpcEvent> {
        AsyncStream { continuation in
            let task = Task.detached { [socketPath] in
                while !Task.isCancelled {
                    do {
                        try Self.readSSEStream(socketPath: socketPath, continuation: continuation)
                    } catch {
                        // Connection lost — retry after a short delay.
                        try? await Task.sleep(nanoseconds: 2_000_000_000)
                    }
                }
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    /// Read a single SSE session (blocks until the connection drops).
    private static func readSSEStream(
        socketPath: String,
        continuation: AsyncStream<IpcEvent>.Continuation
    ) throws {
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw CoreClientError.socketCreationFailed }
        defer { close(fd) }

        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        withUnsafeMutableBytes(of: &addr.sun_path) { buf in
            let cString = socketPath.utf8CString
            let count = min(cString.count, buf.count)
            for i in 0..<count {
                buf[i] = UInt8(bitPattern: cString[i])
            }
        }

        let connectResult = withUnsafePointer(to: &addr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
                Darwin.connect(fd, sockPtr, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard connectResult == 0 else { throw CoreClientError.connectionFailed(errno) }

        // Send the SSE request.
        let request =
            "GET /ipc/v1/events HTTP/1.1\r\nHost: localhost\r\nAccept: text/event-stream\r\nConnection: keep-alive\r\n\r\n"
        guard request.withCString({ Darwin.write(fd, $0, strlen($0)) }) > 0 else {
            throw CoreClientError.writeFailed
        }

        // Skip HTTP headers (read until \r\n\r\n).
        var headerBuf = Data()
        var singleByte: UInt8 = 0
        while Darwin.read(fd, &singleByte, 1) == 1 {
            headerBuf.append(singleByte)
            if headerBuf.count >= 4 {
                let tail = headerBuf.suffix(4)
                if tail.elementsEqual([0x0D, 0x0A, 0x0D, 0x0A]) { break }
            }
        }

        // Read SSE events line by line.
        var currentEventType = ""
        var currentData = ""
        var lineBuf = Data()

        while !Task.isCancelled {
            var byte: UInt8 = 0
            let n = Darwin.read(fd, &byte, 1)
            if n <= 0 { break }  // Connection closed.

            if byte == 0x0A {  // newline
                let line = String(data: lineBuf, encoding: .utf8) ?? ""
                lineBuf.removeAll()

                if line.isEmpty {
                    // Empty line = end of event.
                    if !currentEventType.isEmpty, !currentData.isEmpty,
                        let jsonData = currentData.data(using: .utf8),
                        let event = IpcEvent.parse(type: currentEventType, data: jsonData)
                    {
                        continuation.yield(event)
                    }
                    currentEventType = ""
                    currentData = ""
                } else if line.hasPrefix("event:") {
                    currentEventType = String(line.dropFirst(6)).trimmingCharacters(
                        in: .whitespaces)
                } else if line.hasPrefix("data:") {
                    if !currentData.isEmpty { currentData += "\n" }
                    currentData += String(line.dropFirst(5)).trimmingCharacters(in: .whitespaces)
                }
            } else if byte != 0x0D {  // skip \r
                lineBuf.append(byte)
            }
        }
    }

    // MARK: Low-level HTTP over UDS

    @discardableResult
    private func httpRequest(
        method: String,
        path: String,
        body: Data? = nil
    ) throws -> Data {
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw CoreClientError.socketCreationFailed }
        defer { close(fd) }

        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        withUnsafeMutableBytes(of: &addr.sun_path) { buf in
            let cString = socketPath.utf8CString
            let count = min(cString.count, buf.count)
            for i in 0..<count {
                buf[i] = UInt8(bitPattern: cString[i])
            }
        }

        let connectResult = withUnsafePointer(to: &addr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
                Darwin.connect(fd, sockPtr, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard connectResult == 0 else {
            throw CoreClientError.connectionFailed(errno)
        }

        // Build and send the HTTP request.
        var header = "\(method) \(path) HTTP/1.1\r\nHost: localhost\r\n"
        if let body = body {
            header += "Content-Type: application/json\r\nContent-Length: \(body.count)\r\n"
        }
        header += "Connection: close\r\n\r\n"

        var requestBytes = Array(header.utf8)
        if let body = body {
            requestBytes.append(contentsOf: body)
        }

        let written = requestBytes.withUnsafeBufferPointer { buf in
            Darwin.write(fd, buf.baseAddress!, buf.count)
        }
        guard written == requestBytes.count else { throw CoreClientError.writeFailed }

        // Read the full response.
        var response = Data()
        var readBuf = [UInt8](repeating: 0, count: 16384)
        while true {
            let n = Darwin.read(fd, &readBuf, readBuf.count)
            if n <= 0 { break }
            response.append(readBuf, count: n)
        }

        // Parse HTTP status and extract body.
        guard let headerEnd = response.range(of: Data("\r\n\r\n".utf8)) else {
            throw CoreClientError.invalidResponse
        }

        let headerString =
            String(
                data: response[response.startIndex..<headerEnd.lowerBound],
                encoding: .utf8) ?? ""
        let bodyData = response[headerEnd.upperBound...]

        // Extract status code from first line: "HTTP/1.1 200 OK"
        let statusCode: Int = {
            let parts = headerString.components(separatedBy: " ")
            guard parts.count >= 2, let code = Int(parts[1]) else { return 0 }
            return code
        }()

        guard (200..<300).contains(statusCode) else {
            throw CoreClientError.httpError(
                statusCode,
                String(data: bodyData, encoding: .utf8) ?? "")
        }

        return Data(bodyData)
    }
}
