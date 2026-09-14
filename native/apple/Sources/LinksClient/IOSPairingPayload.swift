import CryptoKit
import Foundation

public enum IOSPairingError: Error {
    case invalidPayload
    case invalidSignature
}

/// Canonical, public device-pairing data carried by `links://connect`.
/// It contains no seed, private key, or bearer token.
public struct IOSPairingPayload: Equatable, Sendable {
    public let userID: String
    public let deviceID: String
    public let mlsNodeID: String
    public let publicKey: Data
    public let nonce: Data
    public let signature: Data

    public init(userID: String, deviceID: String, mlsNodeID: String,
                publicKey: Data, nonce: Data, signature: Data) throws {
        guard IOSClient.isCanonicalUUID(userID),
              IOSClient.isCanonicalUUID(deviceID),
              IOSClient.isCanonicalUUID(mlsNodeID),
              publicKey.count == 32,
              nonce.count == 32,
              signature.count == 64 else {
            throw IOSPairingError.invalidPayload
        }
        self.userID = userID
        self.deviceID = deviceID
        self.mlsNodeID = mlsNodeID
        self.publicKey = publicKey
        self.nonce = nonce
        self.signature = signature
    }

    public init(uri: String) throws {
        guard uri.utf8.count <= 1024,
              uri.utf8.allSatisfy({ $0 < 128 }),
              uri.hasPrefix("links://connect?") else {
            throw IOSPairingError.invalidPayload
        }
        let query = String(uri.dropFirst("links://connect?".count))
        var fields: [String: String] = [:]
        for component in query.split(separator: "&", omittingEmptySubsequences: false) {
            let parts = component.split(separator: "=", maxSplits: 1,
                                        omittingEmptySubsequences: false)
            guard parts.count == 2, !parts[1].isEmpty else {
                throw IOSPairingError.invalidPayload
            }
            let key = String(parts[0])
            guard ["v", "user_id", "device_id", "mls_node_id", "public_key", "nonce",
                   "signature"].contains(key), fields[key] == nil else {
                throw IOSPairingError.invalidPayload
            }
            fields[key] = String(parts[1])
        }
        guard fields["v"] == "1",
              let userID = fields["user_id"],
              let deviceID = fields["device_id"],
              let mlsNodeID = fields["mls_node_id"],
              let publicKey = Self.decode(fields["public_key"], count: 32),
              let nonce = Self.decode(fields["nonce"], count: 32),
              let signature = Self.decode(fields["signature"], count: 64) else {
            throw IOSPairingError.invalidPayload
        }
        try self.init(userID: userID, deviceID: deviceID, mlsNodeID: mlsNodeID,
                      publicKey: publicKey, nonce: nonce, signature: signature)
        guard try toURI() == uri else { throw IOSPairingError.invalidPayload }
    }

    public static func generateNonce() -> Data {
        var generator = SystemRandomNumberGenerator()
        return Data((0..<32).map { _ in
            UInt8.random(in: UInt8.min...UInt8.max, using: &generator)
        })
    }

    public func signingTranscript() throws -> Data {
        guard let userID = UUID(uuidString: userID),
              let deviceID = UUID(uuidString: deviceID),
              let mlsNodeID = UUID(uuidString: mlsNodeID) else {
            throw IOSPairingError.invalidPayload
        }
        var transcript = Data("links/device-pairing/v1\0".utf8)
        transcript.append(contentsOf: userID.bytes)
        transcript.append(contentsOf: deviceID.bytes)
        transcript.append(contentsOf: mlsNodeID.bytes)
        transcript.append(publicKey)
        transcript.append(nonce)
        return transcript
    }

    public func verify() throws {
        let key: Curve25519.Signing.PublicKey
        do {
            key = try Curve25519.Signing.PublicKey(rawRepresentation: publicKey)
        } catch {
            throw IOSPairingError.invalidSignature
        }
        guard key.isValidSignature(signature, for: try signingTranscript()) else {
            throw IOSPairingError.invalidSignature
        }
    }

    public func toURI() throws -> String {
        let uri = "links://connect?v=1&user_id=\(userID)&device_id=\(deviceID)&mls_node_id=\(mlsNodeID)&public_key=\(Self.encode(publicKey))&nonce=\(Self.encode(nonce))&signature=\(Self.encode(signature))"
        guard uri.utf8.count <= 1024 else { throw IOSPairingError.invalidPayload }
        return uri
    }

    private static func encode(_ data: Data) -> String {
        data.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .trimmingCharacters(in: CharacterSet(charactersIn: "="))
    }

    private static func decode(_ value: String?, count: Int) -> Data? {
        guard let value, !value.isEmpty else { return nil }
        let padded = value.replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        guard padded.count % 4 != 1 else { return nil }
        let withPadding = padded + String(repeating: "=", count: (4 - padded.count % 4) % 4)
        guard let decoded = Data(base64Encoded: withPadding),
              decoded.count == count,
              encode(decoded) == value else { return nil }
        return decoded
    }
}

extension UUID {
    var bytes: [UInt8] {
        var value = uuid
        return withUnsafeBytes(of: &value) { Array($0) }
    }
}
