import Foundation
import LinksKeyStore

/// Public identity data passed from the iOS host into the shared Rust core.
/// The hardware seed is never part of this value.
public struct SharedCoreIdentity: Equatable {
    public let userID: String
    public let deviceID: String
    public let mlsNodeID: String
    public let identity: IdentityKeyReference
    public let mlsCredential: Data?

    public init(userID: String, deviceID: String, mlsNodeID: String,
                identity: IdentityKeyReference, mlsCredential: Data? = nil) throws {
        guard IOSClient.isCanonicalUUID(userID),
              IOSClient.isCanonicalUUID(deviceID),
              IOSClient.isCanonicalUUID(mlsNodeID) else {
            throw IOSClientError.invalidMetadata
        }
        self.userID = userID
        self.deviceID = deviceID
        self.mlsNodeID = mlsNodeID
        self.identity = identity
        self.mlsCredential = mlsCredential
    }
}

/// User-visible failure classes from the shared core. Concrete Rust bindings
/// set `SharedClientCore.messagingIssue` when a frame or outbox operation
/// fails; the host never guesses from plaintext or logs.
public enum IOSMessagingIssue: Equatable, Sendable {
    case staleCursor
    case authenticationExpired
    case dependencyOutage
    case sendFailed
}

/// The native adapter for links-client-core. A production implementation
/// owns the Rust ClientCore, MLS provider, envelope provider, and durable store.
/// There is no software or plaintext fallback when this adapter is unavailable.
public protocol SharedClientCore: AnyObject {
    var userID: String { get }
    var deviceID: String { get }
    /// Last typed failure from the Rust binding, if one is available.
    var messagingIssue: IOSMessagingIssue? { get }
    /// Number of encrypted, durable outbox frames awaiting acceptance.
    var pendingOutboxCount: Int { get }
    /// Number of durable frames that still need transport retry, including an
    /// MLS bootstrap that is waiting for the recipient to come online.
    var pendingRetryCount: Int { get }
    func durableCursor() throws -> UInt64
    func createHello(accessToken: String, lastSeenCursor: UInt64) throws -> Data
    /// Return recoveryComplete only after local inbox/MLS commit and QueueAck.
    @discardableResult
    func handleServerFrame(_ frame: Data, transport: any IOSCoreTransport,
                           fullSync: Bool,
                           onTextMessage: (IOSReceivedTextMessage) -> Void)
        throws -> IOSCoreFrameResult
    /// Retry exact persisted outbox frames after reconnect. The binding must
    /// never re-encrypt or create a new message for this operation.
    func retryOutbox(transport: any IOSCoreTransport) throws
    /// Reset only an expired replay cursor before an explicit full recovery.
    /// The binding must preserve encrypted inbox data and MLS state.
    func resetReplayCursorForRecovery() throws
    func maintainPreKeyInventory(accessToken: String, api: any IOSPreKeyAPI)
        async throws -> IOSPreKeyInventory
    func initializeDirectConversation(
        conversationID: String,
        recipientUserID: String,
        recipientDevices: [IOSClaimedRecipientDevice],
        transport: any IOSCoreTransport) throws
    func hasRecipientDevices(for userID: String) -> Bool
    func resetDirectConversation(
        conversationID: String,
        recipientUserID: String,
        recipientDevices: [IOSClaimedRecipientDevice],
        transport: any IOSCoreTransport) throws
    func sendText(conversationID: String, recipientUserID: String, text: String,
                  transport: any IOSCoreTransport) throws
    /// Surface-aware route. Channel publishers can implement this with the
    /// broadcast MLS publisher while the default keeps direct-core support.
    func sendSurfaceText(surfaceID: String, conversationID: String, text: String,
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
    /// Send private video/file metadata only after an exact ciphertext receipt.
    func sendLargeFile(conversationID: String, recipientUserID: String,
                       metadata: IOSLargeFileMetadata,
                       receipt: IOSLargeFileUploadReceipt,
                       transport: any IOSCoreTransport) throws
}

public extension SharedClientCore {
    var messagingIssue: IOSMessagingIssue? { nil }

    var pendingOutboxCount: Int { 0 }

    var pendingRetryCount: Int { pendingOutboxCount }

    func retryOutbox(transport: any IOSCoreTransport) throws {}

