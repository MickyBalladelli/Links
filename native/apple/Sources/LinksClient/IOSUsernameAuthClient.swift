import Foundation

public enum IOSUsernameAuthError: Error {
    case invalidEndpoint
    case invalidHandle
    case invalidRequest
    case invalidResponse
    case serviceRejected
    case serverRejected(statusCode: Int)
    case conflict
    case rateLimited(retryAfterSeconds: Int?)
}

public struct IOSUsernameAuthSession: Sendable {
    public let accessToken: String
    public let expiresAtMs: UInt64
    public let userID: String
    public let deviceID: String
    public let handle: String
    public let mlsCredential: Data

    fileprivate init(object: [String: Any]) throws {
        guard let session = object["session"] as? [String: Any],
              let accessToken = session["access_token"] as? String,
              !accessToken.isEmpty,
              Self.decode(accessToken, count: 32) != nil,
              let expiresAt = session["expires_at_ms"] as? NSNumber,
              expiresAt.int64Value > 0,
              let userID = session["user_id"] as? String,
              IOSClient.isCanonicalUUID(userID),
              let deviceID = session["device_id"] as? String,
              IOSClient.isCanonicalUUID(deviceID),
              let handle = object["handle"] as? String,
              Self.isCanonicalHandle(handle),
              let credential = object["mls_credential"] as? String,
              let mlsCredential = Self.decode(credential, maximum: 1024),
              !mlsCredential.isEmpty else {
            throw IOSUsernameAuthError.invalidResponse
        }
        self.accessToken = accessToken
        self.expiresAtMs = UInt64(expiresAt.int64Value)
        self.userID = userID
        self.deviceID = deviceID
        self.handle = handle
        self.mlsCredential = mlsCredential
    }

    fileprivate static func isCanonicalHandle(_ handle: String) -> Bool {
        let bytes = Array(handle.utf8)
        guard bytes.count >= 3, bytes.count <= 32,
              let first = bytes.first, (97...122).contains(first) else { return false }
        return bytes.dropFirst().allSatisfy {
            (97...122).contains($0) || (48...57).contains($0) || $0 == 95
        }
    }

    fileprivate static func decode(_ value: String, count: Int) -> Data? {
        guard let decoded = decode(value, maximum: count), decoded.count == count else {
            return nil
        }
        return decoded
    }

    fileprivate static func decode(_ value: String, maximum: Int) -> Data? {
        guard !value.isEmpty else { return nil }
        let padded = value.replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        guard padded.count % 4 != 1 else { return nil }
        let withPadding = padded + String(repeating: "=", count: (4 - padded.count % 4) % 4)
        guard let decoded = Data(base64Encoded: withPadding), decoded.count <= maximum,
              Self.encode(decoded) == value else { return nil }
        return decoded
    }

    fileprivate static func encode(_ data: Data) -> String {
        data.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .trimmingCharacters(in: CharacterSet(charactersIn: "="))
    }
}

public struct IOSDirectoryDevice: Sendable {
    public let deviceID: String
    public let mlsNodeID: String
    public let identityPublicKey: Data
    public let mlsCredential: Data

    fileprivate init(object: [String: Any]) throws {
        guard let deviceID = object["device_id"] as? String,
              IOSClient.isCanonicalUUID(deviceID),
              let mlsNodeID = object["mls_node_id"] as? String,
              IOSClient.isCanonicalUUID(mlsNodeID),
              let publicKey = object["identity_public_key"] as? String,
              let identityPublicKey = IOSUsernameAuthSession.decode(publicKey, count: 32),
              let credential = object["mls_credential"] as? String,
              let mlsCredential = IOSUsernameAuthSession.decode(credential, maximum: 1024),
              !mlsCredential.isEmpty else {
            throw IOSUsernameAuthError.invalidResponse
        }
        self.deviceID = deviceID
        self.mlsNodeID = mlsNodeID
        self.identityPublicKey = identityPublicKey
        self.mlsCredential = mlsCredential
    }
}

public struct IOSUsernameDirectory: Sendable {
    public let handle: String
    public let userID: String
    public let devices: [IOSDirectoryDevice]

