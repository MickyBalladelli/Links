import CryptoKit
import Foundation

public enum IOSDecentralizedRelayMode: String, Sendable {
    case open
    case tokenIncentivized = "token-incentivized"
}

public enum IOSDecentralizedError: Error, Equatable {
    case invalidRoute
    case transportUnavailable
    case storageUnavailable
    case mediaUnavailable
    case invalidToken
    case integrityFailure
}

/// Candidate must be marked verified only after the shared Rust/WASM DHT
/// verifier checks its Ed25519 signature and trust-directory binding.
public struct IOSDecentralizedMediaRelay: Sendable {
    public let nodeID: String
    public let region: String
    public let endpoint: URL
    public let turnURL: String?
    public let mode: IOSDecentralizedRelayMode
    public let priceUnitsPerMinute: UInt64
    public let maxBitrateKbps: UInt32
    public let expiresAtMs: UInt64
    public let supportsSFrame: Bool
    public let verified: Bool

    public init(nodeID: String, region: String, endpoint: URL, turnURL: String? = nil,
                mode: IOSDecentralizedRelayMode, priceUnitsPerMinute: UInt64,
                maxBitrateKbps: UInt32, expiresAtMs: UInt64,
                supportsSFrame: Bool = true, verified: Bool = true) throws {
        guard validDecentralizedLocator(nodeID), validDecentralizedLocator(region),
              endpoint.scheme?.lowercased() == "wss", endpoint.host?.isEmpty == false,
              endpoint.user == nil, endpoint.password == nil, endpoint.query == nil,
              endpoint.fragment == nil,
              turnURL.map({ $0.hasPrefix("turn:") || $0.hasPrefix("turns:") }) ?? true,
              maxBitrateKbps > 0, expiresAtMs > 0, supportsSFrame, verified,
              (mode == .open && priceUnitsPerMinute == 0)
                || (mode == .tokenIncentivized && priceUnitsPerMinute > 0) else {
            throw IOSDecentralizedError.invalidRoute
        }
        self.nodeID = nodeID
        self.region = region
        self.endpoint = endpoint
        self.turnURL = turnURL
        self.mode = mode
        self.priceUnitsPerMinute = priceUnitsPerMinute
        self.maxBitrateKbps = maxBitrateKbps
        self.expiresAtMs = expiresAtMs
        self.supportsSFrame = supportsSFrame
        self.verified = verified
    }
}

public struct IOSDecentralizedRelayAccess: Sendable {
    public let token: Data
    public let relayNodeID: String
    public let sessionID: String
    public let expiresAtMs: UInt64
    public let maxDurationMs: UInt64

    public init(token: Data, relayNodeID: String, sessionID: String,
                expiresAtMs: UInt64, maxDurationMs: UInt64) throws {
        guard !token.isEmpty, validDecentralizedLocator(relayNodeID),
              IOSClient.isCanonicalUUID(sessionID), expiresAtMs > 0,
              maxDurationMs > 0 else { throw IOSDecentralizedError.invalidToken }
        self.token = token
        self.relayNodeID = relayNodeID
        self.sessionID = sessionID
        self.expiresAtMs = expiresAtMs
        self.maxDurationMs = maxDurationMs
    }
}

public struct IOSDecentralizedMediaRoute: Sendable {
    public let relay: IOSDecentralizedMediaRelay
    public let sessionID: String
    public let durationMs: UInt64
    public let accessToken: Data?
}

public protocol IOSDecentralizedTransportAdapter: AnyObject {
    func publishOpaque(endpoint: URL, envelope: Data) async throws
    func replayOpaque(endpoint: URL, afterCursor: UInt64, limit: Int) async throws -> [Data]
}

public protocol IOSDecentralizedChunkStore: AnyObject {
    func uploadCiphertext(gateway: URL, cid: String, ciphertext: Data) async throws
    func downloadCiphertext(gateway: URL, cid: String) async throws -> Data
}

