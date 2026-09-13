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
              text.utf8.count <= IOSDirectMessaging.maximumTextBytes,
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

    public static let maximumTextBytes = 64 * 1024

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
