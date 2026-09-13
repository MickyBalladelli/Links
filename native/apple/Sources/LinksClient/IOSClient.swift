import Foundation
import LinksKeyStore

/// Public identity data passed from the iOS host into the shared Rust core.
/// The hardware seed is never part of this value.
public struct SharedCoreIdentity: Equatable {
    public let userID: String
    public let deviceID: String
    public let mlsNodeID: String
    public let identity: IdentityKeyReference

    public init(userID: String, deviceID: String, mlsNodeID: String,
                identity: IdentityKeyReference) throws {
        guard IOSClient.isCanonicalUUID(userID),
              IOSClient.isCanonicalUUID(deviceID),
              IOSClient.isCanonicalUUID(mlsNodeID) else {
            throw IOSClientError.invalidMetadata
        }
        self.userID = userID
        self.deviceID = deviceID
        self.mlsNodeID = mlsNodeID
        self.identity = identity
    }
}

/// The native adapter for links-client-core. A production implementation
/// owns the Rust ClientCore, MLS provider, envelope provider, and durable store.
/// There is no software or plaintext fallback when this adapter is unavailable.
public protocol SharedClientCore: AnyObject {
    var userID: String { get }
    var deviceID: String { get }
    func durableCursor() throws -> UInt64
    func createHello(accessToken: String, lastSeenCursor: UInt64) throws -> Data
    func handleServerFrame(_ frame: Data) throws
}

public protocol SharedCoreIdentitySigner: AnyObject {
    func sign(_ transcript: Data) throws -> Data
}

/// Factory boundary for the generated Rust/Swift client-core binding.
public protocol SharedClientCoreFactory: AnyObject {
    func makeCore(identity: SharedCoreIdentity,
                  signer: any SharedCoreIdentitySigner) throws -> any SharedClientCore
}

public enum IOSClientError: Error {
    case invalidMetadata
    case identityAlreadyEnrolled
    case identityNotEnrolled
    case authenticatedSessionRequired
    case coreIdentityMismatch
    case metadataUnavailable
}

/// Base iOS client session. It owns hardware identity enrollment and public
/// device metadata, then hands the authenticated identity to shared Rust core.
/// OTP, transport, message UI, APNs and recovery are separate host layers.
public final class IOSClient: SharedCoreIdentitySigner {
    private struct StoredMetadata: Codable {
        let handle: String
        let publicKey: Data
        let deviceID: String
        let mlsNodeID: String
        let userID: String?
    }

    private struct AuthenticatedSession {
        let userID: String
        let accessToken: String
        let expiresAtMs: UInt64
    }

    private static let metadataKey = "links.client.metadata.v1"
    private static let nilUUID = UUID(uuidString: "00000000-0000-0000-0000-000000000000")!
    private let identityStore: HardwareIdentityStore
    private let defaults: UserDefaults
    private var authenticated: AuthenticatedSession?

    public private(set) var identity: IdentityKeyReference?
    public private(set) var deviceID: String?
    public private(set) var mlsNodeID: String?
    public private(set) var userID: String?

    public init(identityStore: HardwareIdentityStore = HardwareIdentityStore(),
                defaults: UserDefaults = .standard) throws {
        self.identityStore = identityStore
        self.defaults = defaults
        try restoreMetadata()
    }

    public var isEnrolled: Bool { identity != nil }

    public var isAuthenticated: Bool {
        guard let authenticated else { return false }
        return authenticated.expiresAtMs > Self.nowMs()
    }

    /// Creates a fresh hardware-backed identity and commits only public
    /// metadata to UserDefaults. The seed remains inside HardwareSeedVault.
    @discardableResult
    public func createIdentity() throws -> IdentityKeyReference {
        guard identity == nil else { throw IOSClientError.identityAlreadyEnrolled }
        let created = try identityStore.createIdentity()
        let createdDeviceID = UUID().uuidString.lowercased()
        let createdMLSNodeID = UUID().uuidString.lowercased()
        do {
            try saveMetadata(StoredMetadata(
                handle: created.handle,
                publicKey: created.publicKey,
                deviceID: createdDeviceID,
                mlsNodeID: createdMLSNodeID,
                userID: nil))
        } catch {
            try? identityStore.deleteIdentity(created)
            throw error
        }
        identity = created
        deviceID = createdDeviceID
        mlsNodeID = createdMLSNodeID
        return created
    }

