import Foundation

public enum IOSSurfaceKind: Equatable, Sendable {
    case channel
    case business
    case bot
}

public enum IOSSurfaceRole: Equatable, Sendable {
    case owner
    case admin
    case member
    case subscriber
    case bot
}

public struct IOSSurfaceProfile: Equatable, Sendable {
    public let surfaceID: String
    public let kind: IOSSurfaceKind
    public let role: IOSSurfaceRole
    public let displayName: String
    public let verified: Bool

    public init(surfaceID: String, kind: IOSSurfaceKind, role: IOSSurfaceRole,
                displayName: String, verified: Bool) throws {
        guard IOSClient.isCanonicalUUID(surfaceID), !displayName.isEmpty,
              displayName.utf8.count <= 80,
              !displayName.unicodeScalars.contains(where: {
                  $0.value < 0x20 || $0.value == 0x7f
              }),
              Self.roleAllowed(kind: kind, role: role) else {
            throw IOSMessagingError.invalidMessage
        }
        self.surfaceID = surfaceID
        self.kind = kind
        self.role = role
        self.displayName = displayName
        self.verified = verified
    }

    public var canSend: Bool {
        kind != .channel || role != .subscriber
    }

    public var canPublish: Bool {
        switch kind {
        case .channel, .business:
            return role == .owner || role == .admin
        case .bot:
            return false
        }
    }

    public var canManage: Bool {
        role == .owner || role == .admin
    }

    private static func roleAllowed(kind: IOSSurfaceKind, role: IOSSurfaceRole) -> Bool {
        switch kind {
        case .channel:
            return role == .owner || role == .admin || role == .subscriber
        case .business:
            return role == .owner || role == .admin || role == .member
        case .bot:
            return role == .bot
        }
    }
}

public protocol IOSChannelBusinessBotDelegate: AnyObject {
    func surfaceClient(_ client: IOSChannelBusinessBotClient,
                       didChange state: IOSChannelBusinessBotClient.State)
    func surfaceClient(_ client: IOSChannelBusinessBotClient,
                       didReceive message: IOSReceivedTextMessage)
    func surfaceClientDidFail(_ client: IOSChannelBusinessBotClient)
}

/// iOS host for channel, business, and bot surfaces over the shared core.
/// The profile gates UI actions; the injected core owns MLS, encryption,
/// replay, cursors, and delivery receipts.
public final class IOSChannelBusinessBotClient: IOSConnectionManagerDelegate {
    public enum State: Equatable, Sendable {
        case stopped
        case connecting
        case ready
        case failed
    }

    public static let maximumTextBytes = IOSInternalTextMilestone.maximumTextBytes

    public let surface: IOSSurfaceProfile
    private let client: IOSClient
    private let factory: any SharedClientCoreFactory
    private let endpoint: URL
    private weak var delegate: IOSChannelBusinessBotDelegate?
    private let callbackQueue: DispatchQueue
    private let coreQueue = DispatchQueue(
        label: "ai.links.ios.surface.core", qos: .userInitiated)
    private let lock = NSLock()
    private var core: (any SharedClientCore)?
    private var connection: IOSConnectionManager?
    private var currentState = State.stopped
    private var coreFailed = false

    public init(client: IOSClient, factory: any SharedClientCoreFactory, endpoint: URL,
                surface: IOSSurfaceProfile, delegate: IOSChannelBusinessBotDelegate,
                callbackQueue: DispatchQueue = .main) {
        self.client = client
        self.factory = factory
        self.endpoint = endpoint
        self.surface = surface
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
                try sharedCore.createHello(
                    accessToken: client.accessToken(),
                    lastSeenCursor: sharedCore.durableCursor())
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

    public func sendText(conversationID: String, text: String) throws {
        guard surface.canSend,
              IOSClient.isCanonicalUUID(conversationID),
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
        try sharedCore.sendSurfaceText(
            surfaceID: surface.surfaceID,
            conversationID: conversationID,
            text: text,
            transport: manager)
    }

    public func connectionManager(_ manager: IOSConnectionManager,
                                  didChange state: IOSConnectionManager.State) {
        lock.lock()
        guard connection === manager else {
            lock.unlock()
            return
        }
        switch state {
        case .stopped: currentState = coreFailed ? .failed : .stopped
        case .connecting: currentState = .connecting
        case .ready: currentState = .ready
        case .failed: currentState = .failed
        }
        let next = currentState
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
        var committedMessages = [IOSReceivedTextMessage]()
        do {
            _ = try sharedCore.handleServerFrame(
                frame, transport: manager, fullSync: false) { message in
                    committedMessages.append(message)
                }
            for message in committedMessages {
                notifyMessage(message)
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
            self.delegate?.surfaceClient(self, didChange: state)
        }
    }

    private func notifyMessage(_ message: IOSReceivedTextMessage) {
        callbackQueue.async { [weak self] in
            guard let self else { return }
            self.delegate?.surfaceClient(self, didReceive: message)
        }
    }

    private func notifyFailure() {
        callbackQueue.async { [weak self] in
            guard let self else { return }
            self.delegate?.surfaceClientDidFail(self)
        }
    }
}

public extension SharedClientCore {
    func sendSurfaceText(surfaceID: String, conversationID: String, text: String,
                         transport: any IOSCoreTransport) throws {
        try sendText(conversationID: conversationID, recipientUserID: surfaceID,
                     text: text, transport: transport)
    }
}
