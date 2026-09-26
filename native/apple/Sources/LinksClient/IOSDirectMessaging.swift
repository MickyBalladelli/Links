import Foundation

public protocol IOSCoreTransport: AnyObject {
    @discardableResult
    func send(_ frame: Data) -> Bool
}

public enum IOSCoreFrameResult: Equatable, Sendable {
    case pending
    case recoveryComplete
}

public struct IOSReceivedTextMessage: Sendable {
    public let conversationID: String
    public let senderUserID: String
    public let senderDeviceID: String
    public let text: String
    public let sequenceID: UInt64
    public let sentAtMs: UInt64

    public init(conversationID: String, senderUserID: String, senderDeviceID: String, text: String,
                sequenceID: UInt64, sentAtMs: UInt64) throws {
        guard IOSClient.isCanonicalUUID(conversationID),
              IOSClient.isCanonicalUUID(senderUserID),
              IOSClient.isCanonicalUUID(senderDeviceID),
              !text.isEmpty,
              text.utf8.count <= IOSInternalTextMilestone.maximumTextBytes,
              sequenceID > 0 else {
            throw IOSMessagingError.invalidMessage
        }
        self.conversationID = conversationID
        self.senderUserID = senderUserID
        self.senderDeviceID = senderDeviceID
        self.text = text
        self.sequenceID = sequenceID
        self.sentAtMs = sentAtMs
    }
}

public struct IOSReceivedImageMessage: Sendable {
    public let conversationID: String
    public let senderUserID: String
    public let senderDeviceID: String
    public let metadataProtobuf: Data
    public let sequenceID: UInt64
    public let sentAtMs: UInt64

    public init(conversationID: String, senderUserID: String, senderDeviceID: String,
                metadataProtobuf: Data, sequenceID: UInt64, sentAtMs: UInt64) throws {
        guard IOSClient.isCanonicalUUID(conversationID),
              IOSClient.isCanonicalUUID(senderUserID),
              IOSClient.isCanonicalUUID(senderDeviceID), sequenceID > 0 else {
            throw IOSMessagingError.invalidMessage
        }
        _ = try IOSImageMetadata(protobuf: metadataProtobuf)
        self.conversationID = conversationID
        self.senderUserID = senderUserID
        self.senderDeviceID = senderDeviceID
        self.metadataProtobuf = metadataProtobuf
        self.sequenceID = sequenceID
        self.sentAtMs = sentAtMs
    }
}

public struct IOSReceivedFileMessage: Sendable {
    public let conversationID: String
    public let senderUserID: String
    public let senderDeviceID: String
    public let metadataProtobuf: Data
    public let fileName: String
    public let mimeType: String
    public let sequenceID: UInt64
    public let sentAtMs: UInt64

    public init(conversationID: String, senderUserID: String, senderDeviceID: String,
                metadataProtobuf: Data, sequenceID: UInt64, sentAtMs: UInt64) throws {
        let parsed = try IOSFileMetadata(protobuf: metadataProtobuf)
        guard IOSClient.isCanonicalUUID(conversationID),
              IOSClient.isCanonicalUUID(senderUserID),
              IOSClient.isCanonicalUUID(senderDeviceID) else {
            throw IOSImageError.invalidMetadata
        }
        self.conversationID = conversationID
        self.senderUserID = senderUserID
        self.senderDeviceID = senderDeviceID
        self.metadataProtobuf = metadataProtobuf
        self.fileName = parsed.fileName
        self.mimeType = parsed.mimeType
        self.sequenceID = sequenceID
        self.sentAtMs = sentAtMs
    }
}

public protocol IOSDirectMessagingDelegate: AnyObject {
    func directMessaging(_ messaging: IOSDirectMessaging,
                         didChange state: IOSDirectMessaging.State)
    func directMessaging(_ messaging: IOSDirectMessaging,
                         didReceive message: IOSReceivedTextMessage)
    func directMessaging(_ messaging: IOSDirectMessaging,
                         didReceive image: IOSReceivedImageMessage)
    func directMessaging(_ messaging: IOSDirectMessaging,
                         didReceive file: IOSReceivedFileMessage)
    func directMessagingDidFail(_ messaging: IOSDirectMessaging)
    func directMessagingDidFail(_ messaging: IOSDirectMessaging,
                                reason: IOSMessagingIssue)
    func directMessaging(_ messaging: IOSDirectMessaging,
                         didReceive event: IOSGroupEvent)
}

public extension IOSDirectMessagingDelegate {
    func directMessaging(_ messaging: IOSDirectMessaging,
                         didReceive image: IOSReceivedImageMessage) {}

    func directMessaging(_ messaging: IOSDirectMessaging,
                         didReceive file: IOSReceivedFileMessage) {}