public struct IOSDecentralizedPlan: Sendable {
    public let region: String
    public let transportEndpoints: [URL]
    public let storageGateways: [URL]
    public let mediaRelays: [IOSDecentralizedMediaRelay]

    public init(region: String, transportEndpoints: [URL], storageGateways: [URL],
                mediaRelays: [IOSDecentralizedMediaRelay]) throws {
        guard validDecentralizedLocator(region),
              !transportEndpoints.isEmpty, transportEndpoints.count <= 8,
              !storageGateways.isEmpty, storageGateways.count <= 8,
              mediaRelays.count <= 16,
              transportEndpoints.allSatisfy(Self.validTransportEndpoint),
              storageGateways.allSatisfy(Self.validStorageGateway),
              mediaRelays.allSatisfy({ $0.region == region }) else {
            throw IOSDecentralizedError.invalidRoute
        }
        self.region = region
        self.transportEndpoints = transportEndpoints
        self.storageGateways = storageGateways
        self.mediaRelays = mediaRelays
    }

    private static func validTransportEndpoint(_ value: URL) -> Bool {
        value.scheme?.lowercased() == "wss" && value.host?.isEmpty == false
            && value.user == nil && value.password == nil
            && value.query == nil && value.fragment == nil
    }

    private static func validStorageGateway(_ value: URL) -> Bool {
        value.scheme?.lowercased() == "https" && value.host?.isEmpty == false
            && value.user == nil && value.password == nil
            && value.query == nil && value.fragment == nil
    }

}

public final class IOSDecentralizedClient {
    public static let maximumEnvelopeBytes = 256 * 1024
    public static let maximumChunkBytes = 256 * 1024
    public static let maximumLeaseMs: UInt64 = 10 * 60 * 1000

    private let plan: IOSDecentralizedPlan
    private let transport: any IOSDecentralizedTransportAdapter
    private let storage: any IOSDecentralizedChunkStore

    public init(plan: IOSDecentralizedPlan,
                transport: any IOSDecentralizedTransportAdapter,
                storage: any IOSDecentralizedChunkStore) {
        self.plan = plan
        self.transport = transport
        self.storage = storage
    }

    public func publishOpaqueEnvelope(_ envelope: Data) async throws {
        guard !envelope.isEmpty, envelope.count <= Self.maximumEnvelopeBytes else {
            throw IOSDecentralizedError.invalidRoute
        }
        for endpoint in plan.transportEndpoints {
            do {
                try await transport.publishOpaque(endpoint: endpoint, envelope: envelope)
                return
            } catch { continue }
        }
        throw IOSDecentralizedError.transportUnavailable
    }

    public func replayOpaque(afterCursor: UInt64, limit: Int = 100) async throws -> [Data] {
        guard limit > 0, limit <= 100 else { throw IOSDecentralizedError.invalidRoute }
        for endpoint in plan.transportEndpoints {
            do {
                let envelopes = try await transport.replayOpaque(
                    endpoint: endpoint, afterCursor: afterCursor, limit: limit)
                guard envelopes.count <= limit,
                      envelopes.allSatisfy({ !$0.isEmpty && $0.count <= Self.maximumEnvelopeBytes })
                else { throw IOSDecentralizedError.invalidRoute }
                return envelopes
            } catch let error as IOSDecentralizedError where error == .invalidRoute {
                throw error
            } catch { continue }
        }
        throw IOSDecentralizedError.transportUnavailable
    }

    public func uploadCiphertextChunk(cid: String, ciphertext: Data) async throws {
        guard Self.cid(for: ciphertext) == cid else {
            throw IOSDecentralizedError.integrityFailure
        }
        for gateway in plan.storageGateways {
            do {
                try await storage.uploadCiphertext(
                    gateway: gateway, cid: cid, ciphertext: ciphertext)
                return
            } catch { continue }
        }
        throw IOSDecentralizedError.storageUnavailable
    }

