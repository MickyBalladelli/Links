import Foundation

public enum IOSUsernameAuthError: Error {
    case invalidEndpoint
    case invalidHandle
    case invalidRequest
    case invalidResponse
    case serviceRejected
    case serverRejected(statusCode: Int)
    case networkUnavailable
    case cannotConnect
    case timedOut
    case tlsRejected
    case conflict
    case deviceAlreadyRegistered
    case rateLimited(retryAfterSeconds: Int?)
}

public struct IOSUsernameAuthChallenge: Sendable {
    public let challengeID: String
    public let handle: String
    public let purpose: String
    public let deviceID: String
    public let mlsNodeID: String
    public let publicKey: Data
    public let challenge: Data
    public let expiresAtMs: UInt64

    fileprivate init(object: [String: Any]) throws {
        guard let challengeID = object["challenge_id"] as? String,
              IOSClient.isCanonicalUUID(challengeID),
              let handle = object["handle"] as? String,
              IOSUsernameAuthSession.isCanonicalHandle(handle),
              let purpose = object["purpose"] as? String,
              purpose == "registration" || purpose == "login",
              let deviceID = object["device_id"] as? String,
              IOSClient.isCanonicalUUID(deviceID),
              let mlsNodeID = object["mls_node_id"] as? String,
              IOSClient.isCanonicalUUID(mlsNodeID),
              let publicKeyString = object["public_key"] as? String,
              let publicKey = IOSUsernameAuthSession.decode(publicKeyString, count: 32),
              let challengeString = object["challenge"] as? String,
              let challenge = IOSUsernameAuthSession.decode(challengeString, count: 32),
              let expiresAt = object["expires_at_ms"] as? NSNumber,
              expiresAt.int64Value > 0 else {
            throw IOSUsernameAuthError.invalidResponse
        }
        self.challengeID = challengeID
        self.handle = handle
        self.purpose = purpose
        self.deviceID = deviceID
        self.mlsNodeID = mlsNodeID
        self.publicKey = publicKey
        self.challenge = challenge
        self.expiresAtMs = UInt64(expiresAt.int64Value)
    }
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

    public func startAuthentication(handle: String, purpose: String, deviceID: String,
                                    mlsNodeID: String, publicKey: Data)
        async throws -> IOSUsernameAuthChallenge {
        let cleanHandle = handle.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        try Self.validateHandle(cleanHandle)
        guard purpose == "registration" || purpose == "login",
              IOSClient.isCanonicalUUID(deviceID), IOSClient.isCanonicalUUID(mlsNodeID),
              publicKey.count == 32 else {
            throw IOSUsernameAuthError.invalidRequest
        }
        let body: [String: Any] = [
            "handle": cleanHandle,
            "purpose": purpose,
            "device_id": deviceID,
            "mls_node_id": mlsNodeID,
            "public_key": IOSUsernameAuthSession.encode(publicKey)
        ]
        let request = try makeRequest(path: "v1/auth/username/challenge", body: body)
        return try IOSUsernameAuthChallenge(object: try await post(request))
    }

    public func register(challengeID: String, signature: Data)
        async throws -> IOSUsernameAuthSession {
        try await finishAuthentication(path: "v1/auth/username/register",
                                       challengeID: challengeID, signature: signature)
    }

    public func login(challengeID: String, signature: Data)
        async throws -> IOSUsernameAuthSession {
        try await finishAuthentication(path: "v1/auth/username/login",
                                       challengeID: challengeID, signature: signature)
    }

    public func logout(accessToken: String) async throws {
        guard !accessToken.isEmpty, accessToken.count <= 4096 else {
            throw IOSUsernameAuthError.invalidRequest
        }
        var request = URLRequest(url: baseURL.appendingPathComponent("v1/auth/logout"))
        request.httpMethod = "POST"
        request.httpShouldHandleCookies = false
        request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        let (_, response) = try await data(for: request)
        guard let http = response as? HTTPURLResponse,
              (200..<300).contains(http.statusCode) else {
            throw IOSUsernameAuthError.serviceRejected
        }
    }