    func directMessaging(_ messaging: IOSDirectMessaging,
                         didReceive event: IOSGroupEvent) {}

    func directMessagingDidFail(_ messaging: IOSDirectMessaging,
                                reason: IOSMessagingIssue) {
        directMessagingDidFail(messaging)
    }
}

/// Connected iOS one-to-one text flow. Shared Rust core owns MLS, Sealed
/// Sender, durable outbox/inbox state, cursors and delivery receipts.
public final class IOSDirectMessaging: IOSConnectionManagerDelegate {
    public enum State: Equatable, Sendable {
        case stopped
        case connecting
        case ready
        case reconnecting
        case staleCursor
        case authenticationRequired
        case dependencyOutage
        case sendFailed
        case failed
    }

    public static let maximumTextBytes = IOSInternalTextMilestone.maximumTextBytes

    private let client: IOSClient
    private let factory: any SharedClientCoreFactory
    private let endpoint: URL
    private let localDevelopmentRootCertificateData: Data?
    private weak var delegate: IOSDirectMessagingDelegate?
    private let callbackQueue: DispatchQueue
    private let coreQueue = DispatchQueue(
        label: "ai.links.ios.messaging.core", qos: .userInitiated)
    private let lock = NSLock()
    private var core: (any SharedClientCore)?
    private var connection: IOSConnectionManager?
    private var retryTimer: DispatchSourceTimer?
    private var currentState = State.stopped
    private var coreFailed = false

    public init(client: IOSClient, factory: any SharedClientCoreFactory, endpoint: URL,
                delegate: IOSDirectMessagingDelegate,
                callbackQueue: DispatchQueue = .main,
                localDevelopmentRootCertificateData: Data? = nil) {
        self.client = client
        self.factory = factory
        self.endpoint = endpoint
        self.localDevelopmentRootCertificateData = localDevelopmentRootCertificateData
        self.delegate = delegate
        self.callbackQueue = callbackQueue
    }

    public var state: State {
        lock.lock()
        defer { lock.unlock() }
        return currentState
    }

    public var isConnected: Bool {
        lock.lock()
        defer { lock.unlock() }
        return currentState == .ready && connection?.isConnected == true
    }

    public var pendingOutboxCount: Int {
        lock.lock()
        defer { lock.unlock() }
        return core?.pendingOutboxCount ?? 0
    }