    func hasRecipientDevices(for userID: String) -> Bool { false }

    func resetReplayCursorForRecovery() throws {
        throw IOSMessagingError.staleCursorRecoveryUnavailable
    }
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
    case profileMismatch
    case identityReuse
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
        let accountHandle: String?
        let mlsCredential: Data?

        init(handle: String, publicKey: Data, deviceID: String, mlsNodeID: String,
             userID: String?, accountHandle: String? = nil, mlsCredential: Data? = nil) {
            self.handle = handle
            self.publicKey = publicKey
            self.deviceID = deviceID
            self.mlsNodeID = mlsNodeID
            self.userID = userID
            self.accountHandle = accountHandle
            self.mlsCredential = mlsCredential
        }

        private enum CodingKeys: String, CodingKey {
            case handle, publicKey, deviceID, mlsNodeID, userID, accountHandle, mlsCredential
        }

        init(from decoder: Decoder) throws {
            let values = try decoder.container(keyedBy: CodingKeys.self)
            handle = try values.decode(String.self, forKey: .handle)
            publicKey = try values.decode(Data.self, forKey: .publicKey)
            deviceID = try values.decode(String.self, forKey: .deviceID)
            mlsNodeID = try values.decode(String.self, forKey: .mlsNodeID)
            userID = try values.decodeIfPresent(String.self, forKey: .userID)
            accountHandle = try values.decodeIfPresent(String.self, forKey: .accountHandle)
            mlsCredential = try values.decodeIfPresent(Data.self, forKey: .mlsCredential)
        }
    }

    private struct AuthenticatedSession {
        let userID: String
        let accessToken: String
        let expiresAtMs: UInt64
    }

    private static let metadataKey = "links.client.metadata.v1"
    private static let profileMetadataPrefix = "links.client.metadata.v1."
    private static let nilUUID = UUID(uuidString: "00000000-0000-0000-0000-000000000000")!
    private let identityStore: HardwareIdentityStore
    private let defaults: UserDefaults
    public let profile: ClientProfile
    private let metadataKey: String
    private let sessionLock = NSLock()
    private var authenticated: AuthenticatedSession?

    public private(set) var identity: IdentityKeyReference?
    public private(set) var deviceID: String?
    public private(set) var mlsNodeID: String?
    public private(set) var userID: String?
    public private(set) var accountHandle: String?
    public private(set) var mlsCredential: Data?

    public init(identityStore: HardwareIdentityStore = HardwareIdentityStore(),
                defaults: UserDefaults = .standard,
                profile: ClientProfile = .default) throws {
        self.identityStore = identityStore
        self.defaults = defaults
        self.profile = profile
        metadataKey = Self.metadataKey(for: profile)
        guard identityStore.profile == profile else {
            throw IOSClientError.profileMismatch
        }
        try restoreMetadata()
    }

    public var isEnrolled: Bool { identity != nil }

    public var isAuthenticated: Bool {
        sessionLock.lock()
        defer { sessionLock.unlock() }
        guard let authenticated else { return false }
        return authenticated.expiresAtMs > Self.nowMs()
    }

    /// Creates a fresh hardware-backed identity and commits only public
    /// metadata to UserDefaults. The seed remains inside HardwareSeedVault.
    @discardableResult
    public func createIdentity() throws -> IdentityKeyReference {
        guard identity == nil else { throw IOSClientError.identityAlreadyEnrolled }
        let created = try identityStore.createIdentity()
        return try adoptNewIdentity(created)
    }

    /// Generate a local recovery phrase for the UI. Display and confirm it
    /// before calling restoreFromRecovery; the phrase never enters the server.
    public func generateRecoveryMnemonic(wordCount: Int) throws -> String {
        try identityStore.generateRecoveryMnemonic(wordCount: wordCount)
    }

    /// Derive and seal a first identity from an explicit BIP-39 phrase.
    @discardableResult
    public func restoreFromRecovery(_ phrase: String, passphrase: String) throws -> IdentityKeyReference {
        guard identity == nil else { throw IOSClientError.identityAlreadyEnrolled }
        let recovered = try identityStore.restoreFromRecovery(phrase, passphrase: passphrase)
        return try adoptNewIdentity(recovered)
    }

