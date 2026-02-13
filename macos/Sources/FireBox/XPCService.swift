// SPDX-License-Identifier: Apache-2.0
// Fire Box — macOS Native Layer — XPC Service
//
// Exposes a Mach XPC service (`com.firebox.xpc`) that local apps
// can connect to in order to make LLM requests.  The XPC handler
// forwards each request to the Rust core over the local socket.

import Foundation

// MARK: - XPC Protocol

/// The XPC interface offered to client applications.
/// Client apps call these methods via NSXPCConnection.
@objc protocol FireBoxXPCProtocol {
    /// Send a chat completion request.
    ///
    /// - Parameters:
    ///   - appId: Bundle identifier or unique ID of the calling app.
    ///   - appName: Human-readable app name.
    ///   - requestJson: JSON-encoded unified chat request.
    ///   - reply: Called with (responseJson, errorString).  Exactly one is non-nil.
    func chat(
        appId: String,
        appName: String,
        requestJson: String,
        reply: @escaping (String?, String?) -> Void
    )
}

// MARK: - XPC Service Singleton

@MainActor
final class XPCService: NSObject, @preconcurrency NSXPCListenerDelegate {
    static let shared = XPCService()

    private var listener: NSXPCListener?

    /// Start listening for incoming XPC connections.
    func start() {
        // In production this would be a launchd Mach service:
        //   listener = NSXPCListener(machServiceName: "com.firebox.xpc")
        // For development we use an anonymous listener.
        listener = NSXPCListener.anonymous()
        listener?.delegate = self
        listener?.resume()
    }

    // MARK: NSXPCListenerDelegate

    func listener(
        _: NSXPCListener,
        shouldAcceptNewConnection connection: NSXPCConnection
    ) -> Bool {
        let interface = NSXPCInterface(with: FireBoxXPCProtocol.self)
        connection.exportedInterface = interface
        connection.exportedObject = XPCHandler()

        connection.invalidationHandler = {
            // Connection was invalidated (client disconnected).
        }

        connection.resume()
        return true
    }
}

// MARK: - XPC Handler

/// Handles individual XPC calls by forwarding them through `CoreClient`.
final class XPCHandler: NSObject, FireBoxXPCProtocol {
    func chat(
        appId: String,
        appName: String,
        requestJson: String,
        reply: @escaping (String?, String?) -> Void
    ) {
        // Wrap the reply in a Sendable closure — safe because XPC
        // serialises reply invocations on its own queue.
        nonisolated(unsafe) let reply = reply
        // Run synchronously on a background queue — XPC handlers are
        // already called off the main thread by the framework.
        DispatchQueue.global().async {
            do {
                let wrapper: [String: Any] = [
                    "app_id": appId,
                    "app_name": appName,
                    "request": try JSONSerialization.jsonObject(
                        with: Data(requestJson.utf8)),
                ]
                let body = try JSONSerialization.data(withJSONObject: wrapper)
                let responseData = try CoreClient.shared.postRaw(
                    path: "/ipc/v1/chat", body: body)
                let responseString = String(data: responseData, encoding: .utf8)
                reply(responseString, nil)
            } catch {
                reply(nil, error.localizedDescription)
            }
        }
    }
}

// MARK: - CoreClient extension for raw POST

extension CoreClient {
    /// Expose a raw POST helper for XPC forwarding.
    /// `httpRequestSync` is already fully synchronous (POSIX sockets),
    /// so no dispatch / semaphore is necessary.
    func postRaw(path: String, body: Data) throws -> Data {
        try httpRequestSync(method: "POST", path: path, body: body)
    }

    /// Synchronous HTTP-over-UDS (package-internal, used by XPC handler).
    func httpRequestSync(method: String, path: String, body: Data? = nil) throws -> Data {
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw CoreClientError.socketCreationFailed }
        defer { close(fd) }

        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let socketPath = coreSocketPath
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

        var response = Data()
        var readBuf = [UInt8](repeating: 0, count: 16384)
        while true {
            let n = Darwin.read(fd, &readBuf, readBuf.count)
            if n <= 0 { break }
            response.append(readBuf, count: n)
        }

        guard let headerEnd = response.range(of: Data("\r\n\r\n".utf8)) else {
            throw CoreClientError.invalidResponse
        }

        let headerString =
            String(
                data: response[response.startIndex..<headerEnd.lowerBound],
                encoding: .utf8) ?? ""
        let bodyData = response[headerEnd.upperBound...]

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