    public func hasRecipientDevices(for userID: String) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return core?.hasRecipientDevices(for: userID) ?? false
    }

    public func start() throws {
        lock.lock()
        let alreadyStarted = currentState == .connecting || currentState == .ready
        lock.unlock()
        if alreadyStarted { return }

        let sharedCore = try client.makeCore(using: factory)
        // Events fire inside frame handling, before the texts that follow them
        // are rendered, so a newly joined group exists when its messages land.
        sharedCore.setGroupEventHandler { [weak self] event in
            guard let self else { return }
            self.callbackQueue.async { [weak self] in
                guard let self else { return }
                self.delegate?.directMessaging(self, didReceive: event)
            }
        }
        let manager = try IOSConnectionManager(
            endpoint: endpoint,
            helloProvider: { [client, sharedCore] in
                let cursor = try sharedCore.durableCursor()
                let token = try client.accessToken()
                return try sharedCore.createHello(accessToken: token, lastSeenCursor: cursor)
            },
            delegate: self,
            callbackQueue: coreQueue,
            localDevelopmentRootCertificateData: localDevelopmentRootCertificateData)
        lock.lock()
        core = sharedCore
        connection = manager
        coreFailed = false
        let nextState: State = currentState == .stopped ? .connecting : .reconnecting
        currentState = nextState
        lock.unlock()
        notifyState(nextState)
        manager.start()
    }

    public func stop() {
        lock.lock()
        let manager = connection
        connection = nil
        core = nil
        retryTimer?.cancel()
        retryTimer = nil
        coreFailed = false
        currentState = .stopped
        lock.unlock()
        manager?.stop()
        notifyState(.stopped)
    }

    /// Clear an expired server cursor only through the shared durable core,
    /// then reconnect with a full replay from cursor zero.
    public func recoverFromStaleCursor() throws {
        lock.lock()
        let sharedCore = core
        let manager = connection
        let stale = currentState == .staleCursor
        lock.unlock()
        guard let sharedCore, let manager, stale else {
            throw IOSMessagingError.staleCursorRecoveryUnavailable
        }
        try sharedCore.resetReplayCursorForRecovery()
        lock.lock()
        coreFailed = false
        currentState = .connecting
        lock.unlock()
        notifyState(.connecting)
        manager.stop()
        manager.start()
    }

    /// Ask the shared Rust core to generate the first profile and replenish
    /// both one-time pools. The core owns its durable pending-upload record;
    /// this host only supplies the authenticated protobuf API.
    public func maintainPreKeyInventory(using api: any IOSPreKeyAPI)
        async throws -> IOSPreKeyInventory {
        lock.lock()
        let sharedCore = core
        let active = currentState != .stopped
        lock.unlock()
        guard let sharedCore, active else { throw IOSMessagingError.notConnected }
        let token = try client.accessToken()
        return try await sharedCore.maintainPreKeyInventory(
            accessToken: token, api: api)
    }

    /// Discover the recipient's active devices, claim one bundle per device,
    /// and let the shared core verify the claims and stage the first two-user
    /// MLS commit. KeyPackages are supplied by the authenticated directory
    /// adapter and never constructed by this UI layer.
    public func initializeFirstDirectConversation(
        conversationID: String,
        recipientUserID: String,
        directory: any IOSDirectChatDirectory,
        preKeyAPI: any IOSPreKeyAPI) async throws {
        guard Self.isValidTextID(conversationID),
              Self.isValidTextID(recipientUserID) else {
            throw IOSMessagingError.invalidMessage
        }
        lock.lock()
        let sharedCore = core
        let manager = connection
        let ready = currentState == .ready && !coreFailed
        lock.unlock()
        guard let sharedCore, let manager, ready, manager.isConnected else {
            throw IOSMessagingError.notConnected
        }
        let token = try client.accessToken()
        let descriptors = try await directory.queryRecipientDevices(
            accessToken: token, recipientUserID: recipientUserID)
        guard !descriptors.isEmpty, descriptors.count <= 100,
              descriptors.allSatisfy({ $0.userID == recipientUserID }) else {
            throw IOSPreKeyError.invalidRecipient
        }
        var deviceIDs = Set<String>()
        var claimed = [IOSClaimedRecipientDevice]()
        claimed.reserveCapacity(descriptors.count)
        for descriptor in descriptors {
            guard deviceIDs.insert(descriptor.deviceID).inserted else {
                throw IOSPreKeyError.invalidRecipient
            }
            let bundle = try await preKeyAPI.claim(
                accessToken: token, deviceID: descriptor.deviceID)
            claimed.append(try IOSClaimedRecipientDevice(
                descriptor: descriptor, bundle: bundle))
        }
        try coreQueue.sync {
            try sharedCore.initializeDirectConversation(
                conversationID: conversationID,
                recipientUserID: recipientUserID,
                recipientDevices: claimed,
                transport: manager)
        }
        scheduleRetry(for: manager)
    }

    /// Recreate a direct MLS group when an earlier bootstrap was lost. The
    /// recipient explicitly discards the stuck mailbox batch after it joins.
    public func resetFirstDirectConversation(
        conversationID: String,
        recipientUserID: String,
        directory: any IOSDirectChatDirectory,
        preKeyAPI: any IOSPreKeyAPI) async throws {
        guard Self.isValidTextID(conversationID),
              Self.isValidTextID(recipientUserID) else {
            throw IOSMessagingError.invalidMessage
        }
        lock.lock()
        let sharedCore = core
        let manager = connection
        let ready = currentState == .ready && !coreFailed
        lock.unlock()
        guard let sharedCore, let manager, ready, manager.isConnected else {
            throw IOSMessagingError.notConnected
        }
        let token = try client.accessToken()
        let descriptors = try await directory.queryRecipientDevices(
            accessToken: token, recipientUserID: recipientUserID)
        guard !descriptors.isEmpty, descriptors.count <= 100,
              descriptors.allSatisfy({ $0.userID == recipientUserID }) else {
            throw IOSPreKeyError.invalidRecipient
        }
        var deviceIDs = Set<String>()
        var claimed = [IOSClaimedRecipientDevice]()
        claimed.reserveCapacity(descriptors.count)
        for descriptor in descriptors {
            guard deviceIDs.insert(descriptor.deviceID).inserted else {
                throw IOSPreKeyError.invalidRecipient
            }
            let bundle = try await preKeyAPI.claim(
                accessToken: token, deviceID: descriptor.deviceID)
            claimed.append(try IOSClaimedRecipientDevice(
                descriptor: descriptor, bundle: bundle))
        }
        try coreQueue.sync {
            try sharedCore.resetDirectConversation(
                conversationID: conversationID,
                recipientUserID: recipientUserID,
                recipientDevices: claimed,
                transport: manager)
        }
        scheduleRetry(for: manager)
    }

    // MARK: Groups

    private func readyCore() throws -> (any SharedClientCore, IOSConnectionManager) {
        lock.lock()
        let sharedCore = core
        let manager = connection
        let ready = currentState == .ready && !coreFailed
        lock.unlock()
        guard let sharedCore, let manager, ready, manager.isConnected else {
            throw IOSMessagingError.notConnected
        }
        return (sharedCore, manager)
    }

    /// Fetch each user's devices from the directory, claim one pre-key bundle
    /// per device and register them so the core can seal to them.
    private func registerDevices(of userIDs: [String], core sharedCore: any SharedClientCore,
                                 directory: any IOSDirectChatDirectory,
                                 preKeyAPI: any IOSPreKeyAPI) async throws {
        let token = try client.accessToken()
        var claimed = [IOSClaimedRecipientDevice]()
        for userID in userIDs {
            guard Self.isValidTextID(userID) else { throw IOSMessagingError.invalidMessage }
            let descriptors = try await directory.queryRecipientDevices(
                accessToken: token, recipientUserID: userID)
            guard !descriptors.isEmpty, descriptors.count <= 100,
                  descriptors.allSatisfy({ $0.userID == userID }) else {
                throw IOSPreKeyError.invalidRecipient
            }
            for descriptor in descriptors {
                let bundle = try await preKeyAPI.claim(
                    accessToken: token, deviceID: descriptor.deviceID)
                claimed.append(try IOSClaimedRecipientDevice(descriptor: descriptor, bundle: bundle))
            }
        }
        guard !claimed.isEmpty else { return }
        try coreQueue.sync { try sharedCore.registerRecipientDevices(claimed) }
    }

    /// Register any member device the core cannot seal to yet.
    private func prepareGroupRecipients(conversationID: String, core sharedCore: any SharedClientCore,
                                        directory: any IOSDirectChatDirectory,
                                        preKeyAPI: any IOSPreKeyAPI) async throws {
        let missing = try coreQueue.sync {
            try sharedCore.groupUsersMissingRecipients(conversationID: conversationID)
        }
        try await registerDevices(of: missing, core: sharedCore,
                                  directory: directory, preKeyAPI: preKeyAPI)
    }

    public func createGroup(conversationID: String) throws {
        guard Self.isValidTextID(conversationID) else { throw IOSMessagingError.invalidMessage }
        let (sharedCore, _) = try readyCore()
        try coreQueue.sync { try sharedCore.createGroup(conversationID: conversationID) }
    }

    public func inviteToGroup(conversationID: String, userIDs: [String],
                              directory: any IOSDirectChatDirectory,
                              preKeyAPI: any IOSPreKeyAPI) async throws {
        guard Self.isValidTextID(conversationID), !userIDs.isEmpty else {
            throw IOSMessagingError.invalidMessage
        }
        let (sharedCore, manager) = try readyCore()
        try await prepareGroupRecipients(conversationID: conversationID, core: sharedCore,
                                         directory: directory, preKeyAPI: preKeyAPI)
        try await registerDevices(of: userIDs, core: sharedCore,
                                  directory: directory, preKeyAPI: preKeyAPI)
        try coreQueue.sync {
            try sharedCore.addGroupMembers(conversationID: conversationID, userIDs: userIDs,
                                           transport: manager)
        }
        scheduleRetry(for: manager)
    }

    public func removeFromGroup(conversationID: String, userID: String,
                                directory: any IOSDirectChatDirectory,
                                preKeyAPI: any IOSPreKeyAPI) async throws {
        guard Self.isValidTextID(conversationID), Self.isValidTextID(userID) else {
            throw IOSMessagingError.invalidMessage
        }
        let (sharedCore, manager) = try readyCore()
        try await prepareGroupRecipients(conversationID: conversationID, core: sharedCore,
                                         directory: directory, preKeyAPI: preKeyAPI)
        try coreQueue.sync {
            try sharedCore.removeGroupMember(conversationID: conversationID, userID: userID,
                                             transport: manager)
        }
        scheduleRetry(for: manager)
    }

    public func sendGroupText(conversationID: String, text: String,
                              directory: any IOSDirectChatDirectory,
                              preKeyAPI: any IOSPreKeyAPI) async throws {
        guard Self.isValidTextID(conversationID), !text.isEmpty,
              text.utf8.count <= Self.maximumTextBytes else {
            throw IOSMessagingError.invalidMessage
        }
        let (sharedCore, manager) = try readyCore()
        try await prepareGroupRecipients(conversationID: conversationID, core: sharedCore,
                                         directory: directory, preKeyAPI: preKeyAPI)
        try coreQueue.sync {
            try sharedCore.sendGroupText(conversationID: conversationID, text: text,
                                         transport: manager)
        }
        scheduleRetry(for: manager)
    }

    public func sendGroupImage(conversationID: String, metadata: IOSImageMetadata,
                               receipt: IOSImageUploadReceipt,
                               directory: any IOSDirectChatDirectory,
                               preKeyAPI: any IOSPreKeyAPI) async throws {
        guard Self.isValidTextID(conversationID), receipt.matches(metadata) else {
            throw IOSMessagingError.invalidMessage
        }
        let (sharedCore, manager) = try readyCore()
        try await prepareGroupRecipients(conversationID: conversationID, core: sharedCore,
                                         directory: directory, preKeyAPI: preKeyAPI)
        try coreQueue.sync {
            try sharedCore.sendGroupImage(conversationID: conversationID,
                                          metadata: metadata, transport: manager)
        }
        scheduleRetry(for: manager)
    }

    public func setGroupName(conversationID: String, name: String,
                             directory: any IOSDirectChatDirectory,
                             preKeyAPI: any IOSPreKeyAPI) async throws {
        guard Self.isValidTextID(conversationID) else { throw IOSMessagingError.invalidMessage }
        let (sharedCore, manager) = try readyCore()
        try await prepareGroupRecipients(conversationID: conversationID, core: sharedCore,
                                         directory: directory, preKeyAPI: preKeyAPI)
        try coreQueue.sync {
            try sharedCore.setGroupName(conversationID: conversationID, name: name,
                                        transport: manager)
        }
        scheduleRetry(for: manager)
    }

    public func disbandGroup(conversationID: String, directory: any IOSDirectChatDirectory,
                             preKeyAPI: any IOSPreKeyAPI) async throws {
        guard Self.isValidTextID(conversationID) else { throw IOSMessagingError.invalidMessage }
        let (sharedCore, manager) = try readyCore()
        try await prepareGroupRecipients(conversationID: conversationID, core: sharedCore,
                                         directory: directory, preKeyAPI: preKeyAPI)
        try coreQueue.sync {
            try sharedCore.disbandGroup(conversationID: conversationID, transport: manager)
        }
        scheduleRetry(for: manager)
    }

    public func leaveGroup(conversationID: String) throws {
        guard Self.isValidTextID(conversationID) else { throw IOSMessagingError.invalidMessage }
        lock.lock()
        let sharedCore = core
        lock.unlock()
        guard let sharedCore else { throw IOSMessagingError.notConnected }
        try coreQueue.sync { try sharedCore.leaveGroup(conversationID: conversationID) }
    }

    public func groupMembers(conversationID: String) -> [String] {
        lock.lock()
        let sharedCore = core
        lock.unlock()
        guard let sharedCore else { return [] }
        return (try? coreQueue.sync {
            try sharedCore.groupMembers(conversationID: conversationID)
        }) ?? []
    }

    public func shutdown() {
        lock.lock()
        let manager = connection
        connection = nil
        core = nil
        retryTimer?.cancel()
        retryTimer = nil
        coreFailed = false
        currentState = .stopped
        lock.unlock()
        manager?.shutdown()
        notifyState(.stopped)
    }

    /// Sends one text message through links-client-core::send_text. The core
    /// persists the exact outbox envelope before the transport reports success.
    public func sendText(conversationID: String, recipientUserID: String, text: String) throws {
        guard Self.isValidTextID(conversationID), Self.isValidTextID(recipientUserID),
              !text.isEmpty, text.utf8.count <= Self.maximumTextBytes else {
            throw IOSMessagingError.invalidMessage
        }
        lock.lock()
        let sharedCore = core
        let manager = connection
        let ready = currentState == .ready && !coreFailed
        lock.unlock()
        guard let sharedCore, let manager, ready, manager.isConnected else {
            throw IOSMessagingError.notConnected
        }
        do {
            try sharedCore.sendText(
                conversationID: conversationID,
                recipientUserID: recipientUserID,
                text: text,
                transport: manager)
            scheduleRetry(for: manager)
        } catch {
            let issue = issue(for: sharedCore, error: error, fallback: .sendFailed)
            setState(for: issue)
            notifyFailure(issue)
            throw error
        }
    }

    /// Generate a private placeholder from normalized RGB pixels.
    public func encodeImageBlurHash(rgbPixels: Data, width: Int, height: Int) throws -> String {
        lock.lock()
        let sharedCore = core
        let ready = currentState == .ready && !coreFailed
        lock.unlock()
        guard let sharedCore, ready else { throw IOSMessagingError.notConnected }
        return try sharedCore.encodeImageBlurHash(
            rgbPixels: rgbPixels, width: width, height: height)
    }

    /// Encrypt normalized image bytes through the shared Rust core.
    public func encryptImage(_ image: Data, attachmentID: String, mimeType: String,
                             width: Int, height: Int, blurHash: String)
        throws -> IOSEncryptedImage {
        lock.lock()
        let sharedCore = core
        let ready = currentState == .ready && !coreFailed
        lock.unlock()
        guard let sharedCore, ready else { throw IOSMessagingError.notConnected }
        return try sharedCore.encryptImage(
            image, attachmentID: attachmentID, mimeType: mimeType,
            width: width, height: height, blurHash: blurHash)
    }

    /// Decrypt image ciphertext through the shared core before rendering.
    public func decryptImage(_ metadata: IOSImageMetadata, ciphertext: Data) throws -> Data {
        lock.lock()
        let sharedCore = core
        lock.unlock()
        guard let sharedCore else { throw IOSMessagingError.notConnected }
        return try sharedCore.decryptImage(metadata, ciphertext: ciphertext)
    }

    /// Send private image metadata only after the exact ciphertext upload receipt.
    public func sendImage(conversationID: String, recipientUserID: String,
                          metadata: IOSImageMetadata,
                          receipt: IOSImageUploadReceipt) throws {
        guard receipt.matches(metadata) else { throw IOSImageError.invalidUploadReceipt }
        lock.lock()
        let sharedCore = core
        let manager = connection
        let ready = currentState == .ready && !coreFailed
        lock.unlock()
        guard let sharedCore, let manager, ready, manager.isConnected else {
            throw IOSMessagingError.notConnected
        }
        try sharedCore.sendImage(
            conversationID: conversationID, recipientUserID: recipientUserID,
            metadata: metadata, receipt: receipt, transport: manager)
    }

    public func sendFile(conversationID: String, recipientUserID: String,
                         metadata: Data, receipt: IOSImageUploadReceipt) throws {
        let parsed = try IOSFileMetadata(protobuf: metadata)
        guard receipt.matches(parsed) else { throw IOSImageError.invalidUploadReceipt }
        lock.lock()
        let sharedCore = core
        let manager = connection
        let ready = currentState == .ready && !coreFailed
        lock.unlock()
        guard let sharedCore, let manager, ready, manager.isConnected else {
            throw IOSMessagingError.notConnected
        }
        try sharedCore.sendFile(
            conversationID: conversationID, recipientUserID: recipientUserID,
            metadata: metadata, transport: manager)
    }

    public func sendGroupFile(conversationID: String, metadata: Data,
                              receipt: IOSImageUploadReceipt) async throws {
        let parsed = try IOSFileMetadata(protobuf: metadata)
        guard receipt.matches(parsed) else { throw IOSImageError.invalidUploadReceipt }
        let (sharedCore, manager) = try readyCore()
        try await prepareGroupRecipients(conversationID: conversationID, core: sharedCore,
                                         directory: directoryPlaceholder(), preKeyAPI: preKeyPlaceholder())
    }

    /// Encode PCM with links-client-core's Opus profile. The shared core
    /// remains the only component that decides framing and codec settings.
    public func encodeVoiceNote(pcmFrames: [Int16], profile: IOSVoiceNoteProfile) throws -> Data {
        lock.lock()
        let sharedCore = core
        let ready = currentState == .ready && !coreFailed
        lock.unlock()
        guard let sharedCore, ready else { throw IOSMessagingError.notConnected }
        return try sharedCore.encodeVoiceNote(pcmFrames: pcmFrames, profile: profile)
    }

    /// Encrypt a complete Opus container with attachment metadata hidden in MLS.
    public func encryptVoiceNote(_ container: Data, attachmentID: String,
                                 durationMs: UInt64, profile: IOSVoiceNoteProfile)
        throws -> IOSEncryptedVoiceNote {
        lock.lock()
        let sharedCore = core
        let ready = currentState == .ready && !coreFailed
        lock.unlock()
        guard let sharedCore, ready else { throw IOSMessagingError.notConnected }
        return try sharedCore.encryptVoiceNote(
            container, attachmentID: attachmentID, durationMs: durationMs, profile: profile)
    }

    /// Verify the downloaded ciphertext through the shared core.
    public func decryptVoiceNote(_ metadata: IOSVoiceNoteMetadata, ciphertext: Data) throws -> Data {
        lock.lock()
        let sharedCore = core
        lock.unlock()
        guard let sharedCore else { throw IOSMessagingError.notConnected }
        return try sharedCore.decryptVoiceNote(metadata, ciphertext: ciphertext)
    }

    /// Decode verified Opus into PCM for AVAudioEngine playback.
    public func decodeVoiceNote(_ container: Data, profile: IOSVoiceNoteProfile) throws -> [Int16] {
        lock.lock()
        let sharedCore = core
        lock.unlock()
        guard let sharedCore else { throw IOSMessagingError.notConnected }
        return try sharedCore.decodeVoiceNote(container, profile: profile)
    }

    /// Send the media Message only after the uploader accepted the exact blob.
    public func sendVoiceNote(conversationID: String, recipientUserID: String,
                              metadata: IOSVoiceNoteMetadata,
                              receipt: IOSVoiceNoteUploadReceipt) throws {
        guard receipt.matches(metadata) else { throw IOSVoiceNoteError.invalidUploadReceipt }
        lock.lock()
        let sharedCore = core
        let manager = connection
        let ready = currentState == .ready && !coreFailed
        lock.unlock()
        guard let sharedCore, let manager, ready, manager.isConnected else {
            throw IOSMessagingError.notConnected
        }
        try sharedCore.sendVoiceNote(
            conversationID: conversationID, recipientUserID: recipientUserID,
            metadata: metadata, receipt: receipt, transport: manager)
    }

    /// Send private video/file metadata only after the exact ciphertext upload receipt.
    public func sendLargeFile(conversationID: String, recipientUserID: String,
                              metadata: IOSLargeFileMetadata,
                              receipt: IOSLargeFileUploadReceipt) throws {
        guard receipt.matches(metadata) else { throw IOSLargeFileError.invalidUploadReceipt }
        lock.lock()
        let sharedCore = core
        let manager = connection
        let ready = currentState == .ready && !coreFailed
        lock.unlock()
        guard let sharedCore, let manager, ready, manager.isConnected else {
            throw IOSMessagingError.notConnected
        }
        try sharedCore.sendLargeFile(
            conversationID: conversationID, recipientUserID: recipientUserID,
            metadata: metadata, receipt: receipt, transport: manager)
    }

    public func connectionManager(_ manager: IOSConnectionManager,
                                  didChange state: IOSConnectionManager.State) {
        lock.lock()
        guard connection === manager, !coreFailed || state == .stopped else {
            lock.unlock()
            return
        }
        let sharedCore = core
        let next: State
        switch state {
        // A deliberate stop after a typed core failure must not overwrite the
        // visible stale-cursor, authentication, or send-failure state.
        case .stopped: next = coreFailed ? currentState : .stopped
        case .connecting:
            next = currentState == .stopped || currentState == .connecting
                ? .connecting
                : .reconnecting
        case .ready:
            next = .ready
        case .failed:
            next = client.isAuthenticated ? .dependencyOutage : .authenticationRequired
        }
        currentState = next
        lock.unlock()
        notifyState(next)
        if state == .ready, let sharedCore {
            do {
                try sharedCore.retryOutbox(transport: manager)
                scheduleRetry(for: manager)
            } catch {
                let issue = issue(for: sharedCore, error: error, fallback: .sendFailed)
                setState(for: issue)
                notifyFailure(issue)
            }
        }
    }

    public func connectionManager(_ manager: IOSConnectionManager, didReceive frame: Data) {
        lock.lock()
        let sharedCore = core
        let active = connection
        let failed = coreFailed
        lock.unlock()
        guard let sharedCore, active === manager, !failed else { return }
        var committedMessages = [IOSReceivedTextMessage]()
        var committedImages = [IOSReceivedImageMessage]()
        var committedFiles = [IOSReceivedFileMessage]()
        do {
            _ = try sharedCore.handleServerFrame(
                frame, transport: manager, fullSync: false,
                onTextMessage: { message in
                    committedMessages.append(message)
                }, onImageMessage: { image in
                    committedImages.append(image)
                }, onFileMessage: { file in
                    committedFiles.append(file)
                })
            // The shared core returns only after its inbox/MLS/cursor commit
            // and QueueAck path have completed. Buffering here prevents a
            // callback queue from rendering during core processing.
            for message in committedMessages {
                notifyMessage(message)
            }
            for image in committedImages {
                notifyImage(image)
            }
            for file in committedFiles {
                notifyFile(file)
            }
            // Accepted frames can retire encrypted outbox entries. Refresh
            // the host's visible queue state after the core call completes.
            lock.lock()
            let nextState = currentState
            lock.unlock()
            notifyState(nextState)
        } catch {
            let issue = issue(for: sharedCore, error: error,
                              fallback: client.isAuthenticated
                                  ? .dependencyOutage : .authenticationExpired)
            // One undecryptable frame must not tear down the connection or
            // raise a modal. The sender can still deliver the next message.
            if issue == .dependencyOutage {
                notifyState(.ready)
                return
            }
            lock.lock()
            guard connection === manager else {
                lock.unlock()
                return
            }
            coreFailed = true
            currentState = state(for: issue)
            retryTimer?.cancel()
            retryTimer = nil
            lock.unlock()
            manager.stop()
            notifyState(state(for: issue))
            notifyFailure(issue)
        }
    }

    public func connectionManagerDidFail(_ manager: IOSConnectionManager) {
        lock.lock()
        guard connection === manager, !coreFailed else {
            lock.unlock()
            return
        }
        let issue: IOSMessagingIssue = client.isAuthenticated
            ? .dependencyOutage : .authenticationExpired
        if issue == .authenticationExpired {
            coreFailed = true
        }
        currentState = state(for: issue)
        lock.unlock()
        notifyState(state(for: issue))
        notifyFailure(issue)
        if issue == .authenticationExpired {
            manager.stop()
        }
    }

    public func connectionManagerDidDisconnect(_ manager: IOSConnectionManager) {
        lock.lock()
        guard connection === manager, !coreFailed else {
            lock.unlock()
            return
        }
        let nextState: State = currentState == .connecting ? .connecting : .reconnecting
        currentState = nextState
        lock.unlock()
        notifyState(nextState)
    }

    private func notifyState(_ state: State) {
        callbackQueue.async { [weak self] in
            guard let self else { return }
            self.delegate?.directMessaging(self, didChange: state)
        }
    }

    private func notifyMessage(_ message: IOSReceivedTextMessage) {
        callbackQueue.async { [weak self] in
            guard let self else { return }
            self.delegate?.directMessaging(self, didReceive: message)
        }
    }

    private func notifyImage(_ image: IOSReceivedImageMessage) {
        callbackQueue.async { [weak self] in
            guard let self else { return }
            self.delegate?.directMessaging(self, didReceive: image)
        }
    }

    private func notifyFile(_ file: IOSReceivedFileMessage) {
        callbackQueue.async { [weak self] in
            guard let self else { return }
            self.delegate?.directMessaging(self, didReceive: file)
        }
    }

    private func notifyFailure(_ issue: IOSMessagingIssue) {
        callbackQueue.async { [weak self] in
            guard let self else { return }
            self.delegate?.directMessagingDidFail(self, reason: issue)
        }
    }

    private func scheduleRetry(for manager: IOSConnectionManager) {
        lock.lock()
        let hasPending = connection === manager
            && !coreFailed
            && (core?.pendingRetryCount ?? 0) > 0
        lock.unlock()
        guard hasPending else { return }
        retryTimer?.cancel()
        let timer = DispatchSource.makeTimerSource(queue: coreQueue)
        timer.schedule(deadline: .now() + 1, repeating: 2)
        timer.setEventHandler { [weak self, weak manager] in
            guard let self, let manager else { return }
            self.retryPendingOutbox(on: manager)
        }
        retryTimer = timer
        timer.resume()
    }

    private func retryPendingOutbox(on manager: IOSConnectionManager) {
        lock.lock()
        guard connection === manager,
              currentState == .ready,
              !coreFailed,
              let sharedCore = core,
              manager.isConnected else {
            lock.unlock()
            return
        }
        lock.unlock()
        do {
            try sharedCore.retryOutbox(transport: manager)
            if sharedCore.pendingRetryCount == 0 {
                retryTimer?.cancel()
                retryTimer = nil
            }
        } catch {
            let issue = issue(for: sharedCore, error: error, fallback: .sendFailed)
            setState(for: issue)
            retryTimer?.cancel()
            retryTimer = nil
            notifyFailure(issue)
        }
    }

    private func setState(for issue: IOSMessagingIssue) {
        lock.lock()
        currentState = state(for: issue)
        lock.unlock()
        notifyState(state(for: issue))
    }

    private func state(for issue: IOSMessagingIssue) -> State {
        switch issue {
        case .staleCursor: return .staleCursor
        case .authenticationExpired: return .authenticationRequired
        case .dependencyOutage: return .dependencyOutage
        case .sendFailed: return .sendFailed
        }
    }

    private func issue(for sharedCore: any SharedClientCore, error: Error,
                       fallback: IOSMessagingIssue) -> IOSMessagingIssue {
        if let issue = sharedCore.messagingIssue {
            return issue
        }
        if error is IOSClientError || !client.isAuthenticated {
            return .authenticationExpired
        }
        return fallback
    }

    private static func isValidTextID(_ value: String) -> Bool {
        IOSClient.isCanonicalUUID(value)
    }
}

extension IOSConnectionManager: IOSCoreTransport {}

public enum IOSMessagingError: Error {
    case invalidMessage
    case notConnected
    case preKeyBootstrapUnavailable
    case staleCursorRecoveryUnavailable
    case groupsUnavailable
}