    public func uploadProfilePicture(accessToken: String, jpeg: Data) async throws {
        guard !accessToken.isEmpty, accessToken.count <= 4096,
              jpeg.count >= 3, jpeg.count <= 131_072,
              jpeg.starts(with: Data([0xFF, 0xD8, 0xFF])) else {
            throw IOSUsernameAuthError.invalidRequest
        }
        var request = URLRequest(url: baseURL.appendingPathComponent("v1/profile/picture"))
        request.httpMethod = "PUT"
        request.httpShouldHandleCookies = false
        request.setValue("image/jpeg", forHTTPHeaderField: "Content-Type")
        request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        request.httpBody = jpeg
        let (_, response) = try await data(for: request)
        guard let http = response as? HTTPURLResponse, http.statusCode == 204 else {
            throw IOSUsernameAuthError.serviceRejected
        }
    }

    public func deleteProfilePicture(accessToken: String) async throws {
        guard !accessToken.isEmpty, accessToken.count <= 4096 else {
            throw IOSUsernameAuthError.invalidRequest
        }
        var request = URLRequest(url: baseURL.appendingPathComponent("v1/profile/picture"))
        request.httpMethod = "DELETE"
        request.httpShouldHandleCookies = false
        request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        let (_, response) = try await data(for: request)
        guard let http = response as? HTTPURLResponse, http.statusCode == 204 else {
            throw IOSUsernameAuthError.serviceRejected
        }
    }

    // MARK: Group roles. The server only stores who belongs to a group and
    // with which role; names and messages stay inside MLS.

    public enum GroupRole: String, Codable, Sendable {
        case owner, admin, member
    }

    public struct GroupMember: Codable, Equatable, Sendable {
        public let userID: String
        public let role: GroupRole

        private enum CodingKeys: String, CodingKey {
            case userID = "user_id"
            case role
        }
    }

    private func groupRequest(_ path: String, method: String, accessToken: String,
                              json: [String: String]? = nil) throws -> URLRequest {
        guard !accessToken.isEmpty, accessToken.count <= 4096 else {
            throw IOSUsernameAuthError.invalidRequest
        }
        var request = URLRequest(url: baseURL.appendingPathComponent(path))
        request.httpMethod = method
        request.httpShouldHandleCookies = false
        request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        if let json {
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = try JSONSerialization.data(withJSONObject: json)
        }
        return request
    }

    private func requireGroupStatus(_ response: URLResponse, _ accepted: Set<Int>) throws {
        guard let http = response as? HTTPURLResponse else {
            throw IOSUsernameAuthError.serviceRejected
        }
        guard accepted.contains(http.statusCode) else {
            throw IOSUsernameAuthError.serverRejected(statusCode: http.statusCode)
        }
    }

    /// Register a group with the caller as owner.
    public func createGroup(accessToken: String, groupID: String) async throws {
        guard IOSClient.isCanonicalUUID(groupID) else { throw IOSUsernameAuthError.invalidRequest }
        let request = try groupRequest("v1/groups", method: "POST", accessToken: accessToken,
                                       json: ["group_id": groupID, "kind": "group"])
        let (_, response) = try await data(for: request)
        try requireGroupStatus(response, [200, 201])
    }

    /// Add a member or change a role. Owners grant any role; admins may only
    /// add plain members.
    public func setGroupRole(accessToken: String, groupID: String, userID: String,
                             role: GroupRole) async throws {
        guard IOSClient.isCanonicalUUID(groupID), IOSClient.isCanonicalUUID(userID) else {
            throw IOSUsernameAuthError.invalidRequest
        }
        let request = try groupRequest("v1/groups/\(groupID)/members/\(userID)/role",
                                       method: "PUT", accessToken: accessToken,
                                       json: ["role": role.rawValue])
        let (_, response) = try await data(for: request)
        try requireGroupStatus(response, [200, 204])
    }

    /// Delete the group record. Only the owner is allowed.
    public func deleteGroup(accessToken: String, groupID: String) async throws {
        guard IOSClient.isCanonicalUUID(groupID) else { throw IOSUsernameAuthError.invalidRequest }
        let request = try groupRequest("v1/groups/\(groupID)", method: "DELETE",
                                       accessToken: accessToken)
        let (_, response) = try await data(for: request)
        try requireGroupStatus(response, [200, 204, 404])
    }