    fileprivate init(object: [String: Any]) throws {
        guard let handle = object["handle"] as? String,
              IOSUsernameAuthSession.isCanonicalHandle(handle),
              let userID = object["user_id"] as? String,
              IOSClient.isCanonicalUUID(userID) else {
            throw IOSUsernameAuthError.invalidResponse
        }
        self.handle = handle
        self.userID = userID
        let deviceObjects = object["devices"] as? [[String: Any]] ?? []
        self.devices = try deviceObjects.map(IOSDirectoryDevice.init)
    }
}

/// Local-development account API. Bearer tokens stay in memory and are sent
/// only in the Authorization header for authenticated device registration.
public final class IOSUsernameAuthClient: Sendable {
    private static let maximumBodyBytes = 16 * 1024
    private let baseURL: URL
    private let urlSession: URLSession

    public init(baseURL: URL, urlSession: URLSession = .shared) throws {
        guard baseURL.user == nil,
              baseURL.query == nil,
              baseURL.fragment == nil,
              baseURL.path.isEmpty || baseURL.path == "/",
              let scheme = baseURL.scheme?.lowercased(),
              let host = baseURL.host?.lowercased(),
              scheme == "https" || (scheme == "http" && Self.isLoopback(host)) else {
            throw IOSUsernameAuthError.invalidEndpoint
        }
        self.baseURL = baseURL
        self.urlSession = urlSession
    }

    public static func validateHandle(_ handle: String) throws {
        guard IOSUsernameAuthSession.isCanonicalHandle(handle) else {
            throw IOSUsernameAuthError.invalidHandle
        }
    }

    public func register(handle: String, deviceID: String, mlsNodeID: String,
                         publicKey: Data, nonce: Data, signature: Data)
        async throws -> IOSUsernameAuthSession {
        try await authenticate(path: "v1/auth/username/register", handle: handle,
                               deviceID: deviceID, mlsNodeID: mlsNodeID,
                               publicKey: publicKey, nonce: nonce, signature: signature)
    }

    public func login(handle: String, deviceID: String, mlsNodeID: String,
                      publicKey: Data, nonce: Data, signature: Data)
        async throws -> IOSUsernameAuthSession {
        try await authenticate(path: "v1/auth/username/login", handle: handle,
                               deviceID: deviceID, mlsNodeID: mlsNodeID,
                               publicKey: publicKey, nonce: nonce, signature: signature)
    }

    public func lookup(handle: String) async throws -> IOSUsernameDirectory {
        let cleanHandle = handle.trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased().replacingOccurrences(of: "^@", with: "", options: .regularExpression)
        try Self.validateHandle(cleanHandle)
        var request = URLRequest(url: baseURL.appendingPathComponent("v1/directory/\(cleanHandle)"))
        request.httpMethod = "GET"
        request.httpShouldHandleCookies = false
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        let object = try await get(request)
        return try IOSUsernameDirectory(object: object)
    }

    public func registerDevice(accessToken: String,
                               payload: IOSPairingPayload)
        async throws -> IOSPairingRegistrationResponse {
        guard !accessToken.isEmpty, accessToken.count <= 4096 else {
            throw IOSUsernameAuthError.invalidRequest
        }
        let body: [String: Any] = [
            "device_id": payload.deviceID,
            "mls_node_id": payload.mlsNodeID,
            "public_key": IOSUsernameAuthSession.encode(payload.publicKey),
            "nonce": IOSUsernameAuthSession.encode(payload.nonce),
            "signature": IOSUsernameAuthSession.encode(payload.signature)
        ]
        let request = try makeRequest(path: "v1/devices", body: body,
                                      bearer: accessToken)
        return try IOSPairingRegistrationResponse(object: try await post(request))
    }

    private func authenticate(path: String, handle: String, deviceID: String,
                              mlsNodeID: String, publicKey: Data, nonce: Data,
                              signature: Data) async throws -> IOSUsernameAuthSession {
        let cleanHandle = handle.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        try Self.validateHandle(cleanHandle)
        guard IOSClient.isCanonicalUUID(deviceID), IOSClient.isCanonicalUUID(mlsNodeID),
              publicKey.count == 32, nonce.count == 32, signature.count == 64 else {
            throw IOSUsernameAuthError.invalidRequest
        }
        let body: [String: Any] = [
            "handle": cleanHandle,
            "device_id": deviceID,
            "mls_node_id": mlsNodeID,
            "public_key": IOSUsernameAuthSession.encode(publicKey),
            "nonce": IOSUsernameAuthSession.encode(nonce),
            "signature": IOSUsernameAuthSession.encode(signature)
        ]
        let request = try makeRequest(path: path, body: body)
        return try IOSUsernameAuthSession(object: try await post(request))
    }