    public func downloadCiphertextChunk(cid: String) async throws -> Data {
        guard Self.validCID(cid) else { throw IOSDecentralizedError.integrityFailure }
        for gateway in plan.storageGateways {
            do {
                let ciphertext = try await storage.downloadCiphertext(gateway: gateway, cid: cid)
                guard Self.cid(for: ciphertext) == cid else {
                    throw IOSDecentralizedError.integrityFailure
                }
                return ciphertext
            } catch let error as IOSDecentralizedError where error == .integrityFailure {
                throw error
            } catch { continue }
        }
        throw IOSDecentralizedError.storageUnavailable
    }

    public func selectMediaRelay(sessionID: String, durationMs: UInt64,
                                 access: IOSDecentralizedRelayAccess? = nil,
                                 nowMs: UInt64 = IOSDecentralizedClient.nowMs())
        throws -> IOSDecentralizedMediaRoute {
        guard IOSClient.isCanonicalUUID(sessionID), durationMs > 0,
              durationMs <= Self.maximumLeaseMs else {
            throw IOSDecentralizedError.invalidRoute
        }
        if let relay = plan.mediaRelays.first(where: {
            $0.mode == .open && $0.expiresAtMs > nowMs
        }) {
            return IOSDecentralizedMediaRoute(
                relay: relay, sessionID: sessionID, durationMs: durationMs, accessToken: nil)
        }
        for relay in plan.mediaRelays where relay.mode == .tokenIncentivized {
            guard let access,
                  relay.expiresAtMs > nowMs,
                  access.relayNodeID == relay.nodeID,
                  access.sessionID == sessionID,
                  access.expiresAtMs > nowMs,
                  access.maxDurationMs >= durationMs else { continue }
            return IOSDecentralizedMediaRoute(
                relay: relay, sessionID: sessionID, durationMs: durationMs,
                accessToken: access.token)
        }
        throw IOSDecentralizedError.mediaUnavailable
    }

    private static func nowMs() -> UInt64 {
        UInt64(Date().timeIntervalSince1970 * 1_000)
    }

    private static func validCID(_ value: String) -> Bool {
        let bytes = Array(value.utf8)
        guard bytes.count == 59, bytes.first == Character("b").asciiValue else { return false }
        return bytes.dropFirst().allSatisfy {
            ($0 >= 97 && $0 <= 122) || ($0 >= 50 && $0 <= 55)
        }
    }

    private static func cid(for ciphertext: Data) -> String {
        guard !ciphertext.isEmpty, ciphertext.count <= maximumChunkBytes else {
            return ""
        }
        var binary = Data([1, 0x55, 0x12, 0x20])
        binary.append(contentsOf: SHA256.hash(data: ciphertext))
        return "b" + base32(binary)
    }

    private static func base32(_ data: Data) -> String {
        let alphabet = Array("abcdefghijklmnopqrstuvwxyz234567".utf8)
        var accumulator = 0
        var bits = 0
        var output = ""
        for byte in data {
            accumulator = (accumulator << 8) | Int(byte)
            bits += 8
            while bits >= 5 {
                bits -= 5
                output.append(Character(UnicodeScalar(alphabet[(accumulator >> bits) & 31])))
                accumulator &= bits == 0 ? 0 : (1 << bits) - 1
            }
        }
        if bits > 0 {
            output.append(Character(UnicodeScalar(alphabet[(accumulator << (5 - bits)) & 31])))
        }
        return output
    }
}

private func validDecentralizedLocator(_ value: String) -> Bool {
    let bytes = Array(value.utf8)
    return !bytes.isEmpty && bytes.count <= 128 && bytes.allSatisfy {
        ($0 >= 48 && $0 <= 57) || ($0 >= 65 && $0 <= 90)
            || ($0 >= 97 && $0 <= 122) || $0 == 45 || $0 == 46
            || $0 == 95 || $0 == 58
    }
}