    /// Remove a member, or leave when `userID` is the caller.
    public func removeGroupMember(accessToken: String, groupID: String,
                                  userID: String) async throws {
        guard IOSClient.isCanonicalUUID(groupID), IOSClient.isCanonicalUUID(userID) else {
            throw IOSUsernameAuthError.invalidRequest
        }
        let request = try groupRequest("v1/groups/\(groupID)/members/\(userID)",
                                       method: "DELETE", accessToken: accessToken)
        let (_, response) = try await data(for: request)
        try requireGroupStatus(response, [200, 204, 404])
    }

    public func groupMembers(accessToken: String, groupID: String) async throws -> [GroupMember] {
        guard IOSClient.isCanonicalUUID(groupID) else { throw IOSUsernameAuthError.invalidRequest }
        let request = try groupRequest("v1/groups/\(groupID)/members", method: "GET",
                                       accessToken: accessToken)
        let (body, response) = try await data(for: request)
        try requireGroupStatus(response, [200])
        struct Envelope: Decodable { let members: [GroupMember] }
        return try JSONDecoder().decode(Envelope.self, from: body).members
    }

    /// Returns the published JPEG, or nil when this username has no picture.
    public func downloadProfilePicture(handle: String) async throws -> Data? {
        let cleanHandle = handle.trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased().replacingOccurrences(of: "^@", with: "", options: .regularExpression)
        try Self.validateHandle(cleanHandle)
        var request = URLRequest(url: baseURL.appendingPathComponent("v1/directory/\(cleanHandle)/picture"))
        request.httpMethod = "GET"
        request.httpShouldHandleCookies = false
        request.setValue("image/jpeg", forHTTPHeaderField: "Accept")
        let (data, response) = try await data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw IOSUsernameAuthError.serviceRejected
        }
        if http.statusCode == 404 { return nil }
        guard http.statusCode == 200,
              data.count >= 3, data.count <= 131_072,
              data.starts(with: Data([0xFF, 0xD8, 0xFF])) else {
            throw IOSUsernameAuthError.invalidResponse
        }
        return data
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

    public func lookup(userID: String, accessToken: String) async throws -> IOSUsernameDirectory {
        guard IOSClient.isCanonicalUUID(userID),
              !accessToken.isEmpty, accessToken.count <= 4096 else {
            throw IOSUsernameAuthError.invalidRequest
        }
        var request = URLRequest(url: baseURL.appendingPathComponent("v1/directory/users/\(userID)"))
        request.httpMethod = "GET"
        request.httpShouldHandleCookies = false
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        let directory = try IOSUsernameDirectory(object: try await get(request))
        guard directory.userID == userID else {
            throw IOSUsernameAuthError.invalidResponse
        }
        return directory
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

    private func finishAuthentication(path: String, challengeID: String,
                                      signature: Data) async throws -> IOSUsernameAuthSession {
        guard IOSClient.isCanonicalUUID(challengeID), signature.count == 64 else {
            throw IOSUsernameAuthError.invalidRequest
        }
        let body: [String: Any] = [
            "challenge_id": challengeID,
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
                if Self.errorCode(from: data) == "device_already_registered" {
                    throw IOSUsernameAuthError.deviceAlreadyRegistered
                }
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
        } catch let error as URLError {
            switch error.code {
            case .notConnectedToInternet:
                throw IOSUsernameAuthError.networkUnavailable
            case .networkConnectionLost:
                throw IOSUsernameAuthError.cannotConnect
            case .cannotConnectToHost, .cannotFindHost, .dnsLookupFailed:
                throw IOSUsernameAuthError.cannotConnect
            case .timedOut:
                throw IOSUsernameAuthError.timedOut
            case .serverCertificateUntrusted, .serverCertificateHasBadDate,
                 .serverCertificateNotYetValid, .secureConnectionFailed:
                throw IOSUsernameAuthError.tlsRejected
            default:
                throw IOSUsernameAuthError.serviceRejected
            }
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

    private static func errorCode(from data: Data) -> String? {
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return nil
        }
        return object["error"] as? String
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
