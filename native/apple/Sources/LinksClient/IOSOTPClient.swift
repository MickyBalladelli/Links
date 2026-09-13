import Foundation
import LinksKeyStore

public enum IOSOTPChannel: String, Sendable {
    case sms
    case whatsapp
}

public struct IOSOTPChallenge: Sendable {
    public let challengeID: String
    public let userID: String
    public let deviceID: String
    public let mlsNodeID: String
    public let publicKey: Data
    public let nonce: Data
    public let expiresAtMs: UInt64
    public let mlsCredential: Data

    fileprivate init(object: [String: Any]) throws {
        challengeID = try IOSOTPClient.uuid(object, key: "challenge_id")
        userID = try IOSOTPClient.uuid(object, key: "user_id")
        deviceID = try IOSOTPClient.uuid(object, key: "device_id")
        mlsNodeID = try IOSOTPClient.uuid(object, key: "mls_node_id")
        publicKey = try IOSOTPClient.encoded(object, key: "public_key", length: 32)
        nonce = try IOSOTPClient.encoded(object, key: "nonce", length: 32)
        expiresAtMs = try IOSOTPClient.uint64(object, key: "expires_at_ms")
        mlsCredential = try IOSOTPClient.encoded(object, key: "mls_credential", maximum: 1024)
        guard expiresAtMs > IOSOTPClient.nowMs(), !mlsCredential.isEmpty else {
            throw IOSOTPError.invalidChallenge
        }
    }
}

public struct IOSOTPAuthSession: Sendable {
    public let accessToken: String
    public let expiresAtMs: UInt64
    public let userID: String
    public let deviceID: String

    fileprivate init(object: [String: Any]) throws {
        guard let token = object["access_token"] as? String,
              !token.isEmpty,
              IOSOTPClient.decode(token, expectedLength: 32) != nil else {
            throw IOSOTPError.invalidSession
        }
        accessToken = token
        expiresAtMs = try IOSOTPClient.uint64(object, key: "expires_at_ms")
        userID = try IOSOTPClient.uuid(object, key: "user_id")
        deviceID = try IOSOTPClient.uuid(object, key: "device_id")
        guard expiresAtMs > IOSOTPClient.nowMs() else { throw IOSOTPError.invalidSession }
    }
}

public enum IOSOTPError: Error {
    case invalidEndpoint
    case invalidPhone
    case invalidCode
    case invalidChallenge
    case invalidSession
    case invalidResponse
    case serviceRejected
    case challengeIdentityMismatch
}

/// HTTPS client for the account-auth OTP endpoints. It never stores a bearer.
public final class IOSOTPClient: Sendable {
    private static let maxBodyBytes = 16 * 1024
    private let baseURL: URL
    private let urlSession: URLSession

    public init(baseURL: URL, urlSession: URLSession = .shared) throws {
        guard baseURL.scheme?.lowercased() == "https",
              baseURL.host?.isEmpty == false,
              baseURL.user == nil,
              baseURL.query == nil,
              baseURL.fragment == nil else {
            throw IOSOTPError.invalidEndpoint
        }
        self.baseURL = baseURL
        self.urlSession = urlSession
    }

    public func start(phone: String, channel: IOSOTPChannel, deviceID: String,
                      mlsNodeID: String, publicKey: Data, signature: Data)
        async throws -> IOSOTPChallenge {
        try Self.validatePhone(phone)
        guard IOSClient.isCanonicalUUID(deviceID), IOSClient.isCanonicalUUID(mlsNodeID),
              publicKey.count == 32, signature.count == 64 else {
            throw IOSOTPError.invalidChallenge
        }
        let body: [String: Any] = [
            "phone": phone,
            "channel": channel.rawValue,
            "device_id": deviceID,
            "mls_node_id": mlsNodeID,
            "public_key": Self.encode(publicKey),
            "signature": Self.encode(signature)
        ]
        let response = try await post(path: "v1/auth/start", body: body)
        return try IOSOTPChallenge(object: response.object)
    }

