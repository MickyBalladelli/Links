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
    public let senderDeviceID: String
    public let text: String
    public let sequenceID: UInt64
    public let sentAtMs: UInt64

    public init(conversationID: String, senderDeviceID: String, text: String,
                sequenceID: UInt64, sentAtMs: UInt64) throws {
        guard IOSClient.isCanonicalUUID(conversationID),
              IOSClient.isCanonicalUUID(senderDeviceID),
              !text.isEmpty,
              text.utf8.count <= IOSInternalTextMilestone.maximumTextBytes,
              sequenceID > 0 else {
            throw IOSMessagingError.invalidMessage
        }
        self.conversationID = conversationID
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
}

/// Connected iOS one-to-one text flow. Shared Rust core owns MLS, Sealed
/// Sender, durable outbox/inbox state, cursors and delivery receipts.
public final class IOSDirectMessaging: IOSConnectionManagerDelegate {
    public enum State: Equatable, Sendable {
        case stopped
        case connecting
        case ready
        case failed
    }

    public static let maximumTextBytes = IOSInternalTextMilestone.maximumTextBytes

    private let client: IOSClient
    private let factory: any SharedClientCoreFactory
    private let endpoint: URL
    private weak var delegate: IOSDirectMessagingDelegate?
    private let callbackQueue: DispatchQueue
    private let coreQueue = DispatchQueue(
        label: "ai.links.ios.messaging.core", qos: .userInitiated)
    private let lock = NSLock()
    private var core: (any SharedClientCore)?
    private var connection: IOSConnectionManager?
    private var currentState = State.stopped
    private var coreFailed = false

    public init(client: IOSClient, factory: any SharedClientCoreFactory, endpoint: URL,
                delegate: IOSDirectMessagingDelegate,
                callbackQueue: DispatchQueue = .main) {
        self.client = client
        self.factory = factory
        self.endpoint = endpoint
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
            callbackQueue: coreQueue)
        lock.lock()
        core = sharedCore
        connection = manager
        coreFailed = false
        currentState = .connecting
        lock.unlock()
        notifyState(.connecting)
        manager.start()
    }

    public func stop() {
        lock.lock()
        let manager = connection
        connection = nil
        core = nil
        coreFailed = false
        currentState = .stopped
        lock.unlock()
        manager?.stop()
        notifyState(.stopped)
    }

    public func shutdown() {
        lock.lock()
        let manager = connection
        connection = nil
        core = nil
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
        try sharedCore.sendText(
            conversationID: conversationID,
            recipientUserID: recipientUserID,
            text: text,
            transport: manager)
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

    public func connectionManager(_ manager: IOSConnectionManager,
                                  didChange state: IOSConnectionManager.State) {
        lock.lock()
        guard connection === manager, !coreFailed || state == .stopped else {
            lock.unlock()
            return
        }
        let next: State
        switch state {
        case .stopped: next = coreFailed ? .failed : .stopped
        case .connecting: next = .connecting
        case .ready: next = .ready
        case .failed: next = .failed
        }
        currentState = next
        lock.unlock()
        notifyState(next)
    }

    public func connectionManager(_ manager: IOSConnectionManager, didReceive frame: Data) {
        lock.lock()
        let sharedCore = core
        let active = connection
        let failed = coreFailed
        lock.unlock()
        guard let sharedCore, active === manager, !failed else { return }
        do {
            _ = try sharedCore.handleServerFrame(
                frame, transport: manager, fullSync: false) { [weak self] message in
                self?.notifyMessage(message)
            }
        } catch {
            lock.lock()
            guard connection === manager else {
                lock.unlock()
                return
            }
            coreFailed = true
            currentState = .failed
            lock.unlock()
            manager.stop()
            notifyState(.failed)
            notifyFailure()
        }
    }

    public func connectionManagerDidFail(_ manager: IOSConnectionManager) {
        lock.lock()
        guard connection === manager, !coreFailed else {
            lock.unlock()
            return
        }
        currentState = .failed
        lock.unlock()
        notifyState(.failed)
        notifyFailure()
    }

    public func connectionManagerDidDisconnect(_ manager: IOSConnectionManager) {
        lock.lock()
        guard connection === manager, !coreFailed else {
            lock.unlock()
            return
        }
        currentState = .connecting
        lock.unlock()
        notifyState(.connecting)
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

    private func notifyFailure() {
        callbackQueue.async { [weak self] in
            guard let self else { return }
            self.delegate?.directMessagingDidFail(self)
        }
    }

    private static func isValidTextID(_ value: String) -> Bool {
        IOSClient.isCanonicalUUID(value)
    }
}

extension IOSConnectionManager: IOSCoreTransport {}

public enum IOSMessagingError: Error {
    case invalidMessage
    case notConnected
}