    private func makeRequest(path: String, body: [String: Any], bearer: String? = nil)
        throws -> URLRequest {
        var request = URLRequest(url: baseURL.appendingPathComponent(path))
        request.httpMethod = "POST"
        request.httpShouldHandleCookies = false
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue("application/json; charset=utf-8", forHTTPHeaderField: "Content-Type")
        if let bearer { request.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization") }
        guard JSONSerialization.isValidJSONObject(body) else {
            throw IOSUsernameAuthError.invalidRequest
        }
        request.httpBody = try JSONSerialization.data(withJSONObject: body)
        return request
    }

    private func post(_ request: URLRequest) async throws -> [String: Any] {
        let (data, response) = try await data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw IOSUsernameAuthError.serviceRejected
        }
        guard (200..<300).contains(http.statusCode) else {
            if http.statusCode == 429 {
                let retryAfter = http.value(forHTTPHeaderField: "Retry-After")
                    .flatMap(Int.init)
                throw IOSUsernameAuthError.rateLimited(retryAfterSeconds: retryAfter)
            }
            if http.statusCode == 409 {
                throw IOSUsernameAuthError.conflict
            }
            throw IOSUsernameAuthError.serverRejected(statusCode: http.statusCode)
        }
        return try decodeObject(data)
    }

    private func get(_ request: URLRequest) async throws -> [String: Any] {
        let (data, response) = try await data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw IOSUsernameAuthError.serviceRejected
        }
        guard (200..<300).contains(http.statusCode) else {
            if http.statusCode == 429 {
                let retryAfter = http.value(forHTTPHeaderField: "Retry-After")
                    .flatMap(Int.init)
                throw IOSUsernameAuthError.rateLimited(retryAfterSeconds: retryAfter)
            }
            if http.statusCode == 409 {
                throw IOSUsernameAuthError.conflict
            }
            throw IOSUsernameAuthError.serverRejected(statusCode: http.statusCode)
        }
        return try decodeObject(data)
    }

    private func data(for request: URLRequest) async throws -> (Data, URLResponse) {
        do {
            return try await urlSession.data(for: request)
        } catch {
            throw IOSUsernameAuthError.serviceRejected
        }
    }

    private func decodeObject(_ data: Data) throws -> [String: Any] {
        guard data.count <= Self.maximumBodyBytes,
              let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw IOSUsernameAuthError.invalidResponse
        }
        return object
    }

    private static func isLoopback(_ host: String) -> Bool {
        host == "localhost" || host == "127.0.0.1" || host == "::1"
    }
}

public struct IOSPairingRegistrationResponse: Sendable {
    public let userID: String
    public let deviceID: String
    public let mlsNodeID: String
    public let publicKey: Data
    public let mlsCredential: Data

    fileprivate init(object: [String: Any]) throws {
        guard let userID = object["user_id"] as? String,
              IOSClient.isCanonicalUUID(userID),
              let deviceID = object["device_id"] as? String,
              IOSClient.isCanonicalUUID(deviceID),
              let mlsNodeID = object["mls_node_id"] as? String,
              IOSClient.isCanonicalUUID(mlsNodeID),
              let publicKeyString = object["public_key"] as? String,
              let publicKey = IOSUsernameAuthSession.decode(publicKeyString, count: 32),
              let credentialString = object["mls_credential"] as? String,
              let mlsCredential = IOSUsernameAuthSession.decode(credentialString, maximum: 1024),
              !mlsCredential.isEmpty else {
            throw IOSUsernameAuthError.invalidResponse
        }
        self.userID = userID
        self.deviceID = deviceID
        self.mlsNodeID = mlsNodeID
        self.publicKey = publicKey
        self.mlsCredential = mlsCredential
    }
}