    public func finish(challengeID: String, code: String, signature: Data)
        async throws -> IOSOTPAuthSession {
        guard IOSClient.isCanonicalUUID(challengeID),
              code.count >= 6, code.count <= 10,
              code.utf8.allSatisfy({ $0 >= 48 && $0 <= 57 }), signature.count == 64 else {
            throw IOSOTPError.invalidCode
        }
        let body: [String: Any] = [
            "challenge_id": challengeID,
            "code": code,
            "signature": Self.encode(signature)
        ]
        let response = try await post(path: "v1/auth/finish", body: body)
        return try IOSOTPAuthSession(object: response.object)
    }

    static func nowMs() -> UInt64 {
        UInt64(max(0, Date().timeIntervalSince1970 * 1000))
    }

    fileprivate static func uuid(_ object: [String: Any], key: String) throws -> String {
        guard let value = object[key] as? String, IOSClient.isCanonicalUUID(value) else {
            throw IOSOTPError.invalidResponse
        }
        return value
    }

    fileprivate static func uint64(_ object: [String: Any], key: String) throws -> UInt64 {
        if let value = object[key] as? NSNumber, value.int64Value > 0 {
            return UInt64(value.int64Value)
        }
        throw IOSOTPError.invalidResponse
    }

    fileprivate static func encoded(_ object: [String: Any], key: String, length: Int? = nil,
                                     maximum: Int? = nil) throws -> Data {
        guard let value = object[key] as? String,
              let decoded = decode(value), !decoded.isEmpty,
              length.map({ decoded.count == $0 }) ?? true,
              maximum.map({ decoded.count <= $0 }) ?? true else {
            throw IOSOTPError.invalidResponse
        }
        return decoded
    }

    fileprivate static func decode(_ value: String, expectedLength: Int? = nil) -> Data? {
        guard !value.isEmpty else { return nil }
        let padded = value.replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        guard padded.count % 4 != 1 else { return nil }
        let withPadding = padded + String(repeating: "=", count: (4 - padded.count % 4) % 4)
        guard let decoded = Data(base64Encoded: withPadding),
              encode(decoded) == value,
              expectedLength.map({ decoded.count == $0 }) ?? true else { return nil }
        return decoded
    }

    private static func encode(_ data: Data) -> String {
        data.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .trimmingCharacters(in: CharacterSet(charactersIn: "="))
    }

    private func post(path: String, body: [String: Any]) async throws -> JSONResponse {
        var request = URLRequest(url: baseURL.appendingPathComponent(path))
        request.httpMethod = "POST"
        request.httpShouldHandleCookies = false
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue("application/json; charset=utf-8", forHTTPHeaderField: "Content-Type")
        guard JSONSerialization.isValidJSONObject(body) else { throw IOSOTPError.invalidResponse }
        request.httpBody = try JSONSerialization.data(withJSONObject: body)
        let (data, response): (Data, URLResponse)
        do {
            (data, response) = try await urlSession.data(for: request)
        } catch {
            throw IOSOTPError.serviceRejected
        }
        guard data.count <= Self.maxBodyBytes,
              let http = response as? HTTPURLResponse,
              (200..<300).contains(http.statusCode) else {
            throw IOSOTPError.serviceRejected
        }
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw IOSOTPError.invalidResponse
        }
        return JSONResponse(object: object)
    }

    private static func validatePhone(_ phone: String) throws {
        let bytes = Array(phone.utf8)
        guard bytes.count >= 9, bytes.count <= 16, bytes.first == 43,
              bytes.dropFirst().first != 48,
              bytes.dropFirst().allSatisfy({ $0 >= 48 && $0 <= 57 }) else {
            throw IOSOTPError.invalidPhone
        }
    }
}

private struct JSONResponse {
    let object: [String: Any]
}