    /// Derive and seal a first identity from a locally evaluated passkey PRF.
    @discardableResult
    public func createFromPasskeyPRF(_ prfOutput: Data) throws -> IdentityKeyReference {
        guard identity == nil else { throw IOSClientError.identityAlreadyEnrolled }
        let created = try identityStore.createFromPasskeyPRF(prfOutput)
        return try adoptNewIdentity(created)
    }

    private func adoptNewIdentity(_ created: IdentityKeyReference) throws -> IdentityKeyReference {
        guard identity == nil else { throw IOSClientError.identityAlreadyEnrolled }
        let createdDeviceID = Self.freshIdentifier()
        let createdMLSNodeID = Self.freshIdentifier(excluding: createdDeviceID)
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
        userID = nil
        accountHandle = nil
        mlsCredential = nil
        sessionLock.lock()
        authenticated = nil
        sessionLock.unlock()
        return created
    }

    /// Validate the same hardware key after process or device restart.
    public func validateIdentity() throws {
        guard let identity else { throw IOSClientError.identityNotEnrolled }
        try identityStore.validateIdentity(identity)
    }

    /// Drop this profile's saved identity so a deleted account can be created again.
    /// The seed is removed from Keychain and the public metadata record is cleared.
    public func erasePersistedIdentity() {
        if let identity {
            try? identityStore.deleteIdentity(identity)
        }
        defaults.removeObject(forKey: metadataKey)
        identity = nil
        deviceID = nil
        mlsNodeID = nil
        userID = nil
        accountHandle = nil
        mlsCredential = nil
        sessionLock.lock()
        authenticated = nil
        sessionLock.unlock()
    }

    /// OTP onboarding calls this after the server returns a device-bound
    /// session. The bearer stays in memory and is never written to disk.
    public func setAuthenticatedSession(userID: String, accessToken: String,
                                        expiresAtMs: UInt64,
                                        accountHandle: String? = nil,
                                        mlsCredential: Data? = nil) throws {
        guard let currentDeviceID = deviceID, let currentMLSNodeID = mlsNodeID,
              let identity, Self.isCanonicalUUID(userID), !accessToken.isEmpty,
              expiresAtMs > Self.nowMs(),
              mlsCredential.map({ !$0.isEmpty && $0.count <= 1024 }) ?? true else {
            throw IOSClientError.invalidMetadata
        }
        guard self.userID == nil || self.userID == userID else {
            throw IOSClientError.identityReuse
        }
        if let storedHandle = self.accountHandle {
            guard accountHandle == nil || accountHandle == storedHandle else {
                throw IOSClientError.identityReuse
            }
        }
        if let storedCredential = self.mlsCredential {
            guard mlsCredential == nil || mlsCredential == storedCredential else {
                throw IOSClientError.identityReuse
            }
        }
        let metadata = StoredMetadata(
            handle: identity.handle,
            publicKey: identity.publicKey,
            deviceID: currentDeviceID,
            mlsNodeID: currentMLSNodeID,
            userID: userID,
            accountHandle: accountHandle ?? self.accountHandle,
            mlsCredential: mlsCredential ?? self.mlsCredential)
        try saveMetadata(metadata)
        self.userID = userID
        self.accountHandle = accountHandle ?? self.accountHandle
        self.mlsCredential = mlsCredential ?? self.mlsCredential
        sessionLock.lock()
        authenticated = AuthenticatedSession(
            userID: userID, accessToken: accessToken, expiresAtMs: expiresAtMs)
        sessionLock.unlock()
    }

    public func accessToken() throws -> String {
        sessionLock.lock()
        defer { sessionLock.unlock() }
        guard let authenticated, authenticated.expiresAtMs > Self.nowMs() else {
            throw IOSClientError.authenticatedSessionRequired
        }
        return authenticated.accessToken
    }

    /// Register or log in a local-development username using this hardware
    /// identity. The username is not a password; the identity signs a fresh
    /// nonce-bound transcript and the bearer remains memory-only.
    @discardableResult
    public func registerUsername(using api: IOSUsernameAuthClient, handle: String)
        async throws -> IOSUsernameAuthSession {
        try await usernameAuthentication(using: api, handle: handle, login: false)
    }

    @discardableResult
    public func loginUsername(using api: IOSUsernameAuthClient, handle: String)
        async throws -> IOSUsernameAuthSession {
        try await usernameAuthentication(using: api, handle: handle, login: true)
    }

