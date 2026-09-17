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

public protocol IOSDirectMessagingDelegate: AnyObject {
    func directMessaging(_ messaging: IOSDirectMessaging,
                         didChange state: IOSDirectMessaging.State)
    func directMessaging(_ messaging: IOSDirectMessaging,
                         didReceive message: IOSReceivedTextMessage)
    func directMessagingDidFail(_ messaging: IOSDirectMessaging)
    func directMessagingDidFail(_ messaging: IOSDirectMessaging,
                                reason: IOSMessagingIssue)
}

public extension IOSDirectMessagingDelegate {
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
                let pending = sharedCore.pendingOutboxCount
                if pending > 0 {
                    notifyState(.reconnecting)
                }
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
        do {
            _ = try sharedCore.handleServerFrame(
                frame, transport: manager, fullSync: false) { message in
                    committedMessages.append(message)
                }
            // The shared core returns only after its inbox/MLS/cursor commit
            // and QueueAck path have completed. Buffering here prevents a
            // callback queue from rendering during core processing.
            for message in committedMessages {
                notifyMessage(message)
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
            let pending = sharedCore.pendingOutboxCount
            if pending > 0 {
                notifyState(.reconnecting)
            }
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
}
