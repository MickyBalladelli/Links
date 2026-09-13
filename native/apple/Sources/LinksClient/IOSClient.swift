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
    /// Return recoveryComplete only after local inbox/MLS commit and QueueAck.
    @discardableResult
    func handleServerFrame(_ frame: Data, transport: any IOSCoreTransport,
                           fullSync: Bool,
                           onTextMessage: @escaping (IOSReceivedTextMessage) -> Void)
        throws -> IOSCoreFrameResult
    func sendText(conversationID: String, recipientUserID: String, text: String,
                  transport: any IOSCoreTransport) throws
    /// Encode normalized RGB pixels with the shared fixed BlurHash contract.
    func encodeImageBlurHash(rgbPixels: Data, width: Int, height: Int) throws -> String
    /// Encrypt normalized image bytes; metadata stays inside MLS.
    func encryptImage(_ image: Data, attachmentID: String, mimeType: String,
                      width: Int, height: Int, blurHash: String) throws -> IOSEncryptedImage
    /// Verify image metadata, digest, AEAD, and dimensions before rendering.
    func decryptImage(_ metadata: IOSImageMetadata, ciphertext: Data) throws -> Data
    /// Send private image metadata only after its ciphertext upload receipt.
    func sendImage(conversationID: String, recipientUserID: String,
                   metadata: IOSImageMetadata, receipt: IOSImageUploadReceipt,
                   transport: any IOSCoreTransport) throws
    /// Encode PCM with the shared Opus profile and return a complete Ogg Opus container.
    func encodeVoiceNote(pcmFrames: [Int16], profile: IOSVoiceNoteProfile) throws -> Data
    /// Encrypt the complete container; metadata stays inside the MLS Message.
    func encryptVoiceNote(_ container: Data, attachmentID: String,
                          durationMs: UInt64, profile: IOSVoiceNoteProfile)
        throws -> IOSEncryptedVoiceNote
    /// Verify attachment metadata, digest, AEAD and Opus framing before playback.
    func decryptVoiceNote(_ metadata: IOSVoiceNoteMetadata, ciphertext: Data) throws -> Data
    /// Decode verified Opus into PCM for AVAudioEngine playback.
    func decodeVoiceNote(_ container: Data, profile: IOSVoiceNoteProfile) throws -> [Int16]
    /// Send private media metadata after the opaque upload accepted exact ciphertext.
    func sendVoiceNote(conversationID: String, recipientUserID: String,
                       metadata: IOSVoiceNoteMetadata,
                       receipt: IOSVoiceNoteUploadReceipt,
                       transport: any IOSCoreTransport) throws
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
/// Transport, message UI, APNs and recovery are separate host layers.
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

    /// Start phone verification with a proof signed by this hardware identity.
    public func startOTP(using api: IOSOTPClient, phone: String,
                         channel: IOSOTPChannel) async throws -> IOSOTPChallenge {
        guard let identity, let deviceID, let mlsNodeID,
              let deviceUUID = UUID(uuidString: deviceID),
              let nodeUUID = UUID(uuidString: mlsNodeID) else {
            throw IOSClientError.identityNotEnrolled
        }
        var transcript = try identityStore.phoneAuthTranscript(
            identity, phone: phone, channel: channel.rawValue,
            deviceID: deviceUUID, mlsNodeID: nodeUUID)
        var signature = try sign(transcript)
        defer {
            transcript.resetBytes(in: 0..<transcript.count)
            signature.resetBytes(in: 0..<signature.count)
        }
        let challenge = try await api.start(
            phone: phone, channel: channel, deviceID: deviceID,
            mlsNodeID: mlsNodeID, publicKey: identity.publicKey, signature: signature)
        try validate(challenge, identity: identity, deviceID: deviceID, mlsNodeID: mlsNodeID)
        return challenge
    }

    /// Finish phone verification with a fresh enrollment proof, then keep the
    /// returned bearer only in memory through setAuthenticatedSession().
    @discardableResult
    public func finishOTP(using api: IOSOTPClient, challenge: IOSOTPChallenge,
                          code: String) async throws -> IOSOTPAuthSession {
        guard let identity, let deviceID, let mlsNodeID,
              let userUUID = UUID(uuidString: challenge.userID),
              let deviceUUID = UUID(uuidString: deviceID),
              let nodeUUID = UUID(uuidString: mlsNodeID),
              let challengeUUID = UUID(uuidString: challenge.challengeID) else {
            throw IOSClientError.identityNotEnrolled
        }
        try validate(challenge, identity: identity, deviceID: deviceID, mlsNodeID: mlsNodeID)
        guard challenge.expiresAtMs > IOSOTPClient.nowMs() else {
            throw IOSOTPError.invalidChallenge
        }
        var transcript = try identityStore.enrollmentTranscript(
            identity, userID: userUUID, deviceID: deviceUUID, mlsNodeID: nodeUUID,
            challengeID: challengeUUID, nonce: challenge.nonce,
            expiresAtMs: challenge.expiresAtMs, mlsCredential: challenge.mlsCredential)
        var signature = try sign(transcript)
        defer {
            transcript.resetBytes(in: 0..<transcript.count)
            signature.resetBytes(in: 0..<signature.count)
        }
        let session = try await api.finish(
            challengeID: challenge.challengeID, code: code, signature: signature)
        guard session.userID == challenge.userID, session.deviceID == deviceID else {
            throw IOSOTPError.challengeIdentityMismatch
        }
        try setAuthenticatedSession(
            userID: session.userID, accessToken: session.accessToken,
            expiresAtMs: session.expiresAtMs)
        return session
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

    private func validate(_ challenge: IOSOTPChallenge, identity: IdentityKeyReference,
                          deviceID: String, mlsNodeID: String) throws {
        guard challenge.deviceID == deviceID,
              challenge.mlsNodeID == mlsNodeID,
              challenge.publicKey == identity.publicKey else {
            throw IOSOTPError.challengeIdentityMismatch
        }
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