    /// Create a fresh signed `links://connect` payload for this local device.
    /// The approving device must authenticate the account and submit it before
    /// this client can use the account.
    public func makePairingURI(for userID: String) throws -> String {
        guard let identity, let deviceID, let mlsNodeID,
              Self.isCanonicalUUID(userID),
              Self.isCanonicalUUID(deviceID),
              Self.isCanonicalUUID(mlsNodeID) else {
            throw IOSClientError.identityNotEnrolled
        }
        let unsigned = try IOSPairingPayload(
            userID: userID, deviceID: deviceID, mlsNodeID: mlsNodeID,
            publicKey: identity.publicKey,
            nonce: IOSPairingPayload.generateNonce(),
            signature: Data(repeating: 0, count: 64))
        let signature = try sign(try unsigned.signingTranscript())
        let signed = try IOSPairingPayload(
            userID: unsigned.userID, deviceID: unsigned.deviceID,
            mlsNodeID: unsigned.mlsNodeID, publicKey: unsigned.publicKey,
            nonce: unsigned.nonce, signature: signature)
        return try signed.toURI()
    }

    /// Approve a pairing link while this client has a valid bearer session.
    /// Signature, account, and response identity fields are checked before
    /// the server-created credential is returned to the caller.
    @discardableResult
    public func approvePairing(using api: IOSUsernameAuthClient, uri: String)
        async throws -> IOSPairingRegistrationResponse {
        guard let userID, isAuthenticated else {
            throw IOSClientError.authenticatedSessionRequired
        }
        let payload = try IOSPairingPayload(uri: uri)
        try payload.verify()
        guard payload.userID == userID else { throw IOSPairingError.invalidPayload }
        let response = try await api.registerDevice(
            accessToken: accessToken(), payload: payload)
        guard response.userID == payload.userID,
              response.deviceID == payload.deviceID,
              response.mlsNodeID == payload.mlsNodeID,
              response.publicKey == payload.publicKey else {
            throw IOSPairingError.invalidPayload
        }
        return response
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
            expiresAtMs: session.expiresAtMs,
            mlsCredential: challenge.mlsCredential)
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
            identity: identity,
            mlsCredential: mlsCredential)
        let core = try factory.makeCore(identity: coreIdentity, signer: self)
        guard core.userID == userID, core.deviceID == deviceID else {
            throw IOSClientError.coreIdentityMismatch
        }
        return core
    }

    /// Drop only the memory bearer. Persisted identity and account binding
    /// remain so a returning-device auth flow can validate them.
    public func clearAuthenticatedSession() {
        sessionLock.lock()
        authenticated = nil
        sessionLock.unlock()
    }

    public static func isCanonicalUUID(_ value: String) -> Bool {
        guard let uuid = UUID(uuidString: value),
              uuid.uuidString.lowercased() == value else { return false }
        return uuid != nilUUID
    }

    private func restoreMetadata() throws {
        guard let encoded = defaults.data(forKey: metadataKey) else { return }
        let metadata: StoredMetadata
        do {
            metadata = try PropertyListDecoder().decode(StoredMetadata.self, from: encoded)
        } catch {
            throw IOSClientError.invalidMetadata
        }
        guard Self.isCanonicalUUID(metadata.deviceID),
              Self.isCanonicalUUID(metadata.mlsNodeID),
              metadata.userID.map(Self.isCanonicalUUID) ?? true,
              metadata.accountHandle.map({
                  (try? IOSUsernameAuthClient.validateHandle($0)) != nil
              }) ?? true,
              metadata.mlsCredential.map({ !$0.isEmpty && $0.count <= 1024 }) ?? true else {
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
        accountHandle = metadata.accountHandle
        mlsCredential = metadata.mlsCredential
    }

    private func validate(_ challenge: IOSOTPChallenge, identity: IdentityKeyReference,
                          deviceID: String, mlsNodeID: String) throws {
        guard challenge.deviceID == deviceID,
              challenge.mlsNodeID == mlsNodeID,
              challenge.publicKey == identity.publicKey else {
            throw IOSOTPError.challengeIdentityMismatch
        }
    }

    private func usernameAuthentication(using api: IOSUsernameAuthClient, handle: String,
                                        login: Bool) async throws -> IOSUsernameAuthSession {
        guard let identity, let deviceID, let mlsNodeID,
              let deviceUUID = UUID(uuidString: deviceID),
              let nodeUUID = UUID(uuidString: mlsNodeID) else {
            throw IOSClientError.identityNotEnrolled
        }
        let cleanHandle = handle.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        try IOSUsernameAuthClient.validateHandle(cleanHandle)
        let purpose = login ? "login" : "registration"
        let challenge = try await api.startAuthentication(
            handle: cleanHandle, purpose: purpose, deviceID: deviceID,
            mlsNodeID: mlsNodeID, publicKey: identity.publicKey)
        guard challenge.handle == cleanHandle,
              challenge.purpose == purpose,
              challenge.deviceID == deviceID,
              challenge.mlsNodeID == mlsNodeID,
              challenge.publicKey == identity.publicKey,
              challenge.expiresAtMs > Self.nowMs(),
              let challengeUUID = UUID(uuidString: challenge.challengeID) else {
            throw IOSClientError.invalidMetadata
        }
        var transcript = try usernameTranscript(
            challengeID: challengeUUID, handle: cleanHandle,
            deviceID: deviceUUID, mlsNodeID: nodeUUID,
            publicKey: identity.publicKey, challenge: challenge.challenge,
            expiresAtMs: challenge.expiresAtMs, login: login)
        var signature = try sign(transcript)
        defer {
            transcript.resetBytes(in: 0..<transcript.count)
            signature.resetBytes(in: 0..<signature.count)
        }
        let session: IOSUsernameAuthSession
        if login {
            session = try await api.login(
                challengeID: challenge.challengeID, signature: signature)
        } else {
            session = try await api.register(
                challengeID: challenge.challengeID, signature: signature)
        }
        guard session.deviceID == deviceID, session.handle == cleanHandle else {
            throw IOSClientError.invalidMetadata
        }
        try setAuthenticatedSession(
            userID: session.userID,
            accessToken: session.accessToken,
            expiresAtMs: session.expiresAtMs,
            accountHandle: session.handle,
            mlsCredential: session.mlsCredential)
        return session
    }

    private func usernameTranscript(challengeID: UUID, handle: String,
                                    deviceID: UUID, mlsNodeID: UUID,
                                    publicKey: Data, challenge: Data,
                                    expiresAtMs: UInt64, login: Bool) throws -> Data {
        let handleBytes = Array(handle.utf8)
        guard handleBytes.count >= 3, handleBytes.count <= 32,
              publicKey.count == 32, challenge.count == 32, expiresAtMs > 0 else {
            throw IOSClientError.invalidMetadata
        }
        var transcript = Data((login
            ? "links/username-login/v2\0"
            : "links/username-register/v2\0").utf8)
        transcript.append(contentsOf: challengeID.bytes)
        transcript.append(UInt8(handleBytes.count))
        transcript.append(contentsOf: handleBytes)
        transcript.append(contentsOf: deviceID.bytes)
        transcript.append(contentsOf: mlsNodeID.bytes)
        transcript.append(publicKey)
        transcript.append(challenge)
        var expiry = expiresAtMs.bigEndian
        withUnsafeBytes(of: &expiry) { transcript.append(contentsOf: $0) }
        return transcript
    }

    private func saveMetadata(_ metadata: StoredMetadata) throws {
        let encoded: Data
        do {
            encoded = try PropertyListEncoder().encode(metadata)
        } catch {
            throw IOSClientError.metadataUnavailable
        }
        defaults.set(encoded, forKey: metadataKey)
        guard defaults.data(forKey: metadataKey) == encoded else {
            throw IOSClientError.metadataUnavailable
        }
    }

    private static func nowMs() -> UInt64 {
        UInt64(max(0, Date().timeIntervalSince1970 * 1000))
    }

    private static func freshIdentifier(excluding: String? = nil) -> String {
        let nilValue = nilUUID.uuidString.lowercased()
        while true {
            let value = UUID().uuidString.lowercased()
            if value != nilValue && value != excluding {
                return value
            }
        }
    }

    private static func metadataKey(for profile: ClientProfile) -> String {
        profile == .default ? metadataKey : profileMetadataPrefix + profile.name
    }
}