    /// Validate the same hardware key after process or device restart.
    public func validateIdentity() throws {
        guard let identity else { throw IOSClientError.identityNotEnrolled }
        try identityStore.validateIdentity(identity)
    }

    /// OTP onboarding calls this after the server returns a device-bound
    /// session. The bearer stays in memory and is never written to disk.
    public func setAuthenticatedSession(userID: String, accessToken: String,
                                        expiresAtMs: UInt64) throws {
        guard let currentDeviceID = deviceID, let currentMLSNodeID = mlsNodeID,
              let identity, Self.isCanonicalUUID(userID), !accessToken.isEmpty,
              expiresAtMs > Self.nowMs() else {
            throw IOSClientError.invalidMetadata
        }
        let metadata = StoredMetadata(
            handle: identity.handle,
            publicKey: identity.publicKey,
            deviceID: currentDeviceID,
            mlsNodeID: currentMLSNodeID,
            userID: userID)
        try saveMetadata(metadata)
        self.userID = userID
        authenticated = AuthenticatedSession(
            userID: userID, accessToken: accessToken, expiresAtMs: expiresAtMs)
    }

    public func accessToken() throws -> String {
        guard isAuthenticated, let authenticated else {
            throw IOSClientError.authenticatedSessionRequired
        }
        return authenticated.accessToken
    }

    /// Sign a Rust-core transcript through the hardware-backed identity.
    /// The seed is loaded and wiped inside HardwareIdentityStore only.
    public func sign(_ transcript: Data) throws -> Data {
        guard let identity else { throw IOSClientError.identityNotEnrolled }
        return try identityStore.sign(identity, transcript: transcript)
    }

    /// Construct the shared core only after account authentication is valid.
    /// The factory must bind this identity to the Rust ClientCore providers.
    public func makeCore(using factory: SharedClientCoreFactory) throws -> any SharedClientCore {
        guard isAuthenticated, let identity, let userID,
              let deviceID, let mlsNodeID else {
            throw IOSClientError.authenticatedSessionRequired
        }
        let coreIdentity = try SharedCoreIdentity(
            userID: userID,
            deviceID: deviceID,
            mlsNodeID: mlsNodeID,
            identity: identity)
        let core = try factory.makeCore(identity: coreIdentity, signer: self)
        guard core.userID == userID, core.deviceID == deviceID else {
            throw IOSClientError.coreIdentityMismatch
        }
        return core
    }

    /// Drop only the memory bearer. Persisted identity and account binding
    /// remain so a returning-device auth flow can validate them.
    public func clearAuthenticatedSession() {
        authenticated = nil
    }

    public static func isCanonicalUUID(_ value: String) -> Bool {
        guard let uuid = UUID(uuidString: value),
              uuid.uuidString.lowercased() == value else { return false }
        return uuid != nilUUID
    }

    private func restoreMetadata() throws {
        guard let encoded = defaults.data(forKey: Self.metadataKey) else { return }
        let metadata: StoredMetadata
        do {
            metadata = try PropertyListDecoder().decode(StoredMetadata.self, from: encoded)
        } catch {
            throw IOSClientError.invalidMetadata
        }
        guard Self.isCanonicalUUID(metadata.deviceID),
              Self.isCanonicalUUID(metadata.mlsNodeID),
              metadata.userID.map(Self.isCanonicalUUID) ?? true else {
            throw IOSClientError.invalidMetadata
        }
        let restored = try IdentityKeyReference(
            handle: metadata.handle,
            publicKey: metadata.publicKey)
        try identityStore.validateIdentity(restored)
        identity = restored
        deviceID = metadata.deviceID
        mlsNodeID = metadata.mlsNodeID
        userID = metadata.userID
    }

    private func saveMetadata(_ metadata: StoredMetadata) throws {
        let encoded: Data
        do {
            encoded = try PropertyListEncoder().encode(metadata)
        } catch {
            throw IOSClientError.metadataUnavailable
        }
        defaults.set(encoded, forKey: Self.metadataKey)
        guard defaults.data(forKey: Self.metadataKey) == encoded else {
            throw IOSClientError.metadataUnavailable
        }
    }

    private static func nowMs() -> UInt64 {
        UInt64(max(0, Date().timeIntervalSince1970 * 1000))
    }
}
